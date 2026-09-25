# Tasks

One task per session. Read `../CLAUDE.md` and `../docs/design.md` first. Update a task's status here when it finishes.

| Task | Title | Status | Depends on |
|---|---|---|---|
| [T01](T01-toolchain-probes.md) | Pin toolchain, prove features | done | - |
| [T02](T02-workspace-scaffold.md) | Workspace scaffold | done | T01 |
| [T03](T03-frontend.md) | Frontend: oxc parse + semantic | done | T02 |
| [T04](T04-ir-v0.md) | IR v0 design | todo | T02 |
| [T05](T05-lower-numeric.md) | Lower numeric subset | todo | T03, T04 |
| [T06](T06-backend-v0.md) | Backend v0: IR → core Wasm | todo | T04 |
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
