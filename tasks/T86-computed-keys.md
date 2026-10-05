# T86: computed-key and `.length` property access (F363)

Status: in progress (2026-10-05) | Depends on: T85 | Local

## Goal
T84's profile: `get` (the generic, uncached [[Get]] with ToPropertyKey and a string hash per object on the chain) is
15.3% inclusive on the zigbee2mqtt replay. Make the biggest parts cached, same results as node.

## Rule (user, 2026-10-05)
One replay on a-03 after this task: ≤ 21.5 s keeps shapes (T83, T85, T86); otherwise revert all the shapes work.

## Why (T84 perf, `t79/out-t83-perf`; don't re-measure)
Direct callers of `get` (perf script, % of all samples): `get_length` 4.72% (`.length` of Buffers: from Buffer's
`copy` 0.79, `writeOrBuffer` 0.62, `slice` 0.53, `consume` 0.30, `_offset` 0.21, `readUInt8` 0.19, …), then many
sites under 0.7% each (`$$reSearch` 0.66, `_write` 0.60, `writeOrBuffer` 0.58, `iter_step` 0.58, `$$reCompile` 0.51,
`instanceof_value` 0.51, `clearBuffer` 0.47, …).

## Plan
1. `.length` of a typed array without own properties: a prototype-hit cache (`G_LEN_IC`), validated like T83's
   site caches (prototype identity + `G_EPOCH`), the getter called live.
2. Then, if worth it: the next largest computed paths (stream state reads, `instanceof`, iterator steps).
3. Tests: Buffer/typed array fixtures, a guard test (own `length`, `setPrototypeOf`, getters added/replaced on
   `Buffer.prototype` and `%TypedArray%.prototype`), `wasm-tools validate`, smoke run, `cargo test`.

## Acceptance
- [ ] Micro-benchmark before/after; fixtures and guard test pass.
- [ ] One replay on a-03 (ask first); the rule applied; docs and follow-ups.

## Notes / Result
(to be written)

## Progress
- 2026-10-05: task started. Callers of `get` aggregated from T84's `perf script`: `.length` of Buffers is the one
  concentrated part (4.7%); the rest is spread thin.
- 2026-10-05: `get_length`: typed arrays without own properties look `length` up in `G_LEN_IC` (new global; 4 `$ic`
  entries per prototype, newest first; filled by walking the chain with `find_own_named` and `mark_chain`, so a
  layout change of a prototype bumps `G_EPOCH`). One entry thrashed between Buffer and Uint8Array (length 585 →
  743 ms); 4 entries: `tasks/T86-files/length-bench.js` (min of 5, ms; node / before / after): length 2/585/279,
  read 12/2129/1668, slice-copy 12/624/504, concat 12/238/198. Guard test (own `length`, `setPrototypeOf`, a getter
  on `Buffer.prototype` added and deleted, `%TypedArray%.prototype.length` replaced and restored): = node.
- 2026-10-05: the next part of `get` was `state.length` on plain objects (readable-stream's `writeOrBuffer`): the
  compiler never treats `length` as a named key, so reads went `get_length` → `get` and writes generic `set`.
  `get_length`: a shaped receiver (always an ordinary object) finds its own `length` by the constant's hash; new
  `SetLength` helper + lowering for `v.length = x`: a shaped receiver's own writable data `length` stored directly,
  else `set`. Bench `state` case (plain object, `length +=` / reads): 212 → 91 ms (node 2). All: concat 261→192,
  length 623→218, read 2216→1549, slice-copy 670→467.
- 2026-10-05: tests: `typed-length-cache.js` (new fixture: typed guards + plain `length` reads/writes, arrays,
  accessors, a setter on the prototype, frozen/read-only in strict mode, classes) = node; `scripts/diff.sh` arrays/,
  objects/, classes/, generators/ and the Array/typed/Buffer/shapes fixtures: 42 pass; `cargo test -p
  js2wasi3-backend -p js2wasi3` ok; test262 `built-ins/Array/length`, `Array/prototype/push`: 0 regressions;
  `Function/prototype/bind` (not in the subset): the same 10 failures with the T83 compiler. Found: F364 (message).
