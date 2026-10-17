# T132: objects as one WasmGC struct per shape (step 2 of the new design)

Status: todo | Depends on: T000, T131 | Local (+ a-03 for the gate, after asking)

## Goal
Step 2 of `tasks/T000-general-speed-analysis.md`: objects whose shape the type analysis (T131, `crates/types`)
knows become a WasmGC struct per shape, with a header (flags + overflow dictionary, as `probes/layout2.wat`), and
every generic path (property get/set/has/delete, keys, for-in, JSON, `console.log`, `Object.*`, prototypes) reads
them correctly. Field reads/writes at sites the analysis typed become one shape check + one struct load/store; the
rest stays on today's generic paths. Number fields (`f64`, step 3) and typed function versions (step 4) are not in
this task.

Why: probe 1/2 (T000) measured the proposed design 19–22× faster than our output today on the same kernel; T131
measured that the analysis types 95.7% of the 18 programs' executed operations correctly (best possible 99.3%).

## Open design question (decide first, with a probe, then ask)
Today an object is a final `$plain` (keys / vals / attrs arrays, `F_INDEX` layout index; T109/T113/T117/T128).
Two ways to add shaped objects:
- (a) a new struct type per shape (subtype of a common `$obj` with the header); every generic runtime function
  dispatches on it — fastest field access, the largest change (every helper that tests `$plain`);
- (b) `$plain` keeps its role and gets a typed "slots" struct field per shape — generic paths need only learn the
  slots; field access costs one more load.
Measure (a) vs (b) on the probe kernel (as `probes/layout*.wat`, both variants, V8 and wasmtime) and count the
runtime functions each one touches, before choosing (ask).

## Read first
- `tasks/T000-general-speed-analysis.md` (probes, the design), `tasks/T131-type-analysis.md` (what the analysis
  gives, its limits: Notes / Result).
- `crates/backend/src/object.rs`, `props.rs`, `runtime.rs` (`$plain`, layouts, `G_IC`), `crates/lower/src/lib.rs`
  (`this_layout`, T109), `docs/design.md` "Speed work".
- `crates/types/src/lib.rs`: `analyze`; the lowering needs it on the program it lowers (`parse_entry` wraps CommonJS
  code and appends the prelude, so offsets differ from the file; F447).

## Plan (suggested; ask at each step marked "ask")
- [ ] Run the analysis on what the lowering sees (`Program` from `parse_entry`), results by AST node/span (F447).
- [ ] Probe (a) vs (b); list the runtime functions each touches; propose (ask).
- [ ] Build for objects from `new` of user constructors first; fixtures `objects/`, `classes/`; `scripts/diff.sh`;
  `cargo test`.
- [ ] Gate on a-03 (ask): the 18-program geomean, ms, instructions, `.wasm` size; test262 on a-03 (ask).

## Rules that apply
- Dated Progress line after every step; status in `tasks/README.md`.
- Only the standard tests and benchmarks; report plain numbers (geomean of the 18, `.wasm` size); never
  commit/push without asking; no attribution lines.

## Progress

## Notes / Result
