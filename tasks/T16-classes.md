# T16: Classes

Status: todo | Depends on: T15

## Goal
`class` with constructor, methods, static members, fields, `extends`, `super()` and `super.m()`, `new.target`, `instanceof`.
Also F36: getters/setters (in classes and object literals, where the lowering rejects them today), `Object.defineProperty`,
read-only/non-configurable attributes, `Object.freeze`/`seal` (accessor entries in `crates/backend/src/object.rs`).

## Acceptance
- Fixtures `classes/*.js` match node.

## Notes / Result
