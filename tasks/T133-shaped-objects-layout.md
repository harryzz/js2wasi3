# T133: objects as one WasmGC struct per shape, small header (step 2 of the new design)

Status: todo | Depends on: T000, T131, T132 | Local (+ a-03 for the gate, after asking)

## Goal
Objects whose shape the type analysis knows become a WasmGC struct per shape with a small header (flags +
overflow dictionary, as `probes/layout2.wat`), not under today's 7-field `$object` header (`probes/layout3.wat`:
that header costs 2.0× V8 / 1.7× wasmtime); every generic path reads them. Then `f64` fields (step 3).
Probe numbers: struct instead of keys/vals 1.7×, `f64` fields 3.2× (V8; `tasks/T132-method-calls.md` Progress).

## Read first
- `tasks/T000-general-speed-analysis.md`, `tasks/T131-type-analysis.md`, `tasks/T132-method-calls.md`.

## Progress

## Notes / Result
