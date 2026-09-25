# T13: Objects and prototypes

Status: done | Depends on: T12

## Goal
Plain objects in dictionary mode (D1): literals, get/set/delete, `in`, computed keys, prototype chain lookup,
`Object.create(proto)`, and property enumeration order (integer keys, then strings in insertion order).

## Out of scope
Getters/setters, `Object.defineProperty` (list them as follow-ups), hidden classes.

## Acceptance
- [x] Fixtures `objects/*.js` match node. (9 fixtures: access, conversions, delete-in, enumeration, errors, literals,
  many, prototypes, strict.)

## Progress
- 2026-09-25: started (same session as T12, with the user's approval). Tools unchanged. Baseline: `cargo test` 54 ok,
  `diff.sh` 12 passed / 8 skipped.
- 2026-09-25: decisions.
  1. **Layout** (new `crates/backend/src/object.rs`): `$object { class: i32, proto: mut (ref null $object), keys: mut
     (array (mut (ref null $string))), vals: mut (array (mut eqref)), attrs: mut (array (mut i8)), count: mut i32,
     index: mut (ref null (array (mut i32))) }`. Entries in insertion order (a deleted entry's key is null); lookups
     scan linearly up to 8 entries, then use an open-addressing hash index (FNV-1a over UTF-16 units, entry + 1 per
     slot). Keys are strings (no symbols yet): ToPropertyKey is ToString. `attrs` bit 0: not enumerable.
  2. **Built-in prototypes** as lazily created objects: Object, String, Number, Boolean, Error and the 6 native error
     prototypes, each holding its real property names. Methods (not implementable before functions, T14/T21) hold a
     sentinel oddball: `in` and shadowing see them, `for-in` skips them (not enumerable), **reading one traps**
     (`builtin_not_implemented`), so `typeof o.toString` can't silently be `"undefined"`. Primitives look properties up on
     their prototype (strings also have `length` and index properties).
  3. **Errors become objects** (F32): class `error`, prototype `<Kind>.prototype`, own non-enumerable `message` (if
     given), `cause` (from an options object) and `stack` (sentinel: reading it traps, F31). `name`/`message` are plain
     property reads now. ToPrimitive of an object: if `valueOf`/`toString` are the built-ins, `Error.prototype.toString`
     or `[object Object]`/`[object Error]`; a user-replaced one traps (callable values: T14). ToNumber of that string
     is NaN when it can't be numeric, else the F20 trap.
  4. **Supported syntax**: object literals (plain/string/number/computed keys, shorthand, `__proto__: v`, spread),
     `o.k`, `o[k]`, `o?.k`, assignment/compound/`++` on members, `delete`, `in`, `for-in` (own then inherited
     enumerable keys; integer keys ascending first; keys deleted during the loop are skipped), `instanceof` with
     `Object` or a native error constructor, `Object.create`, `Object.getPrototypeOf`, `Object.hasOwn`, `X.prototype`
     of those constructors. Strict-mode TypeErrors match node's messages.
  5. **Deferred**: `console.log` of plain objects (F24, node's `util.inspect` line-breaking rules) traps
     (`inspect_object`); moved to T21 (self-hosted, with arrays). Getters/setters, `Object.defineProperty`, methods
     and `this` (T14), `Object.keys` & co. (arrays, T15).
- 2026-09-25: IR: `Builtin`, intrinsics `NewObject/GetProp/SetProp/DefineProp/DeleteProp/HasProp/SetProto/CopyProps/
  ForInKeys/KeysLen/KeysGet/ObjectCreate/GetPrototypeOf/HasOwn/InstanceOf/Prototype`; `Length`/`ErrorProp` removed
  (`GetProp`); `NewError` takes `(message, options)`. Backend: `object.rs` (layout, lazy prototypes, all helpers),
  runtime types 4–8 and globals `G_BUILTIN`/`G_PROTOS`; ToString/ToNumber/`==`/`+`/typeof/inspect/describe/adapter
  go through objects. Lowering: members (get/set/compound/`++`), literals, `delete`, `in`, `instanceof`, `Object.*`,
  `B.prototype`, optional member chains, `for-in`. Bugs found on the way: `string_has` used a nullable key (validation
  error); `args()` popped the last argument even without extras (tuple `if let` evaluates both sides); `"str".name`
  took the string-index path (-1 compared signed) → unsigned compare. T12 fixtures pass again.
- 2026-09-25: 9 fixtures in `tests/fixtures/objects/` (access, conversions, delete-in, enumeration, errors, literals,
  many, prototypes, strict): `scripts/diff.sh tests/fixtures/objects` → 9 passed. `scripts/diff.sh` → all pass.
- 2026-09-25: unit tests: `unsupported.rs` (new `objects` test; `delete` example replaced), `build.rs`
  `unimplemented_object_features_trap`. `cargo test` 56 ok, clippy clean, `scripts/diff.sh` 21 passed / 8 skipped,
  `wasm-tools validate --features all` on `many.js`.

## Notes / Result
**Done.** Plain objects in dictionary mode with prototypes; errors are objects (F32). Decisions above.

What changed:
- IR: `Builtin`; object intrinsics (see Progress); `NewError(message, options)`.
- Backend: `crates/backend/src/object.rs` (layout, hash index, lazy built-in prototypes with trapping method
  sentinels, get/set/define/delete/in/hasOwn/create/getPrototypeOf/instanceof, for-in keys, ToPrimitive, error
  objects); runtime types `$object`, `$keys`, `$vals`, `$bytes`, `$i32s`; conversions and `==`/`+` handle objects;
  the export adapter reads `name`/`message` as properties.
- Lowering: literals, members (get/set/compound/`++`/`delete`), optional member chains, `in`, `instanceof`,
  `for-in`, `Object.create`/`getPrototypeOf`/`hasOwn`, `B.prototype`; compound assignment shared (`combine`).

Commands: `cargo test` (56 ok), `cargo clippy --all-targets` (clean), `scripts/diff.sh` (21 passed, 8 skipped).

Unsupported / traps (Follow-ups): `console.log` of a plain object traps (F24, now T21); reading a built-in method traps
(F37); getters/setters/`defineProperty`/freezing (F36); symbols (F38); `Object.keys` & co. (F39, arrays); method and
optional calls (F41, T14); hidden classes (F40).

Next smallest step: T14 (closures, function values, `this`): methods in literals, `o.m()`, and real built-in
methods can replace the sentinels.
