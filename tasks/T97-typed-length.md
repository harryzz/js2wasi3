# T97: a typed array's / Buffer's `.length` without the getter call (F382)

Status: done (2026-10-09, cloud session; test262 on a-03 pending: F384) | Depends on: T96 | Cloud

## Goal
`buf.length` / `u8.length` cost ~250 ns (node 2–4 ns): `get_length` sends them to `get`, which walks Buffer.prototype
→ Uint8Array.prototype → `%TypedArray%.prototype` and calls the prelude getter `$$TypedArray_prototype_length`
through `call_value` (which checks `this` and calls `__intrinsic_typed_length`). zigbee2mqtt reads Buffer lengths
constantly.

## Plan
1. Snapshot (as T76's `G_REGEXP_SNAP`): when `%TypedArray%.prototype` is built, `G_TLEN[0..2]` = `length`'s entry
   index and getter.
2. `get_length` fast path: a `$typed` receiver with no own properties, whose prototype is one of two cached ones
   with the current `G_EPOCH` (`G_TLEN[2..6]`), and `%TypedArray%.prototype`'s entry still `length` with the
   snapshot getter → the length field (`typed::Length`). A redefined or deleted getter fails the entry check.
3. Cache fill (`typed_length_fill(p)`): walk from `p` to `%TypedArray%.prototype` with no `length` on the way, then
   `mark_chain(p)` (T90) so a `length` added to any of them later bumps `G_EPOCH`.
4. Measure `T96-files/nonplain.js` (`buf.length`, `u8.length`), plain objects; tests: typed-array fixtures, full
   `diff.sh`, `cargo test`, validate, smoke; test262 on a-03 (typed arrays redefine `length` in tests).

## Acceptance
- [x] Before/after in Notes; results = node (new fixture `builtins/typed-length.js`); test262: pending (F384).
- [x] Docs, follow-ups (F382 ticked, F384).

## Notes / Result
**What changed** (`object.rs`, `runtime.rs`):
- `G_TLEN` (`(ref null $vals)`, 6 slots): `[0]` `length`'s entry index in `%TypedArray%.prototype` and `[1]` its
  getter, recorded by `typed_length_snapshot` when that prototype is built (before user code, as T76's
  `G_REGEXP_SNAP`); `[2..6]` two cached prototypes with the `G_EPOCH` they were checked at.
- `typed_length_fill(p)`: the chain from `p` reaches `%TypedArray%.prototype` with no other `length` → `mark_chain(p)`
  (T90's `IS_PROTO`, so a `length` added there later bumps `G_EPOCH`) and cache `p` (the older slot gives way).
- `get_length`: a `$typed` receiver with no own properties, a cached prototype at the current epoch (or filled
  now), and `%TypedArray%.prototype`'s entry still `length` with the snapshot getter → the length field
  (`typed::Length`, the intrinsic the getter calls). Anything else: `get` as before.

**Measurements** (cloud, 3 alternating rounds, `T96-files/nonplain.js`, 2M reads): `buf.length` 522–684 → 44–80 ms,
`u8.length` 515–583 → 44–52 ms (~12×; node 4–12 ms). Outputs identical. Plain objects untouched (only
`get_length`'s non-array, non-string branch changed).

**Correctness**: `tests/fixtures/builtins/typed-length.js`: own `length` on an instance, a getter on a subclass
prototype and on `Uint8Array.prototype` (then deleted), `%TypedArray%.prototype`'s getter redefined, deleted and
restored, `setPrototypeOf` away and back, Buffer and Uint8Array receivers alternating: same as node. (Node's own
`console.log` stops writing while `Uint8Array.prototype.length` or the built-in getter is overridden, so the
fixture collects results and prints after restoring.)

**Commands**: full `scripts/diff.sh -j 4`: 248 pass, 19 skip, 2 known sandbox failures; `cargo test -p
js2wasi3-backend` ok; `cargo test -p js2wasi3` 30 pass, 4 `wac` (tool missing); validate + smoke.

**Not done**: full test262 (F384; test262 redefines typed `length` in places). `byteLength`/`byteOffset` getters
take the slow path still (not in a profile).

**Next smallest step**: F384 on a-03.

## Progress
- 2026-10-09: task created (user: F382). Design from T76's RegExp snapshot + T90's epoch.
- 2026-10-09: implemented snapshot + fill + fast path; validates; buf.length 522–684 → 44–80 ms, u8.length
  515–583 → 44–52 ms. Fixture typed-length.js (node needed results printed after restoring: its console.log breaks
  while Uint8Array.prototype.length is overridden): pass. Full suite 248 pass; cargo test ok (wac missing). Done.
