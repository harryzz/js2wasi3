# Tasks

One task per session. Read `../CLAUDE.md` and `../docs/design.md` first.

**How to read this file**
- **Status table**: every task and where it stands (`todo` / `in progress` / `done` / `needs approval`).
- **Follow-ups**: everything found during a task but deliberately left for later, with the task that should pick it up.
- Details of a finished task (what changed, commands run, results) are in its own file under "Notes / Result".

**Next up: T19** (async/await + P3 event loop + timers). Start with the *Handoff* section in [T19-async-await-event-loop.md](T19-async-await-event-loop.md). T18 is done: generators (`function*`, `yield`, `yield*`, `next`/`return`/`throw`, `finally` on `return()`) via the IR suspension transform `ir::suspend` (see T18's Notes / Result). T17 is done: `Promise` (constructor, `then`/`catch`/`finally`, `resolve`/`reject`/`all`/`allSettled`/`race`), `queueMicrotask`, one job queue drained by the command after top-level code, unhandled rejections reported like node, `console.log` of promises (see T17's Notes / Result). T16: classes (fields, statics, private `#x` fields/methods, `extends` incl. native errors, `super`, `new.target`), `new`/`instanceof` with any function, accessors and property attributes (`defineProperty`, freeze/seal), user `valueOf`/`toString` (see T16's Notes / Result). T15: arrays, symbols, the iterator protocol, `for-of`, spread, destructuring and `console.log` of arrays/plain objects (see T15's Notes / Result). T14: functions are values (closures, arrows, methods, `this`, `call`/`apply`). T13: plain objects with prototypes. T12: exceptions on native Wasm EH. The second milestone is reached: `hello.js` runs as a WASI 0.3 command, and `scripts/diff.sh` compares every command fixture with node (see T11's Notes / Result). WIT naming rules: [../docs/wit-names.md](../docs/wit-names.md).

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
| [T14](T14-closures.md) | Closures, function values, `this` | done | T13 |
| [T15](T15-arrays-iteration.md) | Arrays and iteration | done | T14 |
| [T16](T16-classes.md) | Classes | done | T15 |
| [T17](T17-promises-microtasks.md) | Promises + microtask queue | done | T16 |
| [T18](T18-generators-state-machine.md) | Suspension transform: generators | done | T17 |
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
| F16 | Calls with more arguments than parameters (evaluate the extras, then drop them) | T08 | T14 | [x] T14 (`direct_call`; calls through values pass them all) |
| F17 | `typeof x` as a value (a string) | T08 | T09 | [x] T09 (`$typeof_string`, folded when the kind is known) |
| F18 | Logical assignment `&&=`, `\|\|=`, `??=` | T08 | T13 or any | [ ] |
| F19 | String literals as `array.new_fixed` globals cost ~3 bytes per code unit; long ones should come from a data segment (`array.new_data`, lazily) | T09 | optimisation pass, after T11 | [ ] |
| F20 | String → number (ToNumber of a string: `"5" * 2`, `"1" == 1`) and `<`/`>` on two strings. Rejected when known to be a string, a trap at run time otherwise | T09 | T21 | [ ] |
| F21 | Canonical ABI: more than 16 flat params (passed in memory) are rejected today | T09 | T20 | [ ] |
| F22 | TDZ: a function reading a top-level `let`/`const` before `main` initialized it gets a placeholder value (only direct reads in `main` are rejected) | T10 | T12 (throw ReferenceError) | [x] T12 (init flags; `main` throws too) |
| F23 | `console.log` format strings (`%s %d %i %f %j %o %O %c %%`, first of ≥2 args): rejected for literals, a trap otherwise | T10 | T21 | [ ] |
| F24 | `console.log` of objects, arrays, functions (node's `util.inspect` format). T15: arrays and plain objects print like node (`crates/backend/src/inspect.rs`); T17: promises print like node; still trapping: class instances, null prototypes, > 100 elements, extra array properties, symbol keys, strings needing escapes, nested errors; non-ASCII widths are counted as UTF-16 units | T10 | T21 | [ ] |
| F25 | Uncaught exception in a command: print it to stderr, flush, `task.return(err)` (`run` in `backend/src/host.rs` must catch `$js`) | T10 | T12 | [x] T12 |
| F26 | Event loop: the callback ignores `TASK_CANCELLED`; output is only pumped while JS runs if the host takes writes at once (otherwise buffered until `main` returns) | T10 | T19 | [ ] |
| F27 | When stdout's reader goes away (`\| head`) `run` returns `err` (exit 1); node exits 0. Decide which the harness expects | T10 | T11 | [x] T11 (keep `err`: the write failed; not covered by the harness) |
| F28 | `var` in a nested top-level block (`{ var z }`, `for (var i …)`) is a local of `main`: functions using it are rejected | T10 | T14 | [x] T14 (they're globals: `nested_vars`) |
| F29 | Harness: uncaught exceptions print differently from node (stack traces), so stderr needs a comparison mode for them (e.g. first line only, via a sidecar) | T11 | T12 | [x] T12 (`x.uncaught`, `scripts/node-uncaught.mjs`) |
| F30 | `scripts/diff.sh` isn't run by `cargo test` (it needs node); wire it into CI once there is one | T11 | not scheduled | [ ] |
| F31 | Stack traces: `err.stack`, `console.log(err)` printing the stack like node (today `[TypeError: msg]`), `js-error.stack` (always `none`) | T12 | not scheduled | [ ] |
| F32 | Errors as real objects: `Error.prototype`, `instanceof`, other properties, `{ cause }`, `new` of other constructors; replace the `name`/`message` intrinsics with property access | T12 | T13 (subclassing: T16) | [x] T13 (subclassing and other `new`: T16) |
| F33 | Deep recursion is a Wasm trap (uncatchable); node throws a catchable `RangeError: Maximum call stack size exceeded` | T12 | not scheduled | [ ] |
| F34 | `ir::may_throw` is conservative (e.g. `.length` of a value not known to be a string), so some exports get `result<T, js-error>` needlessly | T12 | T20 (user WIT decides) | [ ] |
| F35 | When the event-loop callback runs JS (T19), it must catch `$js` like `run` does (print `Uncaught …`, return `err`) | T12 | T19 | [ ] |
| F36 | Getters/setters, `Object.defineProperty`, property attributes beyond enumerable (read-only, non-configurable), `Object.freeze`/`seal` | T13 | T16 (with class `get`/`set`; needs T14) | [x] T16 (`props.rs`; array element attributes: F61) |
| F37 | Built-in methods (`Object.prototype.toString`, `String.prototype.slice`, …) are sentinels that trap when read; a user-replaced `toString`/`valueOf` traps in ToPrimitive. T14: values are callable now (`Function.prototype.call` is real); ToPrimitive still returns a string only, so calling a user `valueOf` needs it to return any primitive | T13 | T16 (user methods) / T21 (the methods) | [ ] T16: user `valueOf`/`toString` called by ToPrimitive (hint aware); built-ins still sentinels |
| F38 | Symbols as property keys (`Symbol.iterator` is needed by T15) | T13 | T15 | [x] T15 (`$symbol` ⊂ `$string`; `Symbol()`, `Symbol.iterator`; rest in F53) |
| F39 | `Object.keys`/`values`/`entries`/`assign`/`fromEntries`, `JSON.stringify`. T15: `keys`/`values`/`entries` done | T13 | T21 | [ ] |
| F40 | Hidden classes / inline caches for property access (dictionary mode only today) | T13 | after a benchmark | [ ] |
| F41 | Method calls `o.m()` and optional calls `o?.m()` | T13 | T14 | [x] T14 (`CallRef`, `chain_call`) |
| F42 | `arguments` (an array-like object) | T14 | T21 (moved in T15: rest parameters cover it) | [ ] |
| F43 | `Function.prototype.apply` (needs arrays), `bind`, `toString` (source text): sentinels that trap when read. T15: `apply` done | T14 | T21 | [ ] |
| F44 | `new` of user functions, `instanceof` with a user function (the `prototype` objects already exist) | T14 | T16 | [x] T16 (`Expr::New`, `InstanceOfValue`) |
| F45 | Rejected for now: assigning to a function declaration's binding (calls to it are direct), a function with a computed key in a literal (its `name` comes from the key at run time), a closure in a `for (let …)` head reading a head variable before its declaration | T14 | later | [ ] |
| F46 | Function objects get `length`/`name` (and `prototype` for `function`s) when made: 1–2 extra allocations per closure; could be lazy. Cell and environment reads `ref.cast` every time; a self-recursive nested function reads its own binding's cell for `self` | T14 | after a benchmark | [ ] |
| F47 | `Function.prototype` is a plain object (spec: a function, so `typeof` is `"function"`); functions' `name`/`length` are writable (read-only attributes, F36) | T14 | T21 | [ ] T16: `name`/`length` read-only; `Function.prototype` still a plain object |
| F48 | `console.log` of a function with own enumerable properties traps (node prints them after `[Function: f]`, F24) | T14 | T21 | [x] T16 (functions and classes print their own properties) |
| F49 | Array methods other than `push`/`pop`/`join`/`toString`/`values`/`[Symbol.iterator]` (`map`, `forEach`, `slice`, `indexOf`, …) are sentinels; `Array(...)`, `Array.from`, `Array.of`; the real methods only accept arrays as `this` | T15 | T21 | [ ] |
| F50 | Sparse arrays: an index or length ≥ 2^26 traps (dense store only) | T15 | after a benchmark / when needed | [ ] |
| F51 | Arrays and strings iterate without looking up `Symbol.iterator` (a replaced `Array.prototype[Symbol.iterator]` isn't seen); `values` and `[Symbol.iterator]` are two function objects (spec: the same) | T15 | T21 | [ ] |
| F52 | Array `length` is read-only-unaware: `Object.defineProperty`/frozen arrays (F36); array iterator objects print as a trap | T15 | T16/T21 | [ ] T16: frozen/sealed arrays respected; array iterator objects don't print like node |
| F53 | Symbols: `Symbol.for`, other well-known symbols, `description` of `Symbol()` vs `Symbol("")` (both empty), `Object.getOwnPropertySymbols`; object spread / `{...rest}` skip symbol keys; `sym == 1` throws (node: false) | T15 | T21 | [ ] |
| F54 | Destructuring assignment evaluates a member target after the value (spec: the reference first); a hole after a spread element in an array literal is rejected | T15 | later | [ ] |
| F55 | Exported functions with a rest parameter are rejected (the WIT signature needs a `list`) | T15 | T20 | [ ] |
| F56 | `for-of` and array destructuring always wrap the loop/bindings in a `try_table` (needed only for the protocol path; zero-cost EH, but bigger code) | T15 | optimisation pass | [ ] |
| F57 | `new.target` outside class constructors (ordinary functions, arrows), `super()` in an arrow function, `this`/`super` in an arrow inside a static field initializer: rejected. An arrow capturing a derived constructor's `this` before `super()` keeps the uninitialized value | T16 | later | [ ] |
| F58 | Classes: private getters/setters, `static { }` blocks, `accessor` fields, computed instance field keys, `o?.#x`: rejected. Private methods are own read-only hidden entries installed per instance (brand check by presence) | T16 | later | [ ] |
| F59 | Methods, accessors and fields whose key is computed and whose value is a function (its `name` comes from the key at run time, SetFunctionName) are rejected (except `[Symbol.iterator]`) | T16 | T21 | [ ] |
| F60 | `ir::may_throw` is unsound now that ToPrimitive calls user `valueOf`/`toString`: `+`, ToString, ToNumber and `==` on objects may throw but don't count, so such a library export lacks `result<T, js-error>` | T16 | T20 | [ ] |
| F61 | `Object.defineProperty` with non-default attributes on array indices or `length` traps (dense store has no attributes; only sealed/frozen arrays) | T16 | when needed | [ ] |
| F62 | `console.log`: circular references (`<ref *1>` / `[Circular *1]`) aren't detected (depth-limited instead); a function/class with own properties uses a width 1 larger than node's in the single-line check; class-instance errors print as `[name: message]` (no stack, F31) | T16 | T21 | [ ] |
| F63 | A class (like a `let`) used before its declaration in the same function is rejected at compile time; node throws a ReferenceError at run time | T16 | later | [ ] |
| F64 | Promises: species / subclassing (`then` and the statics always make `%Promise%` promises; only `new MyPromise` gets the subclass prototype), `Promise.any` (AggregateError), `Promise.withResolvers`, `Symbol.toStringTag`; the statics ignore `this` | T17 | T21 | [ ] |
| F65 | Library exports: jobs queued during an export call never run (only a command's `run` drains the queue) and unhandled rejections aren't reported | T17 | T20 | [ ] |
| F66 | The unhandled-rejection check runs once, after `main`'s drain; with timers/events it must run after every macrotask's drain (node: `processTicksAndRejections`) | T17 | T19 | [ ] |
| F67 | `queueMicrotask`'s TypeError lacks node's `code: 'ERR_INVALID_ARG_TYPE'` (uncaught: node prints `TypeError [ERR_INVALID_ARG_TYPE]: …`); an unhandled rejection whose reason is a function shows `#<Function>`-style text (node: the source); a rejected promise holding an Error prints without the stack (F31) | T17 | T21 | [ ] |
| F68 | Every command links the promise runtime (the drain and the unhandled check in `run`: +2.8 KB for `hello.js`) even without promises; gate it on use | T17 | optimisation pass | [ ] |
| F69 | Suspension transform: every local is saved/restored at each `yield` (no liveness) and boxed into a `$vals` frame; a per-generator GC struct with typed fields and only live locals would be faster; the frame stays alive after completion | T18 | optimisation pass, after a benchmark | [ ] |
| F70 | Generators: generator functions have no own `prototype` (generator objects' prototype is `%GeneratorPrototype%` directly), no `%IteratorPrototype%` / `%GeneratorFunction%`, `Symbol.toStringTag`; `%GeneratorPrototype%.constructor` is a sentinel | T18 | T21 | [ ] |
| F71 | `ir::suspend` handles `yield` only: `SuspendKind::Await` (and async generators) must be added, resumed from promise reaction jobs | T18 | T19 | [ ] |
