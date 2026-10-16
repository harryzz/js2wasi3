# T120: typed arrays' integer-indexed exotic methods for every key (F429)

Status: done | Depends on: T20, T48, T118 | Local (+ a-03 only after asking)

## Goal
F429 (found by T118's fixture): typed arrays handle only `i31` keys as elements. A probe (2026-10-15) against node
shows every other key going to the ordinary property path: `t["0"]`/`t["1"]` miss the elements (a write makes an
ordinary property); canonical numeric strings and numbers that aren't valid indices (`1.5`, `"-0"`, `10` past the
length, `-1`, `"NaN"`, `"Infinity"`, `1e21`) become ordinary properties instead of being ignored (read
`undefined`, `in` false, `defineProperty` TypeError); `delete t[0]` returns true (should be false);
`defineProperty` on an element or an invalid index never throws. The spec: ECMA-262 §10.4.5 (TypedArray exotic
objects: `[[GetOwnProperty]]`, `[[HasProperty]]`, `[[DefineOwnProperty]]`, `[[Get]]`, `[[Set]]`, `[[Delete]]`,
each first `CanonicalNumericIndexString` for a string key). The user chose F429 on 2026-10-15.

## Read first
- ECMA-262 §10.4.5, §7.1.21 (`CanonicalNumericIndexString`), `IsValidIntegerIndex`.
- `crates/backend/src/object.rs` `get`, `set`, `has`, `delete`, `define`/`getOwnPropertyDescriptor` paths;
  `crates/backend/src/typed.rs`.

## Plan
- [x] Measure: probe (`scratchpad/f429.js` vs node) and test262 `built-ins/TypedArrayConstructors/internals`
  (added to the laptop's sparse clone; not in the subset) on HEAD `03c518b`.
- [x] Design: one canonical-numeric-key test for a string key on a typed array, used by each internal method.
- [x] Fixture `builtins/typed-array-keys.js`; `scripts/diff.sh` on typed-array fixtures.
- [x] internals tests again; gate (the typed paths are hot in mandelbrot/revcomp/fasta) and full test262 on a-03 (ask).

## Progress
- 2026-10-15: task created (user: "Go f429"). Probe vs node: the differences above (`scratchpad/f429.js`).
- 2026-10-15: test262 `built-ins/TypedArrayConstructors/internals` (240 tests, added to the laptop's sparse clone
  with `git -C test262 sparse-checkout add`, not in the subset) on HEAD `03c518b`: 71 pass, 141 fail, 2 unsupported,
  26 skip (`out/t262-t120-internals-base.log`). Many failures aren't keys: `$262.detachArrayBuffer`, and the
  harness writing to `TypedArray.prototype` ("Cannot set properties of undefined", "Object.defineProperty called
  on non-object": no `%TypedArray%` constructor) — a follow-up.
- 2026-10-15: design: `object::Helper::TypedIndex` `(key) -> i64` (an index ≥ 0, -2 another canonical numeric
  key, -1 an ordinary key; strings: ArrayIndex, then "-0", then NumberToString(StringToNumber(s)) == s, only for
  strings starting with a digit, `-`, `I`, `N`) and `TypedPut` (TypedArraySetElement: convert, store if in range).
  Used by `own_get` (an element or nothing), `lookup` (a numeric key ends the chain walk), `has`, `delete`
  (an element: node's TypeError; another numeric key: true), `props::set` (OrdinarySet), `define_property`
  (invalid index: "Invalid typed array index"; accessor / false configurable/enumerable/writable: "Cannot
  redefine property: k"; then the value), `get_own_property_descriptor`. `object::named_key`: a constant key
  starting with a digit or `-`, or NaN/Infinity, takes the generic paths (no named-path cost). Lowering keeps
  StringToNumber in modules with typed arrays. Probe (now strict, as js2wasi3): all 23 keys × 9 operations = node.
- 2026-10-15: internals rerun (`out/t262-t120-internals.log`): 107 pass (was 71), 105 fail, 0 regressions. A symbol
  key was read as its description (`$symbol` is a `$string`): `typed_index` and `get`'s typed block now use
  `is_string` (HEAD read `t[Symbol("1")]` as element 1 too). Left (follow-ups): `$262` detach tests, no
  `%TypedArray%` (harness writes `TypedArray.prototype`), `[[Set]]` with another receiver (typed array on a prototype
  chain with setters further up, `Reflect.set`'s receiver, Proxy), `Reflect.defineProperty` on a non-extensible
  typed array, `Reflect.set` with a symbol key returns false.
- 2026-10-15: fixture `builtins/typed-array-keys.js` (37 keys × 10 operations, constant keys on typed arrays and on
  ordinary objects/arrays/literals, symbols, conversions once and in order, defineProperty descriptors, element
  descriptors/deletes, a shrunk resizable buffer, a typed array as a prototype, BigInt64Array) = node. Fixtures
  `builtins` 74 (1 skip), `objects` 19, `arrays` 9, `classes` 12, `syntax` 10 (1 skip), `node` 42 pass.
- 2026-10-15: laptop gate vs `03c518b` (`out/gate-t120-laptop.log`): every program's `.wasm` changes (nearly all have
  `$typed`, e.g. through the runtime's Buffer support); all same except splay −4.8% (instr −2.97%); binarytrees
  +0.76% instr, raytrace +0.47%. `own_get`'s typed test moved to its miss branch (elements are never entries):
  raytrace +0.13% (`out/gate-t120b-laptop.log`). binarytrees: with the lowering change reverted (no forced
  StringToNumber) its instructions equal base (58,758.95M vs 58,758.99M); with it, `perf record` by DSO: program code
  24,520M → 24,459M (−61M), wasmtime 32,241M → 32,458M (+217M: copying collector `collect_increment`/`forward`),
  libc memmove 1,992M → 2,285M (+293M): GC timing from the larger start (F304's grow-or-collect rule), as splay's −3%
  in the other direction; no program work added. Kept. Micro modules all differ (laptop times are noise).
- 2026-10-15: user: gate and test262 on a-03. Gate vs `03c518b` (`~/gate-t120.log`), then full test262 (`~/t262-t120.log`): started.
- 2026-10-15: test262 on a-03 (`out/t262-t120.log`): no changes since the baseline, 31,461 tests in 1,519 s. Gate
  (`out/gate-t120-a03.log`): splay −2.9% (instr −2.97%); marked slower: pidigits +3.1%, navier-stokes +3.2%,
  spectralnorm +2.1%, all at 0.00/−0.02% instructions (placement); binarytrees +1.0% (same; +0.77% instr, GC, above).
  Micro marked slower: `o[k]` plain +3, `o[k] = v` plain +2, `k in o` +2…+5, `Object.keys` function +9,
  `instanceof` +2, `v == 1 [boolean]` +2, two 2k-op regex cases. Laptop instructions per `object-kinds.js` case vs
  base: all within ±0.006M of 4–42G (placement). `primitives.js` whole file +390M (+0.36%), but each op alone (`==
  null`, `== 1`, `typeof`, BigInt `+`, `*`) ±0.01M; `perf record` by DSO: wasmtime +181M, libc +109M, compiled code
  +78M (≈ sampling precision): GC timing, as binarytrees. `regex.js` +0.03%.
- 2026-10-16: user accepted (a correctness fix, not a speed task): task done; commit and push.

## Notes / Result
- **What changed.** Typed arrays now follow ECMA-262 10.4.5 for every key, not only `i31`s.
  `object::Helper::TypedIndex` `(key) -> i64` sorts a key: a valid integer index (≥ 0), -2 for another canonical
  numeric key (`1.5`, `"-0"`, `-1`, `"NaN"`, `"Infinity"`, `1e21`: never a property), -1 for an ordinary key (symbols
  included: a `$symbol` is a `$string` holding its description). `TypedPut` is TypedArraySetElement (convert, store
  if in range). Used by `own_get` (on a `find` miss only: an element or nothing), `lookup` (a numeric key ends the
  chain walk), `has`, `delete` (an element: node's "Cannot delete property 'k' of [object Uint8Array]"),
  `props::set`, `define_property` ("Invalid typed array index", "Cannot redefine property: k"; then the value) and
  `get_own_property_descriptor`. `object::named_key`: constant keys starting with a digit or `-`, and NaN/Infinity,
  take the generic paths (the named paths pay nothing). Lowering links StringToNumber in modules with typed arrays.
- **Result:** the probe (23 keys × 9 operations) and the new fixture `builtins/typed-array-keys.js` = node; test262
  `built-ins/TypedArrayConstructors/internals` (laptop, not in the subset) 71 → 107 pass, no regressions
  (`out/t262-t120-internals-base.log`, `out/t262-t120-internals.log`); full test262 (a-03, `out/t262-t120.log`,
  1,519 s): no changes since the baseline (28,105 pass, 97.0%).
- **Gate (a-03, `out/gate-t120-a03.log`, vs `03c518b`):** not a speed change; run because the shared property
  helpers changed. splay −2.9%; pidigits +3.1%, navier-stokes +3.2%, spectralnorm +2.1% at equal instructions
  (placement); binarytrees +0.77% instructions, all in wasmtime's collector (GC timing from the forced
  StringToNumber); micro cases +2…+9 ns at equal instructions per case (placement), `primitives.js` +0.36%
  whole-file (GC). Accepted by the user (2026-10-16: a correctness fix).
- **Commands:** probe `scratchpad/f429.js`; `scripts/diff.sh` on `tests/fixtures/{builtins,objects,arrays,classes,
  syntax,node}`; `node scripts/test262.mjs --depth 5 --list built-ins/TypedArrayConstructors/internals` (laptop);
  `scripts/gate.sh -o out/gate-t120 03c518b` (laptop twice, a-03); `node scripts/test262.mjs subset` (a-03);
  `perf stat` / `perf record --sort dso` on binarytrees and the micro files.
- **Not done (follow-ups):** F430 `%TypedArray%` constructor, F431 typed `[[Set]]` with another receiver, F432
  `Reflect.defineProperty` on a non-extensible typed array, F433 `Reflect.set` with a symbol key.
- **Next smallest step:** choose among F420, F427, F404, F430.

