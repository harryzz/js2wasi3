# Tasks

One task per session. Read `../CLAUDE.md` and `../docs/design.md` first.

**How to read this file**
- **Status table**: every task and where it stands (`todo` / `in progress` / `done` / `needs approval`).
- **Follow-ups**: everything found during a task but deliberately left for later, with the task that should pick it up.
- Details of a finished task (what changed, commands run, results) are in its own file under "Notes / Result".

**Next up: T07.** Start with the *Handoff* section in [T07-component-lib.md](T07-component-lib.md).

| Task | Title | Status | Depends on |
|---|---|---|---|
| [T01](T01-toolchain-probes.md) | Pin toolchain, prove features | done | - |
| [T02](T02-workspace-scaffold.md) | Workspace scaffold | done | T01 |
| [T03](T03-frontend.md) | Frontend: oxc parse + semantic | done | T02 |
| [T04](T04-ir-v0.md) | IR v0 design | done | T02 |
| [T05](T05-lower-numeric.md) | Lower numeric subset | done | T03, T04 |
| [T06](T06-backend-v0.md) | Backend v0: IR → core Wasm | done | T04 |
| [T07](T07-component-lib.md) | Component v0: library world | todo | T06 |
| [T08](T08-jsval.md) | JS values on WasmGC | todo | T07 |
| [T09](T09-strings.md) | Strings + string boundary | todo | T08 |
| [T10](T10-command-console.md) | Command world + console.log (P3 stdout) | todo | T09 |
| [T11](T11-diff-harness.md) | Differential harness node vs wasmtime | todo | T10 |
| [T12](T12-exceptions.md) | Exceptions (native EH) | todo | T11 |
| [T13](T13-objects.md) | Objects and prototypes | todo | T12 |
| [T14](T14-closures.md) | Closures, function values, `this` | todo | T13 |
| [T15](T15-arrays-iteration.md) | Arrays and iteration | todo | T14 |
| [T16](T16-classes.md) | Classes | todo | T15 |
| [T17](T17-promises-microtasks.md) | Promises + microtask queue | todo | T16 |
| [T18](T18-generators-state-machine.md) | Suspension transform: generators | todo | T17 |
| [T19](T19-async-await-event-loop.md) | async/await + P3 event loop + timers | todo | T18 |
| [T20](T20-wit-async-user-worlds.md) | WIT async + user worlds | todo | T19 |
| [T21](T21-selfhosted-builtins.md) | Self-hosted built-ins | todo | T16 |
| [T22](T22-stack-switching-mode.md) | Optional stack-switching mode | needs approval | T19 |
| [T23](T23-test262-subset.md) | test262 subset runner | needs approval | T12+ |

The first milestone is T03 → T07: `add.js` runs as a component. The second is T08 → T11: `hello.js` runs as a WASI 0.3 command with differential tests.

## Follow-ups (deferred work)

Each open item names where it came from and which task should pick it up. When it's done, tick it and say where.

| # | Item | Found in | Pick up in | Done |
|---|---|---|---|---|
| F1 | IR validator: label scoping, `Continue` only to a `Loop`, id ranges, type agreement | T04 | start of T06 | [x] T06 (`ir::validate`) |
| F2 | Source positions per statement in the IR (only `Function.pos` today). T05 didn't need them: its diagnostics use AST spans | T04 | when backend errors need source positions | [ ] |
| F3 | Static data segments (string literals) in the IR | T04 | T09 | [ ] |
| F4 | Reject imports / re-exports of other files with a clear "unsupported" diagnostic | T03 | T05 | [x] T05 (`lower`: import, re-export, `export *`) |
| F5 | Show secondary diagnostic labels (e.g. "first declared here") as notes | T03 | any, when useful | [ ] |
| F6 | TypeScript input (`.ts`): today it's parsed as `.mjs` | T03 | not planned yet, needs a task | [ ] |
| F7 | Multi-file ES modules (resolve and link imports) | T03 | not planned yet, needs a task | [ ] |
| F8 | Peephole: drop `f64->i32(i32->f64(x))` round trips between chained bitwise ops (correct, only wasteful; in Wasm each is a `$to_int32` call) | T05 | an optimisation pass, after T11 | [ ] |
| F9 | `>>>` / `>>>=`: needs an unsigned `I32 → F64` convert in the IR | T05 | T08 | [ ] |
| F10 | Implicit `undefined`: declarations without init, `return;` mixed with values, falling off the end, missing args, void results as values, postfix `++` as value, booleans as values | T05 | T08 | [ ] |
| F11 | Top-level code and non-literal top-level initializers (module init) | T05 | T10 | [ ] |
