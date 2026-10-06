# T90: adding a property without searching the prototype chain every time (F368)

Status: done (2026-10-06) | Depends on: T89 | Local (not a cloud task: the user's cloud sessions
use the same plan limits)

## Goal
`o.k = v` where `o` has no own `k` (constructors: `this.car = car`, object building) runs `ordinary_set_named`,
which searches the object **and every object on its prototype chain** for `k` (a setter, or a read-only `k`,
changes the result) before appending. Cache, per `SetNamed` site, "from prototype P there's no `k` on the chain"
so a repeated add only checks the receiver; same results as node in every case (setters and read-only keys added
later, `setPrototypeOf`, `Object.defineProperty` on prototypes, frozen/non-extensible receivers, proxies).

## Why (measured; don't re-measure)
T88's perf of EarleyBoyer (laptop, `fixed 0.05`): `ordinary_set_named` → `find_own_named` 12.5% + `append_named`
2.8% self (~16%); reads `get_named` 18.7% self (F359, not this task). zigbee2mqtt (T84 perf, `t79/out-t83-perf`,
with shapes then): `find_own_named` 3.8%, property helpers ~25% in all. Current numbers: EarleyBoyer 10.4–12.2 s
(node 0.45 s); zigbee2mqtt replay 22.07 s (node 8.66 s), `t79/out-t89`.

## Read first
- `docs/design.md`: T74's paragraph (named helpers, `G_IC` sites, the per-site entry guess), T83–T86's note
  (shapes reverted), T89's note.
- `crates/backend/src/object.rs`: `set_named`, `ordinary_set_named`, `append_named`, `find_own_named`,
  `lookup_named`, `G_IC`/`ic_entry`/`ic_store` (sites are numbered by `crates/backend/src/lib.rs`, `ic_sites`).
- Branch **`shapes`** (8532edb), `crates/backend/src/object.rs`: T83's epoch machinery to port **without shapes**:
  `G_EPOCH` (runtime global), the `IS_PROTO` class flag (0x1000; the identity hash then moved from bit 12 to 13 in
  `mapset.rs`), `mark_chain`, `layout_changed`/`bump_epoch` at every layout change of a flagged object (add, delete,
  attribute change via `props.rs`, `integrity`, `set_proto`). `git show shapes:crates/backend/src/object.rs`.

## Plan
1. Port the epoch: `G_EPOCH`, `IS_PROTO`, `mark_chain`, and a bump at every layout change of a flagged object
   (adds, deletes, `defineProperty`/attribute changes, `preventExtensions`/`seal`/`freeze`, `setPrototypeOf`).
2. Per `SetNamed` site, in its `$ic` entry: the receiver's prototype `P` and the epoch tag, filled when the slow add
   found no `k` on the chain from `P` (after `mark_chain(P, null)`). A hit: receiver extensible, no own `k`
   (`find_own_named` on the receiver only, usually tiny), `o.proto == P`, `tag == G_EPOCH` → `append_named`.
3. Micro-benchmark `tasks/T90-files/` (constructors adding fields, object literals built by assignment, a setter
   added to a prototype later, a frozen receiver) vs node, before/after; EarleyBoyer before/after (laptop).
4. Tests: a fixture with the cases in the goal (`tests/fixtures/objects/add-property-chain.js`), the object/class
   fixture dirs, `wasm-tools validate`, smoke run, `cargo test`; then (ask first) full test262 on a-03 and one
   zigbee2mqtt replay against 22.07 s.

## Acceptance
- [x] Adds hit the cache; every invalidation case = node (fixture).
- [x] Micro-benchmark and EarleyBoyer before/after in Notes.
- [x] Full test262 0 regressions; one replay; docs (design.md, HISTORY.md, feasibility doc), follow-ups.

## Notes / Result
- What changed (`crates/backend`): `G_EPOCH` (i32, starts 1, bumps stay in [1, 2^30]) and `G_SET_IC` (`$vals`,
  per write site a prototype and an i31 tag; from 16 entries, doubling) in `runtime.rs`; `IS_PROTO` (0x1000;
  `mapset.rs` identity hash from bit 13); `layout_changed` in `define` (new key) and `set_proto`; helpers
  `MarkChain`, `SetIcStore`; `SetNamed` / `ordinary_set_named` take a site (`set_sites`, numbered apart from the
  reads' `ic_sites`). `ordinary_set_named` fills the cache only when the chain walk reaches the end (an inherited
  writable `k` doesn't fill: its attributes could change without a bump). `set_named`: an own writable data
  property in place; a hit (no own `k`, extensible, `o.proto == P`, tag == epoch) → `append_named`; else
  `ordinary_set_named`.
- Results: EarleyBoyer (laptop, default heap) median ~13.2 → ~12.2 s, `find_own_named` 12.5% → 2.0%. Micro-benchmark
  (`T90-files/adds.js`, 256 MiB heap): constructor adds −10–25%, literal adds −5%; default heap: GC-bound, equal.
  test262 (a-03, `~/t262-t90.log`): 28,099 pass, 0 regressions. Replay (`t79/out-t90`): 22.07 → 21.47 s
  (node 8.66 s), same output.
- Commands: `scripts/diff.sh` on `tests/fixtures/objects`, `classes`, `builtins/{map-set,weakmap,set-prototype-of}.js`;
  `cargo test`; `wasm-tools validate`; `scripts/bench.sh -r 3 octane-earley-boyer` (baseline from a worktree at
  83c660b); perf; on a-03 `~/t90-run.sh` (test262 `--update --causes 25 subset`, `build-p.sh`, `run-replay.sh`).
- Not done: computed-key adds (`o[k] = v`) and `DefineNamed` (literals) don't use the cache; updates of existing
  own properties still search (F372). Found: the default-heap GC cost of live runtime arrays (F371).
- Next smallest step: F338 (`Map` with string keys); for property speed, F359 (`get_named` 17.4% of EarleyBoyer
  now) or F372.

## Progress
- 2026-10-06: handoff written after T89 (user: F368 next, in a new session).
- 2026-10-06: started. Ported the epoch without shapes: `G_EPOCH` (starts 1, never 0), `G_SET_IC` (per write
  site the prototype), `IS_PROTO` 0x1000 (identity hash now from bit 13), `layout_changed` in `define` (a new
  key) and `set_proto`. Only adds and new prototypes bump: the cache records "no `k` at all on the chain" (filled
  only when the walk reaches the end, not on an inherited writable `k`), which deletes and attribute changes can't
  break. `SetNamed` takes a site (same counter as `GetNamed`); `set_named` writes an own writable data property
  directly, and appends when extensible + proto == site's + tag == epoch. Fixture
  `objects/add-property-chain.js` passes vs node.
- 2026-10-06: micro-benchmark `T90-files/adds.js` (n=100000, default heap): the first version was ~2× slower
  (ctor 426 → 808 ms, even `frozen`, which never hits). perf: GC-bound in both, `find_own_named` gone from the
  after profile (hits work); the cost was `G_SET_IC` as a ≥1,024-entry ref array (one per site of the shared
  `G_IC` numbering) live in every collection of wasmtime's copying collector on its small default heap. With
  `-O gc-heap-initial-size=256MiB` the same build was faster (ctor 58 → 47 ms). Fix: write sites numbered apart
  (`set_sites`), `G_SET_IC` = pairs (proto, i31 tag) from 16 entries, doubling; `G_IC` untouched. Default heap
  now = before (ctor 428 / 426 ms).
- 2026-10-06: micro-benchmark, n=1e6, `-O gc-heap-initial-size=256MiB`, 3 runs (before → after, ms): ctor
  594/604/699 → 515/439/626; literal 296/299/351 → 278/294/339; setter 188/190/220 → 188/204/232; frozen ~same.
  node: 8 / 7 / 37 / 463. Default heap: GC-bound, before = after.
- 2026-10-06: EarleyBoyer (`scripts/bench.sh -r 3 octane-earley-boyer`, laptop, default heap): before (worktree at
  83c660b) 13.09/13.63/13.19 s; after 11.83/12.29 (+ one 18.0 s run that overlapped a `cargo test`; rerun alone:
  10.26/12.19/12.64). Median ~13.2 → ~12.2 s. perf after: `find_own_named` 12.5% → 2.0%, `ordinary_set_named` out
  of the top; `set_named` 4.5%, `append_named` 2.5%; `get_named` 17.4% (F359) now the top.
  Tests: `objects/`, `classes/`, `builtins/{map-set,weakmap,set-prototype-of}.js` pass; `cargo test` ok.
  Waiting: full test262 on a-03 and one zigbee2mqtt replay (ask first).
- 2026-10-06: a-03 (user OK): synced the working tree; `~/t90-run.sh` (nohup, log `~/t90-run.log`) runs full test262 (`~/t262-t90.log`) then build-p.sh + one replay into `t79/out-t90` (`t79/t90.log`).
- 2026-10-06: a-03 done: test262 28,099 pass, 0 regressions (1,270 s); replay 21.47 s (T89 22.07), 13,478 publishes, same 9 error lines. Docs, follow-ups F371/F372, F368 ticked. Done.
