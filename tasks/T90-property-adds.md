# T90: adding a property without searching the prototype chain every time (F368)

Status: todo (handoff written 2026-10-06) | Depends on: T89 | Local (not a cloud task: the user's cloud sessions
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
- [ ] Adds hit the cache; every invalidation case = node (fixture).
- [ ] Micro-benchmark and EarleyBoyer before/after in Notes.
- [ ] Full test262 0 regressions; one replay; docs (design.md, HISTORY.md, feasibility doc), follow-ups.

## Notes / Result
(to be written)

## Progress
- 2026-10-06: handoff written after T89 (user: F368 next, in a new session).
