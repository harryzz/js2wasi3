# T34: array writes consult the prototype chain (F162)

Status: done | Depends on: T32, T33

## Goal
`a[i] = v` / `push` on a hole or past the end is [[Set]] (OrdinarySet): a setter found on the prototype chain is
called with the array as `this`, a read-only property there makes the write fail (TypeError in strict mode), and
only otherwise the array gets an own element. Today `array_set_index` always defines the element.

## Design
An array's chain is `Array.prototype` → `Object.prototype` unless `__proto__` changed it. A runtime flag
`G_INDEX_PROTO` (`(mut i32)`) is set when an index key is added to `Array.prototype` or `Object.prototype`
(`object::Helper::Define`'s new-entry path) or when an array's prototype is replaced (`set_proto`). Only while it is
set does `set_index` walk the chain for an absent element (index as a string key; `OwnGet` + the attribute of the
entry, or an array prototype's element attributes). `push` stays a store otherwise.

## Read first
F162 in `tasks/README.md`; T32/T33 Notes; `crates/backend/src/array.rs` (`set_index`), `crates/backend/src/object.rs`
(`define`, `set_proto`), `crates/backend/src/props.rs` (`set`).

## Tests
Fixture `tests/fixtures/arrays/writes-prototype.js`, `tests/fixtures/arrays`, `cargo test`, validate + wasmtime run;
test262 `built-ins/Array/prototype/{unshift,push,splice,shift,fill,copyWithin,reverse}` and the T33 set.

## Checklist
- [x] Flag + chain walk in `set_index`
- [x] Fixture
- [x] test262 rerun, follow-ups, baseline

## Notes / Result
**What changed**
- `runtime::G_INDEX_PROTO` (`(mut i32)`): set by `object::Helper::Define` when an index key is added to
  `Array.prototype` or `Object.prototype`, and by `set_proto` when an array's prototype is replaced. While it is set,
  `array_set_index` ([[Set]]) walks the chain for an absent element (hole or past the end): a setter there is called
  with the array as `this` (node's "which has only a getter" without one), a read-only data property throws, else
  the array gets its own element. Arrays that never meet such a prototype pay one global read per write to an absent
  element.
- [[Set]] vs CreateDataProperty are now separate: `array::Helper::SetIndex` / `DefineIndex` and
  `Define { set }`. `DefineIndex` replaces a configurable element with a plain data one and rejects a
  non-configurable one unless it is already a writable, enumerable data element ("Cannot redefine property: k"):
  this resolves F161. Array literals, spread and rest (`Push`), `object::Helper::Define` and `__intrinsic_define`
  (now with an array fast path for small integer keys) define.
- `Array.prototype.push` does `a[len + i] = args[i]`, then sets `length` to `len + argc` even when a setter took an
  element.
- The prelude builds its result and scratch arrays with definitions, never `[[Set]]`: `$$append(a, v)` replaces
  `push` in `json.js`, `string.js`, `math.js`, `map.js`, `object.js`; index writes on new arrays in `array.js`
  (`of`, `from`, `map`, `filter`, `concat`, `flat`, `splice`'s removed, sort buffers, `toReversed`, `with`,
  `toSpliced`) and the numeric scratch arrays in `math.js`/`string.js` use `__intrinsic_define`. Writes that are
  [[Set]] in the spec (`o[k] = …` in `fill`, `copyWithin`, `reverse`, `shift`, `unshift`, `splice`, `sort`,
  `Object.assign`) stay.

**Tests**: `tests/fixtures/arrays/writes-prototype.js` (= node; uses `JSON.stringify` while the prototype has index
setters/read-only indices), `holes-prototype.js`; whole `diff.sh`: 112 pass; `cargo test`, clippy; validate +
wasmtime run.

**test262**: `built-ins/{Array,JSON,String,Math,Map,Set}`, `language/statements/for-of`,
`language/expressions/{array,assignment/dstr}` (6,555 tests, 2,094 s at -j 4, `--update`): 6 fail → pass, 1 pass →
fail (`sort/precise-comparefn-throws.js`: the sort buffers still used [[Set]]; fixed, then `sort`/`toSorted` again:
2 → pass). Subset: 21,726 of 24,820 run pass (87.5%); `built-ins/Array` 2537 of 3030 (83.7%).

**Next smallest step**: no task proposed; F108 (generic `push`/`pop`/`join`) is the next array item.

## Progress
- 2026-09-27: task written for F162 at the user's request and started.
- 2026-09-27: flag G_INDEX_PROTO + chain walk in set_index; set_index split into [[Set]] (SetIndex) and CreateDataProperty (DefineIndex, also fixes F161: replaces configurable elements, rejects non-configurable); literals/spread/rest (Push helper), `__intrinsic_define` (array fast path) and the prelude's result/scratch arrays define (`$$append`, 77 sites in array/json/string/math/map/object.js); push sets length = len + argc. Fixture arrays/writes-prototype.js passes (with JSON.stringify inside the window).
- 2026-09-27: test262: +8, one regression found and fixed (sort buffers). Task done.
