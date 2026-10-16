# T126: subclasses of built-ins (F106/F108)

Status: done | Depends on: T25, T35, T47, T121 | Local (+ a-03 only after asking)

## Goal
F106/F108 (subclassing part): `class X extends <built-in>` where the prelude constructors ignore `new.target`.
T47 did Map/Set/ArrayBuffer/typed arrays. User chose F106/F108 on 2026-10-16. (F106's other items, native
statics as properties of constructor values, stubs, `Math.x =`, and F108's iterator helpers stay open.)

## Plan
- [x] Probe every built-in subclass against node (`scratchpad/t126/sub.js`, `sub2.js`, `show.js`).
- [x] Fix what differs; fixture `classes/builtin-subclass.js`; fixture directories; gate (laptop; prelude paths of
  every program changed); full test262 on a-03 (ask).

## Progress
- 2026-10-16: task created. Probe: Error (incl. TypeError, AggregateError, `cause`), Map, Set, Date, String,
  Number, Boolean subclasses already = node. Different: `extends Array` (instances not the subclass; `map`/`of`
  results), `extends RegExp` and `extends Object` (instances not the subclass), `extends Promise` (`then` results
  `Promise`: species not modelled, `promise.rs`), `extends Function` (needs `new Function(code)` at run time:
  refused by design), and `console.log` of subclass instances (node: `Stack(3) [ … ]`, `M(1) [Map] { … }`,
  `U8(2) [Uint8Array] [ … ]`, `AB [ArrayBuffer] { … }`, `R /x+/g`, `P [Promise] { … }`).
- 2026-10-16: fixes: `func.rs` `ConstructFrom` (T47's `new.target` prototype after the prelude constructor) also
  for `Array`/`Object`/`RegExp` when the module has subclassing; a new mode intrinsic `__intrinsic_subclass_mode()`
  (prelude blocks kept only in such modules, `Module::subclassing`); `$$speciesCreate` takes the species path for an
  array whose prototype isn't `Array.prototype` in that mode (first version took it for every array: a program
  with subclassing but no `Array` value trapped reading the stub `Array.prototype.constructor`,
  `builtins/object-methods.js`; fixed); `$$Array_of` uses `this` as the constructor (as `Array.from`); a
  `$$Array_$species` getter (T121's `$name` statics), rooted also for subclassing; `inspect.rs`: a
  `SubclassName` helper rewrites `Map(1) {…}` → `M(1) [Map] {…}` etc. for typed arrays, buffers, Maps/Sets, arrays
  get `Name(n) `, RegExps `Name ` (all only in modules with subclassing). Probes = node except Promise species and
  display, `extends Function`, `console.log(error)`'s stack (every error, not subclass-specific).
- 2026-10-16: fixture `classes/builtin-subclass.js` (the three probes, each in a block) = node. `cargo test`
  passes; fixtures `classes` 13, `builtins` 76, `arrays`, `objects`, `closures`, `syntax`, `async`, `promises`,
  `exceptions`, `generators`, `globals`, `modules`, `commonjs`, `node` pass (`node/` fetch, http, http-server,
  stream-errors timed out under the parallel run, pass alone).
- 2026-10-16: after the species guard: `builtins` 76 (incl. `object-methods.js`), `arrays` 9, `classes` 13 pass.
  Laptop gate vs `d3de5dc` (`out/gate-t126-laptop.log`): every program's `.wasm` changed (the prelude's
  `$$speciesCreate`/`$$Array_of` shapes), all 18 at equal instructions (±0.00%), times laptop noise; micro modules
  differ the same way. Next: full test262 on a-03 (ask).
- 2026-10-16: user: yes. Full test262 on a-03 started (`~/t262-t126.log`).
- 2026-10-16: test262 (a-03, `out/t262-t126.log`): 28,105 → 28,119 pass (+14: `Array[Symbol.species]` ×4, RegExp
  named groups with a subclass ×2, `subclass-builtins` Array/Object/RegExp ×6, `builtin-objects/Object` ×2), 0
  regressions. Baseline updated for those 14 on the laptop (`node scripts/test262.mjs --update <the 14>`,
  `out/t262-t126-update.log`). Task done.

## Notes / Result
- **What changed:** `Array`/`Object`/`RegExp` construct through `ConstructFrom` in modules with subclassing (the
  instance gets `new.target.prototype`); `__intrinsic_subclass_mode()` (prelude blocks only in those modules);
  ArraySpeciesCreate for subclass instances, a `Array[Symbol.species]` getter, `Array.of` honours `this`;
  `console.log` names subclass instances as node (`Stack(3) [ … ]`, `M(1) [Map] { … }`, `U8(2) [Uint8Array] [ … ]`,
  `AB [ArrayBuffer] { … }`, `R /x+/g`).
- **Result:** fixture `classes/builtin-subclass.js` = node; `cargo test`; fixture directories pass; laptop gate:
  equal instructions in all 18; full test262 +14, no regressions.
- **Not done (follow-ups):** F438 Promise species (`then`/statics on a subclass) and `Promise.prototype[Symbol.toStringTag]`
  (`P [Promise] { … }`); `extends Function` needs code at run time (refused by design); `console.log(error)` prints
  `[E: m]` where node prints the stack (all errors). F106's other items and F108's iterator helpers stay open.
- **Next smallest step:** choose: F407 (raytrace), F438, F436 when wasmtime 50.0.0 is final.

