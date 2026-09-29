# T47: Resizable `ArrayBuffer` and length-tracking typed arrays (part of F116)

Status: done (2026-09-29) | Depends on: T20 slice 1 (typed arrays, `ArrayBuffer`), T40 (BigInt), T45 (built-ins gated outside `Builtin::ALL`)

## Goal
`new ArrayBuffer(n, { maxByteLength })`, `resize`, `resizable`, `maxByteLength`, plus `transfer`,
`transferToFixedLength` and `detached`, as node 24.21.0 has them. Typed arrays over a resizable buffer:
- **length-tracking** (`new Uint8Array(rab)`, no length): `length`/`byteLength` follow the buffer;
- **fixed-length**: go out of bounds when the buffer shrinks below them (IsTypedArrayOutOfBounds: `length` 0, most
  methods throw a TypeError), and come back when it grows.

Programs that don't use these compile to the same bytes (the gating check of every task).

## What we know (at handoff)
- **test262 in the subset:** 88 tests with feature `resizable-arraybuffer`, all "unsupported". Nearly all are
  `built-ins/Array/prototype/*/resizable-buffer*.js` and `callbackfn-resize-arraybuffer.js`: the generic Array
  methods called on typed arrays over resizable buffers, 3–4 per method. They include
  `harness/resizableArrayBufferUtils.js`, and that's what blocks them today, before anything resizable:
  - `subClass(type)`: `new Function('return class My' + type + ' extends ' + type + ' {}')()`, code only known at
    run time (F191). The build rejects it ("`Function` of code not known at build time").
  - `typeof Float16Array` / `typeof BigInt64Array`: names we don't implement ("unsupported: global
    `Float16Array`"); node 24 has all three (`Float16Array`, `BigInt64Array`, `BigUint64Array`).
  - `MyUint8Array`, `MyFloat32Array`, `MyBigInt64Array`: subclassing typed arrays (F116).
- **Outside the subset**, for a later slice: `built-ins/ArrayBuffer` (221 files), `built-ins/TypedArray` (1,453),
  `built-ins/TypedArrayConstructors` (738), `built-ins/DataView` (561, needs `DataView`, F116).
- **Current implementation:**
  - `crates/backend/src/typed.rs` (768 lines): native `$buffer` (a fixed-size byte array) and `$typed` (buffer,
    offset, length, kind); element access and printing.
  - `crates/lower/builtins/typedarray.js` (700 lines): the constructors and methods on `Intrinsic::Typed`
    operations; `$$ArrayBuffer`.
  - `ArrayBuffer` and the typed arrays are `Builtin`s in `Builtin::ALL` (T20); NEW_ONLY in `builtins.rs`.
- **Spec points that decide the design:**
  - A resizable buffer's byte length changes in place; its identity and `maxByteLength` don't.
  - The spec lets an implementation reserve `maxByteLength` up front. node does (`resize` never moves data).
  - A typed array's `length`, `byteLength` and `byteOffset` are computed at each access
    (TypedArrayLength with IsTypedArrayOutOfBounds); element reads out of bounds give `undefined`, writes are
    ignored.
  - `transfer(len?)` moves the data to a new buffer (resizable if the source was) and detaches the source;
    `transferToFixedLength` always gives a fixed one. A detached buffer's `byteLength` is 0 and most operations
    throw.

## Design questions (decide first, record in docs/design.md: D1 has the T20 typed-array note)
1. **Storage.** Allocate `maxByteLength` bytes up front in `$buffer` and add a current byte length (plus
   `resizable`/`detached` flags), or reallocate on `resize`? *Recommendation:* reserve up front, as V8 does. Then
   element access never re-reads a moved array; only bounds change. Watch that fixed buffers keep today's layout
   and code (gating).
2. **Typed array views.** Add a length-tracking flag to `$typed` (length = (bufferByteLength − offset) / size,
   computed per access) and make every native accessor go through one "current length" helper. Measure the
   cost on a typed-array loop: fixed buffers should keep today's fast path (for example, a flag test only when
   the buffer is resizable).
3. **Detach** (`transfer`) needs the same checks as out-of-bounds; decide whether it's in this task (node has it;
   test262 `ArrayBuffer/prototype/transfer*`).
4. **Enabling the test262 harness:**
   - (a) implement `Float16Array`/`BigInt64Array`/`BigUint64Array` and typed-array subclassing (F116; big), and
     have the runner replace the harness's `subClass` helper with literal class declarations (a harness
     adaptation like T42's `globalThis.$DONE` shim, justified by the same "code built at run time" reason); or
   - (b) only the runner replacement, with `typeof Float16Array` giving `"undefined"` for unimplemented node globals.

   *Recommendation:* (a)'s runner part now, `BigInt64Array`/`BigUint64Array` if cheap (BigInt exists), and
   `Float16Array` as its own follow-up.

## Plan (slices; each ends with fixtures + a targeted test262 run)
1. Resizable buffers: constructor options, `resize`, `resizable`, `maxByteLength`, length-tracking and
   out-of-bounds typed arrays, with node's errors. Fixture `tests/fixtures/typed/resizable.js` (check the
   directory name in `tests/fixtures`).
2. `transfer`, `transferToFixedLength`, `detached`.
3. test262 enablement: the runner's `subClass` replacement, typed-array subclassing and BigInt64 arrays (or
   follow-ups). The 88 subset tests; then decide whether to add `built-ins/ArrayBuffer` to the subset.
4. Docs, follow-ups, baseline (targeted `--update` over tests with `resizable-arraybuffer`, `ArrayBuffer`,
   `TypedArray` in their source).

## Read first
- `docs/design.md`: D1's T20 note (typed arrays), D5 (the prelude), D7 (gating new built-ins like T45).
- `crates/backend/src/typed.rs`, `crates/lower/builtins/typedarray.js`.
- F116 in `tasks/README.md`; T20 slice 1 Notes / Result (`tasks/T20-wit-async-user-worlds.md`).
- `test262/harness/resizableArrayBufferUtils.js`; one test, for example
  `test262/test/built-ins/Array/prototype/map/resizable-buffer.js`.

## Tests
Fixtures (commands; node is the oracle via `scripts/diff.sh`), `cargo test`, `wasm-tools validate` plus a wasmtime
run. Gating: an old vs new build of every fixture is byte-identical, except F186's nondeterministic ones (the
scratchpad script `cmp.sh` from T42–T46 does this: build `old-js2wasi3` from the stashed tree). Measure a
typed-array loop before and after. test262: targeted `--update` runs; ask before a full run on `a-03`.

## Acceptance
- [x] Resizable buffers and length-tracking/out-of-bounds typed arrays behave like node's (fixtures).
- [x] `transfer`/`transferToFixedLength`/`detached` (or a follow-up with the reason).
- [x] The 88 subset tests pass or have a named follow-up; 0 regressions; baseline updated.
- [x] Programs without resizable buffers compile to the same bytes; typed-array speed unchanged.

## Notes / Result
**What changed**
- Design (docs/design.md, D5's T47 note): gated by `ir::Module::rab` (the user source contains `maxByteLength` or
  `transfer`); buffers reallocate on `resize` (not reserved: `maxByteLength` can be huge); in rab mode `$buffer` has a
  mutable `data`, `max`, `detached`, `$typed` has `tracking`, and every native length read goes through
  `typed::Length` / `Helper::Len`. New `TypedOp`s: `ViewTracking`, `Tracking`, `OutOfBounds`, `BufferNewResizable`,
  `BufferMax`, `BufferDetached`, `BufferResize`, `BufferTransfer`.
- Prelude (`builtins/typedarray.js`): `$$ArrayBuffer$resizable(length, options)` (aliased to `$$ArrayBuffer` in rab
  mode), getters `resizable`/`maxByteLength`/`detached`, `resize`, `transfer`, `transferToFixedLength`; constructors
  make length-tracking views and reject detached buffers; every `%TypedArray%.prototype` method but `subarray` and the
  getters checks IsTypedArrayOutOfBounds with node's message (`$$typedLive`); `byteOffset` 0 out of bounds;
  `subarray` through the constructor (tracking without `end`). All in `if (__intrinsic_rab_mode()) { … }` blocks
  (new gate next to `__intrinsic_bigint_mode`/`__intrinsic_regexp_mods`; dropped blocks leave the reachability scan).
- Backend: array iterators throw for out-of-bounds views; `%TypedArray%.prototype.values/keys/entries` are their own
  functions in rab mode; a detached buffer prints `ArrayBuffer { (detached), [byteLength]: 0 }`.
- Subclassing `Map`/`Set`/`ArrayBuffer`/typed arrays (F116/F106 part): `func::Helper::ConstructFrom`, gated on
  `ir::Module::subclassing` (`extends` or the name `construct` in the user's code).
- Runner: `HARNESS_ADAPT` in `scripts/test262.mjs` (see HISTORY.md).
- Fixtures: `tests/fixtures/builtins/{resizable-buffers,buffer-transfer,subclass-collections}.js` (node is the oracle).

**Commands run**: `scripts/diff.sh` on the new fixtures and `typed-arrays*.js`, `reflect.js`; `cargo test`;
`wasm-tools validate` + `wasmtime run -Sp3`; scratchpad `cmp.sh` old vs new over all 240 fixtures (identical except
the three new ones and `reflect.js`, which does `Reflect.construct(Map, …)`: the subclassing gate); a 1M-element
`Float64Array` read loop (3.1–3.5 s in every mode: noise); test262 targeted `--update` over 917 tests: +99 pass,
0 regressions, subset 27,152 of 28,512 run (95.2%).

**Unsupported / follow-ups**: F216 (subclass instances print with the built-in's name), F217 (`BigInt64Array`/
`BigUint64Array`/`Float16Array`; 3 tests), F218 (`typeof Symbol`; 13 tests), F219 (re-validation after resizing
callbacks in `%TypedArray%` methods, `maxByteLength` limit, `byteLength` getter message, shared `values` without rab).

**Next smallest step**: F217's `BigInt64Array`/`BigUint64Array` (kinds 9–10 in bigint mode: element get/set with
ToBigInt, printing `1n`, the prelude methods' `+value` conversions), then drop their `HARNESS_ADAPT` entries.

## Checklist
- [x] design questions decided and recorded
- [x] slice 1: resizable buffers, length-tracking views
- [x] slice 2: transfer / detached
- [x] slice 3: test262 harness, subclassing (BigInt64 arrays: F217)
- [x] test262 + baseline, follow-ups, docs (HISTORY.md, keywords.md, design.md; README's status unchanged)

## Progress
- 2026-09-29: handoff written after T46 (the user chose resizable `ArrayBuffer` next). State at handoff: `main` at
  `3aee035`, pushed; test262 subset 27,053 of 28,512 run pass (94.9%).
- 2026-09-29: started. Tools unchanged (`check-tools.sh` all ok). Probed node 24.21.0: `maxByteLength` up to
  2^33−1 is accepted (virtual reservation), `resize` beyond max / negative: `RangeError: ArrayBuffer.prototype.resize:
  Invalid length parameter`; on a fixed buffer: `TypeError: Method ArrayBuffer.prototype.resize called on incompatible
  receiver #<ArrayBuffer>`; `new Uint32Array(rab6)` is length-tracking with no multiple-of-size check; every
  `%TypedArray%.prototype` method but `subarray` and the getters throws `TypeError: Cannot perform
  %TypedArray%.prototype.<m> on a detached ArrayBuffer` when out of bounds (V8 quirks: `findLastIndex` says
  `findIndexLast`, `toString` says `join`); getters give 0; a detached buffer prints `ArrayBuffer { (detached),
  [byteLength]: 0 }`.
- 2026-09-29: design decided. (1) Storage: **reallocate on resize**, not reserve: `maxByteLength` can be 2^33−1 and
  a GC byte array is real memory; element access already reads `buffer.data` on every access, so a moved array costs
  nothing there. Byte length stays `array.len(data)`. (2) Gated by `ir::Module::rab` (the user's code names property
  `maxByteLength`, `transfer` or `transferToFixedLength`): then `$buffer` gets a mutable `data` plus `max: i32` (−1:
  fixed) and `detached: mut i32`, `$typed` gets `tracking: i32`, and every native reader of a typed array's length
  calls `typed::Helper::Len` (IsTypedArrayOutOfBounds → 0; length-tracking: (bytes − offset) / size); without it the
  types and code are today's (no speed change for fixed buffers). The prelude's checks go in
  `if (__intrinsic_rab_mode()) { … }` blocks (like `__intrinsic_bigint_mode`). (3) `transfer`/detach are in this
  task (slice 2; the same Len helper covers detached buffers: length 0).
- 2026-09-29: slice 1 implemented: IR `Module::rab` + `TypedOp::{ViewTracking, Tracking, OutOfBounds,
  BufferNewResizable, BufferMax, BufferDetached, BufferResize, BufferTransfer}`; `Prelude::rab` (source has
  `maxByteLength`/`transfer`) aliases `$$ArrayBuffer` to `$$ArrayBuffer$resizable(length, options)` and keeps the
  `__intrinsic_rab_mode()` blocks (dropped blocks are also left out of the reachability scan, like T46's);
  backend `typed::Length` (field or `Helper::Len`) used by `object.rs` (get/set/has/own keys), `wit.rs`, printing;
  the array iterators throw node's TypeErrors for out-of-bounds typed arrays; getters `resizable`/`maxByteLength`/
  `detached`. node's `maxByteLength` limit is machine-dependent (68,753,477,599,232 here): only ToIndex's range is
  checked. Fixture `tests/fixtures/builtins/resizable-buffers.js` passes `scripts/diff.sh` (1.5 s debug build);
  output validates.
- 2026-09-29: slice 2 (`transfer`, `transferToFixedLength`, `detached`) done in the same prelude/backend pieces;
  fixture `tests/fixtures/builtins/buffer-transfer.js` passes. `subarray` goes through `$$typedCreate` in rab mode
  (the constructor's checks). Gating check (scratchpad `cmp.sh`, old vs new build of every fixture): identical except
  the new fixture.
- 2026-09-29: slice 3: subclassing `Map`/`Set`/`ArrayBuffer`/typed arrays (F116 part): the constructor values'
  construct entry is `func::Helper::ConstructFrom(b)` (the prelude code, then `newTarget.prototype`), gated on
  `ir::Module::subclassing` (user code has `extends` or names `construct`), so `typed-arrays.js` (ctor values, no
  classes) keeps its bytes. Separate `%TypedArray%.prototype.values/keys/entries` in rab mode
  (`iter::Helper::TypedProto`: ValidateTypedArray; `Array.prototype.values` on an out-of-bounds view doesn't throw).
  Runner: `HARNESS_ADAPT` in `scripts/test262.mjs` rewrites `resizableArrayBufferUtils.js` (literal subclasses
  instead of `new Function`, Float16/BigInt64 arrays reported absent). The 89 `resizable-arraybuffer` tests: 73 pass
  (5 "timeouts" at `-j 8` pass at `-j 2`), 11 name `Float16Array` and 3 `BigInt64Array` themselves.
- 2026-09-29: `ConstructFrom` keeps the built-in's prototype when `newTarget.prototype` isn't an object (fixture
  `tests/fixtures/builtins/subclass-collections.js` passes). `testTypedArray.js` also adapted in the runner; its 13
  tests are then blocked by `typeof Symbol` (F106 → F218). Benchmark (1M `Float64Array` reads, release build): fixed
  buffer 3.1–3.2 s (bytes identical to before), same program in rab mode 3.35 s, a tracking view 3.2–3.5 s: within
  noise. `cargo test` passes. Docs (design.md T47 note, keywords.md rows) and follow-ups F216–F219 written.
