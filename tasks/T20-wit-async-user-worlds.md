# T20: WIT async and user worlds

Status: todo | Depends on: T19

## Goal
Accept a user `.wit` world. Map exported and imported functions with D4's conventions, including `async func` ↔ async JS functions,
`future<T>` ↔ Promise, `stream<u8>` ↔ async iterable of `Uint8Array`, records, variants, options, results and lists.
Imported interfaces become ES imports (`import { f } from "pkg:ns/iface"`).

## Acceptance
- A library world with 1 async export and 1 async import, composed with a test component via `wac`, runs on wasmtime.
- A stream export echoes input bytes.

## Notes / Result

## Handoff (written 2026-09-26, end of the T25 session)

**State:** `main` after `T25 slice 6: promises and timers` (T17–T19, T21, T24, T25 done). This task file is only
a goal so far: nothing in it is started. Commit/push only when the user asks; no attribution lines.

**Run first (baseline):**
```sh
scripts/check-tools.sh
cargo build -q --release && cargo test -q --no-fail-fast 2>&1 | grep -E 'FAILED|panicked'   # nothing
scripts/diff.sh -j 4 | tail -1                              # 92 passed, 0 failed, 11 skipped
target/release/js2wasi3 build tests/fixtures/hello.js -o /tmp/h.wasm && stat -c %s /tmp/h.wasm   # 55367
```

**Read first:** `docs/design.md` D3 (event loop: callback lift, `Helper::Loop`) and D4 (WIT conventions),
`docs/wit-names.md`, `TOOLS.md` (async core names from wit-component, probe results), probes 2, 5, 8
(`probes/cmd.wat`, `cmd2.wat`, `timers.wat`: hand-written P3 callback loops), then the code:
- `crates/component/src/lib.rs`: generates the world from `ExportSig`s (params/results only `F64`/`String`,
  `result<T, js-error>` for throwing exports), embeds it, encodes with wit-component. `--world ns:pkg/world` names it.
- `crates/backend/src/abi.rs`: library export adapters (flat params, strings through `cabi_realloc` + a bump
  allocator reset per call, return area, `js-error`). Sync lifts only.
- `crates/backend/src/host.rs`: a command's `run` export (async callback lift), the event loop (`Loop`, `Tick`,
  `Callback`), stdout/stderr streams; `timers.rs`, `promise.rs` (job queue, `Drain`, `Unhandled`).
- `crates/lower/src/lib.rs`: export collection and every `"T20"` rejection (`console`, timers, `Math.random`,
  `process.nextTick` in a module with exports; grep `"T20"`).

**Follow-ups picked up in T20** (rows in `tasks/README.md`): F11 (library top-level code / module init), F12
(`--emit-wit`, optional), F13 (types beyond `f64`/`string`), F15 (wasmtime `--invoke` keyword names: CLI-only),
F21 (> 16 flat params), F26 (callback ignores `TASK_CANCELLED`; output pumping), F34/F60 (`ir::may_throw`: a user
world decides `result`), F55 (rest params in exports), F65 (library exports never drain jobs / report unhandled
rejections), F83 (`wasi:random` in library worlds).

**Suggested slices (smallest first; like T25, log each in Progress; agree the split with the user first):**
1. **Library runtime:** exports drain the job queue after the call and report unhandled rejections (F65);
   libraries run top-level code once before the first export (F11); `wasi:random` and stdout/stderr imports in
   library worlds so `Math.random`/`console`/timers stop being command-only (F83; timers in a library need the
   async lift of slice 2).
2. **Async exports:** `export async function f` → `async func` in the generated world, lifted with the callback
   ABI: run until the promise settles, `task.return` its value (or `js-error`), with the same loop as `run`
   (timers, microtasks). Test with `wasmtime run --invoke`.
3. **User worlds:** `--wit <file|dir> --world <name>` parsed with `wit-parser` (already a dependency): check the
   module's exports against the world, map WIT types per D4 (bool, integers, `f32`/`f64`, `char`, `string`,
   `list<T>`, `option`, `result`, `record`, `variant`, `enum`, `flags`, `tuple`; F13, F21, F55); the world then
   decides which exports return `result` (F34/F60). Generated worlds keep working without `--wit`.
4. **Imports:** `import { f } from "ns:pkg/iface"` (today every `import` is rejected: `docs/keywords.md`) → lowered
   imports with the adapters in reverse; `async func` imports via `[async-lower]` + a subtask in the waitable
   set, surfaced as a Promise (like `wait-until` in `host.rs`). Acceptance: a library world with one async export
   and one async import, composed with a test component via `wac` (installed: `wac-cli 0.11.0`), runs on wasmtime.
5. **`future<T>` / `stream<u8>`:** future ↔ Promise; `stream<u8>` ↔ async iterable of `Uint8Array`. **Blockers to
   raise with the user:** `for await` and async generators are unsupported (`patterns.rs`: "`for await`", F72) and
   there are no typed arrays (`Uint8Array`, T25 left them out). Either add them (a slice or a new task) or map
   `stream<u8>` to something smaller first. Acceptance: a stream export echoes its input bytes.

**Pitfalls carried over:**
- Wasm `select` picks its **first** operand when the condition is non-zero; check every `select`/`typed_select`.
- Size gates: anything not always needed is gated (`ir::Module::names` incl. the `$timers`/`$immediates`/`$ticks`
  markers the lowering adds, `builtins::GATED`, `has_maps`, `has_tags`, …); every runtime global is declared in
  every module (F113), so new globals cost hello.js bytes: say so in Notes.
- `git stash` + rebuild for size comparisons leaves a stale `target/release` binary: rebuild after `stash pop`.
- Fixtures are ES modules (`tests/fixtures/package.json`); scratch files outside run as CommonJS in node, which
  changes `process.nextTick` vs microtask order.
- Subtasks must leave their waitable set (`waitable.join h 0`) before `subtask.cancel` (probe 8).

## Progress
