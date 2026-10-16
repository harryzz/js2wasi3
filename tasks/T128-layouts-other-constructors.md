# T128: layouts for objects whose fields another function sets (F407)

Status: not started (handoff, 2026-10-16) | Depends on: T109, T110, T111, T113, T117, T124 | Local (+ a-03 only after asking)

## Goal
F407 (found in T109): an object gets a layout (T109: pre-sized, keys added by slot, a shared index; T110 inline slot
reads/writes; T111 number cells; T117 shared keys/attributes) only when `new F(…)` names a `function`/base-class
constructor whose own body writes `this.k = …`. Objects whose fields are written by **another function** get no
layout and take the dictionary path:
- raytrace (Octane, Prototype.js style): `var Class = { create: function() { return function() {
  this.initialize.apply(this, arguments); } } }` (`tests/bench/octane/raytrace.js:33`); the fields are set in each
  class's `initialize: function(…) { this.x = …; … }` (e.g. line 607), called through `apply` from a constructor
  that has no `this.k =` of its own.
- deltablue: `X.superConstructor.call(this, …)` (`tests/bench/octane/deltablue.js:216, 297, 315, 345, 462`): a
  derived constructor runs its base's field writes through `call`.
- also no layout for derived-class constructors and arrow functions (F407's text).

This is a **speed** task (no output changes): raytrace is 31.8× node on a-03 (`docs/compatibility.md`), the worst
Octane ratio; deltablue 22.5×.

## Evidence so far (T124, laptop, `out/count-t124`, `perf record -e instructions:u`)
- `grow` calls per run: raytrace **11,131,976**, earley-boyer 1,750,300, regexp 908,290, deltablue 67,901 (counters
  `grow.call`, `define.layout_grow`, `define.layout_own` exist in `build --count`, T124).
- raytrace profile: `get_named` 14.8%, `find_own_named` 12.5%, `find_own` 11.5% (lookups on layout-less objects,
  ~39%), `grow` 8.1%, `set_named` 3.8%, `define` 3.8%, `get` 3.5%, `append_named` 2.3%, `alloc_object` 1.8%
  (building them, ~16%).
- splay is NOT a target (T124: ~92% wasmtime's copying collector; F427 dropped).

## Read first
- `tasks/T109-site-layouts.md` (how layouts are found: `Lowerer::this_layout`, `crates/lower/src/lib.rs:3176`;
  `Intrinsic::LayoutSet`/`LayoutDefine`, `lib.rs:7071`, `:7194`; `module.layouts`), `tasks/T117-shared-layout-shape.md`
  (shared keys/attrs, `object.rs` `alloc_layout` `:2740`, `own_shape`, `define`'s layout branch), `tasks/T110`/`T111`
  (inline slots: `crates/backend/src/lib.rs` `LAYOUT_GUESSES` `:1044`, `num_slots`, the `GetProp`/`SetProp` constant-key
  paths ~`:2525`, `:2674`), `tasks/T124-layout-slack.md` (the measurements above).
- `docs/design.md` "Speed work" (the rules below), `docs/root-causes.md` R3.
- User rule "consult proven implementations first": find what V8 (in-object slack tracking: a map's instance size is
  learned from the first allocations, then shrunk; transitions give objects built the same way the same map
  whatever function wrote the fields) and JSC (structure transitions, inferred inline capacity) ship for this case, let
  that decide the design, report it and **ask before building**.

## Plan (suggested; ask at each step marked "ask")
- [ ] Count run (`scripts/bench.sh -c`): in raytrace/deltablue, how many objects are built per constructor and which
  keys each ends with (a counter or a one-off trace in `--count` builds only); confirm the `grow`/lookup numbers.
- [ ] Research V8/JSC for fields written by another function (`apply`/`call` of an initializer, derived constructors);
  write the findings in Progress; propose a design (ask). Candidates to evaluate, not to assume: (a) compile-time:
  follow `this.initialize.apply(this, arguments)` / `Base.call(this, …)` in a constructor to the called function's
  `this.k =` writes (static, like T109); (b) run-time: per-allocation-site learned capacity/keys (slack tracking).
- [ ] Build (after the user's yes); fixtures `objects/`, `classes/`, `builtins/`, `scripts/diff.sh`; `cargo test`.
- [ ] Gate (`scripts/gate.sh <base>`, laptop, then a-03: ask); full test262 on a-03 (ask).

## Rules that apply (from `CLAUDE.md`, `docs/design.md` "Speed work" and the user's standing instructions)
- Write a dated Progress line after every step. Set the status in `tasks/README.md` to `in progress` when starting.
- Root cause first; count before designing; a miss may not cost more than the old path; a miss doesn't repeat the
  inline test; equal instructions = placement; nothing slower (laptop gate, then a-03 after asking).
- Only the standard tests and benchmarks (fixtures, test262, the 18 programs, micro-benchmarks). No zigbee2mqtt.
- Report results, don't fix after results without asking; never tune to a benchmark.
- a-03 is test-only: copy every log back to `out/` at once; docs cite the laptop copies. Ask before a-03 runs, long
  runs and installing tools. Never propose wasmtime upstream issues.
- Never commit/push without asking each time; no attribution lines (no Co-Authored-By, no session links).
- At the end: Notes/Result, `tasks/README.md` status and follow-ups (next free F is F439), tick F407, `CLAUDE.md`,
  `tests/test262/HISTORY.md` (after a full run), `docs/design.md`, `docs/root-causes.md`.

## State at handoff (2026-10-16)
- `main` = `ebe6055` (T127), pushed; working tree clean except this file and the README row.
- wasmtime 49.0.3 pinned (laptop `~/.wasmtime/bin/wasmtime`, a-03 `~/.local/bin/wasmtime`; 50.0.0-rc.1 also on a-03 as
  `wasmtime-50.0.0-rc.1`, F436 pins 50.0.0 when final). test262 baseline: 28,123 pass.
- Gate base for this task: `ebe6055`.

## Progress
- 2026-10-16: handoff written (user: "Handoff f407 for new session").

## Notes / Result
