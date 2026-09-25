# Tasks

One task per session. Read `../CLAUDE.md` and `../docs/design.md` first.

**How to read this file**
- **Status table**: every task and where it stands (`todo` / `in progress` / `done` / `needs approval`).
- **Follow-ups**: everything found during a task but deliberately left for later, with the task that should pick it up.
- Details of a finished task (what changed, commands run, results) are in its own file under "Notes / Result".

**Next up: T14.** Start with the *Handoff* section in [T14-closures.md](T14-closures.md). T13 is done: plain objects with prototypes (literals, get/set/delete/`in`, `for-in`, `Object.create`), errors are objects (see T13's Notes / Result). T12: exceptions on native Wasm EH. The second milestone is reached: `hello.js` runs as a WASI 0.3 command, and `scripts/diff.sh` compares every command fixture with node (see T11's Notes / Result). WIT naming rules: [../docs/wit-names.md](../docs/wit-names.md).

| Task | Title | Status | Depends on |
|---|---|---|---|
| [T01](T01-toolchain-probes.md) | Pin toolchain, prove features | done | - |
| [T02](T02-workspace-scaffold.md) | Workspace scaffold | done | T01 |
| [T03](T03-frontend.md) | Frontend: oxc parse + semantic | done | T02 |
| [T04](T04-ir-v0.md) | IR v0 design | done | T02 |
| [T05](T05-lower-numeric.md) | Lower numeric subset | done | T03, T04 |
| [T06](T06-backend-v0.md) | Backend v0: IR → core Wasm | done | T04 |
| [T07](T07-component-lib.md) | Component v0: library world | done | T06 |
| [T08](T08-jsval.md) | JS values on WasmGC | done | T07 |
| [T09](T09-strings.md) | Strings + string boundary | done | T08 |
| [T10](T10-command-console.md) | Command world + console.log (P3 stdout) | done | T09 |
| [T11](T11-diff-harness.md) | Differential harness node vs wasmtime | done | T10 |
| [T12](T12-exceptions.md) | Exceptions (native EH) | done | T11 |
| [T13](T13-objects.md) | Objects and prototypes | done | T12 |
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
| F3 | Static data segments (string literals) in the IR | T04 | T09 | [x] T09 (`Expr::Str`; literals are `array.new_fixed` globals, no data segment needed; see F19) |
| F4 | Reject imports / re-exports of other files with a clear "unsupported" diagnostic | T03 | T05 | [x] T05 (`lower`: import, re-export, `export *`) |
| F5 | Show secondary diagnostic labels (e.g. "first declared here") as notes | T03 | any, when useful | [ ] |
| F6 | TypeScript input (`.ts`): today it's parsed as `.mjs` | T03 | not planned yet, needs a task | [ ] |
| F7 | Multi-file ES modules (resolve and link imports) | T03 | not planned yet, needs a task | [ ] |
| F8 | Peephole: drop `f64->i32(i32->f64(x))` round trips between chained bitwise ops (correct, only wasteful; in Wasm each is a `$to_int32` call) | T05 | an optimisation pass, after T11 | [ ] |
| F9 | `>>>` / `>>>=`: needs an unsigned `I32 → F64` convert in the IR | T05 | T08 | [x] T08 (signed convert + 2^32 if negative, no IR change) |
| F10 | Implicit `undefined`: declarations without init, `return;` mixed with values, falling off the end, missing args, void results as values, postfix `++` as value, booleans as values | T05 | T08 | [x] T08 |
| F11 | Top-level code and non-literal top-level initializers (module init). Commands: done in T10 (`main`). Libraries (files with exports) still reject it: needs init code run before the first export call | T05 | T20 | [ ] |
| F12 | `--emit-wit <file>`: write the generated WIT next to the component (component binaries don't keep the world/package name, `wasm-tools component wit` shows `root:component`) | T07 | any, when useful (latest T20) | [ ] |
| F13 | Export signatures beyond `f64`: map `JsVal`/`Bool` params and results to WIT types at the boundary. T08: `$export` wrappers convert jsval ↔ f64 (ToNumber). T09: `string` done (JSDoc + usage); `bool`, `option`, records etc. remain | T07 | T20 (user WIT) | [ ] |
| F14 | Inference: `bool`-typed slots (booleans stored in variables are boxed today), int32 ranges | T08 | optimisation pass, after T11 | [ ] |
| F15 | wasmtime 49 `--invoke` can't call exports whose WIT name is a keyword (`bool`, `use`, `type`…), even as `%bool`. Valid components, CLI-only limit. Avoid such names in tests; maybe warn at build | T08 | T20 (not a harness concern: it runs commands, T11) | [ ] |
| F16 | Calls with more arguments than parameters (evaluate the extras, then drop them) | T08 | T14 | [ ] |
| F17 | `typeof x` as a value (a string) | T08 | T09 | [x] T09 (`$typeof_string`, folded when the kind is known) |
| F18 | Logical assignment `&&=`, `\|\|=`, `??=` | T08 | T13 or any | [ ] |
| F19 | String literals as `array.new_fixed` globals cost ~3 bytes per code unit; long ones should come from a data segment (`array.new_data`, lazily) | T09 | optimisation pass, after T11 | [ ] |
| F20 | String → number (ToNumber of a string: `"5" * 2`, `"1" == 1`) and `<`/`>` on two strings. Rejected when known to be a string, a trap at run time otherwise | T09 | T21 | [ ] |
| F21 | Canonical ABI: more than 16 flat params (passed in memory) are rejected today | T09 | T20 | [ ] |
| F22 | TDZ: a function reading a top-level `let`/`const` before `main` initialized it gets a placeholder value (only direct reads in `main` are rejected) | T10 | T12 (throw ReferenceError) | [x] T12 (init flags; `main` throws too) |
| F23 | `console.log` format strings (`%s %d %i %f %j %o %O %c %%`, first of ≥2 args): rejected for literals, a trap otherwise | T10 | T21 | [ ] |
| F24 | `console.log` of objects, arrays, functions (node's `util.inspect` format; plain objects trap today, errors print `[Name: msg]`) | T10 | T21 (self-hosted, with arrays; moved in T13) | [ ] |
| F25 | Uncaught exception in a command: print it to stderr, flush, `task.return(err)` (`run` in `backend/src/host.rs` must catch `$js`) | T10 | T12 | [x] T12 |
| F26 | Event loop: the callback ignores `TASK_CANCELLED`; output is only pumped while JS runs if the host takes writes at once (otherwise buffered until `main` returns) | T10 | T19 | [ ] |
| F27 | When stdout's reader goes away (`\| head`) `run` returns `err` (exit 1); node exits 0. Decide which the harness expects | T10 | T11 | [x] T11 (keep `err`: the write failed; not covered by the harness) |
| F28 | `var` in a nested top-level block (`{ var z }`, `for (var i …)`) is a local of `main`: functions using it are rejected | T10 | T14 | [ ] |
| F29 | Harness: uncaught exceptions print differently from node (stack traces), so stderr needs a comparison mode for them (e.g. first line only, via a sidecar) | T11 | T12 | [x] T12 (`x.uncaught`, `scripts/node-uncaught.mjs`) |
| F30 | `scripts/diff.sh` isn't run by `cargo test` (it needs node); wire it into CI once there is one | T11 | not scheduled | [ ] |
| F31 | Stack traces: `err.stack`, `console.log(err)` printing the stack like node (today `[TypeError: msg]`), `js-error.stack` (always `none`) | T12 | not scheduled | [ ] |
| F32 | Errors as real objects: `Error.prototype`, `instanceof`, other properties, `{ cause }`, `new` of other constructors; replace the `name`/`message` intrinsics with property access | T12 | T13 (subclassing: T16) | [x] T13 (subclassing and other `new`: T16) |
| F33 | Deep recursion is a Wasm trap (uncatchable); node throws a catchable `RangeError: Maximum call stack size exceeded` | T12 | not scheduled | [ ] |
| F34 | `ir::may_throw` is conservative (e.g. `.length` of a value not known to be a string), so some exports get `result<T, js-error>` needlessly | T12 | T20 (user WIT decides) | [ ] |
| F35 | When the event-loop callback runs JS (T19), it must catch `$js` like `run` does (print `Uncaught …`, return `err`) | T12 | T19 | [ ] |
| F36 | Getters/setters, `Object.defineProperty`, property attributes beyond enumerable (read-only, non-configurable), `Object.freeze`/`seal` | T13 | T16 (with class `get`/`set`; needs T14) | [ ] |
| F37 | Built-in methods (`Object.prototype.toString`, `String.prototype.slice`, …) are sentinels that trap when read; a user-replaced `toString`/`valueOf` traps in ToPrimitive | T13 | T14 (callable values) / T21 (the methods) | [ ] |
| F38 | Symbols as property keys (`Symbol.iterator` is needed by T15) | T13 | T15 | [ ] |
| F39 | `Object.keys`/`values`/`entries`/`assign`/`fromEntries`, `JSON.stringify` | T13 | T15 (arrays) / T21 | [ ] |
| F40 | Hidden classes / inline caches for property access (dictionary mode only today) | T13 | after a benchmark | [ ] |
| F41 | Method calls `o.m()` and optional calls `o?.m()` | T13 | T14 | [ ] |
