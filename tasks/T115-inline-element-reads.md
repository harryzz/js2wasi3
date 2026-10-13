# T115: array element reads inline on receivers of unknown type

Status: in progress | Depends on: T107, T113 | Local (+ a-03 only after asking)

## Goal
nbody's profile on a-03 (2026-10-13, `fd1b5e7`, recorded in T111's Progress): `get` is 22.9% self:
`bodies[i]` / `bodies[j]` in `advance`, where `bodies` is a parameter of unknown type, so every element read
calls the generic `get` helper (14 per pair of bodies). `get`'s first fast path (an `$array`, a small integer
key, inside the element store, a data value) is cheap; the call and its key boxing are not. The user chose this
before T111 (2026-10-13: "first an inline fast path for array element reads on values of unknown type").

## Design
- In the backend, `GetProp(obj, key)` with a key that isn't a constant string (`lib.rs` `elem_get`): the same
  checks as `get`'s first fast path, inline: `ref.test $array` (final since T113: one compare), the key an i31
  (or, for an `f64` key such as a typed loop index, an integer-valued `f64`: no boxing), inside the element
  store, not null (hole / past `length`), not an accessor; else the `get` call (an `f64` key boxed only then).
- Number-mode arrays (T107) have an empty element store: they take the call, as before.
- Three more scratch locals per function (`i32`, `(ref null $vals)`, `jsval`).

## Plan
- [x] Implement (`elem_get`).
- [x] Checks: `scripts/diff.sh` on `arrays/`, `objects/`, `classes/`, `builtins/`; `wasm-tools validate`.
- [ ] Gate on a-03 against `fd1b5e7` (ask): no program or micro case slower (the misses: `o[k]` on plain
  objects, functions, Maps in `object-kinds.js`); test262 (ask).

## Acceptance
- `scripts/gate.sh`: nbody faster, nothing slower; fixtures = node; test262 no changes.

## Notes / Result

## Progress
- 2026-10-13: implemented. Laptop: nbody 4,078 → 3,208 ms (−21%), instructions 35.96G → 30.19G (−16%), output =
  node; `arrays/` fixtures pass (8); the module validates.
- 2026-10-13: fixtures: `arrays/` 8, `objects/` 16, `classes/` 12, `builtins/` 73 (1 skipped as before) pass. Next:
  the gate on a-03 (to ask).
