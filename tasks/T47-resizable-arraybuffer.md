# T47: Resizable `ArrayBuffer` and length-tracking typed arrays (part of F116)

Status: todo (handoff written 2026-09-29, not started) | Depends on: T20 slice 1 (typed arrays, `ArrayBuffer`), T40 (BigInt), T45 (built-ins gated outside `Builtin::ALL`)

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
- [ ] Resizable buffers and length-tracking/out-of-bounds typed arrays behave like node's (fixtures).
- [ ] `transfer`/`transferToFixedLength`/`detached` (or a follow-up with the reason).
- [ ] The 88 subset tests pass or have a named follow-up; 0 regressions; baseline updated.
- [ ] Programs without resizable buffers compile to the same bytes; typed-array speed unchanged.

## Notes / Result
(to be written)

## Checklist
- [ ] design questions decided and recorded
- [ ] slice 1: resizable buffers, length-tracking views
- [ ] slice 2: transfer / detached
- [ ] slice 3: test262 harness, subclassing / BigInt64 arrays
- [ ] test262 + baseline, follow-ups, docs (HISTORY.md, keywords.md, design.md, README)

## Progress
- 2026-09-29: handoff written after T46 (the user chose resizable `ArrayBuffer` next). State at handoff: `main` at
  `3aee035`, pushed; test262 subset 27,053 of 28,512 run pass (94.9%).
