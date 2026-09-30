# T55: node's built-in modules, first slice (`process`, `path`, `events`, `util`, `os`)

Status: done | Depends on: T41 (linker, packages), T54 (CommonJS,
`require` table, `__intrinsic_cjs_mode` gating), F200

## Goal
Programs and packages that use node's built-in modules build and behave like node 24.21.0 for the pure-JS part of
the API, with node's module identity rules:
1. `require("path")`, `require("node:path")`, `import path from "node:path"`, `import { join } from "path"`,
   `import()` of them; one module object per built-in (`require("path") === require("node:path")`), ESM named
   exports as node gives them (built-ins' namespaces have every own property as a named export, plus `default`).
2. First modules: `path` (POSIX only, `path.posix`; `path.win32` a build error or a copy), `events`
   (`EventEmitter`, `once`, `on`, error handling, `captureRejections` maybe not), `util` (`inherits`, `format`,
   `inspect` through the existing inspect, `promisify`, `deprecate`, `types.*` subset, `isDeepStrictEqual`?), `os`
   (stubs with plausible WASI values: `platform()`, `EOL`, `tmpdir()`, `homedir()`, `cpus()`, …), `process` as a
   module and a global (`env` from `wasi:cli/environment`, `argv`, `exit`/`exitCode`, `cwd`, `platform`,
   `version(s)`, `hrtime`/`hrtime.bigint`, `uptime`, `memoryUsage`, `on("exit")`, `emitWarning`, `stdout.write`/
   `stderr.write`; `nextTick` exists since T25), `assert` if cheap (it's the most-required one in zigbee2mqtt's graph).
3. The globals node has that the probes hit: `process`, `performance` (`now`, `timeOrigin`), maybe `Buffer` (big:
   decide if it's this task or the next).
Programs that use no built-in compile to the same bytes. `fs`, `net`, `crypto`, `stream`, `buffer` are later tasks.

## What we know (at handoff)
- **Today**: a static `import` of a built-in, or a `require` of one that always runs (module top level, not in `try`
  or a function), is the build error "unsupported: node's built-in module `x` (planned: F200)"; a guarded `require`
  and `import()` throw `ERR_UNKNOWN_BUILTIN_MODULE` at run time (`link.rs`: `cjs_requests`, `dynamic_imports`;
  `packages::is_builtin`, `BUILTINS`). `process` as a global: only `process.nextTick(…)` (T25, `lib.rs`
  `is_global(…, "process")`); any other use is "unsupported: global `process`".
- **T54's real-package probe** (scratchpad, npm latest): `semver` stops only at `process` (`internal/debug.js`:
  `typeof process === 'object' && process.env && process.env.NODE_DEBUG`); `lru-cache@10` at `performance` and
  `process`; `inherits` falls back to its browser file (`util` guarded). Rerun them first: they're the acceptance
  probe (`npm install semver lru-cache@10 inherits` in the scratchpad, compare with node).
- **zigbee2mqtt counts** ([docs/zigbee2mqtt-feasibility.md](../docs/zigbee2mqtt-feasibility.md) blocker 2):
  `assert` 75, `fs` 40, `path` 38, `events` 37, `util` 36, `buffer` 36, `stream` 34, `os` 23, …
- **WASI**: `wasi:cli/environment` (env, args, cwd) and `wasi:cli/exit` are in the vendored WIT
  (`crates/component/wit/wasi-cli-0.3.1.wit`); a command imports only what it uses (like the clock, T26).
- **Gating pattern**: prelude code in `__intrinsic_*_mode()` blocks must hold no functions and use only what the
  enclosing function captures anyway (T54: the closure analysis sees dropped blocks). New top-level prelude
  functions are free (linked only when reachable).

## Design questions (decide first, record in docs/design.md)
1. **Representation**: each built-in as a module the linker adds to the graph from JS source shipped with the
   compiler (e.g. `crates/lower/builtins/node/path.js`, written as CommonJS or ESM), resolved for `x` and `node:x`,
   so T41/T54's machinery (namespaces, `require` table, interop) applies unchanged? Or prelude functions + a
   synthetic namespace? Recommendation at handoff: linker-level virtual modules (a `Host` that serves
   `<builtin>/path.js`), paths shown as `node:path` in diagnostics and `require.resolve("path") === "path"`.
2. **Source**: write our own or vendor MIT polyfills (`path-browserify` = node's path.posix, `events` npm package =
   node's `EventEmitter`, `util`'s `format`/`inherits`)? Vendoring: licence headers, and they are ES5 with sloppy
   patterns (F232). Check each against node's behaviour with fixtures, not by inspection.
3. **`process`** as a global: an object made on first use like `console` (T53) or the global object (T42); which
   members need host imports (`env`, `argv`, `exit`, `cwd`) and which are constants/stubs (`platform: "wasi"`? or
   `"linux"` to keep packages on their POSIX paths — node on WASI doesn't exist, pick and document). `process.exit`
   inside the P3 event loop: stop the loop, flush streams, return the status. `process.on("exit")`.
4. **ESM named exports of built-ins** and `default`: node's rule (`import { readFileSync } from "fs"` works; the
   default is the module object; `syncBuiltinESMExports`) — match for the modules done here.
5. **Unsupported built-ins/members**: keep them build errors where they always run, run-time errors elsewhere
   (T54's rule), and name what's missing per module (`os.networkInterfaces`, …).
6. **`Buffer`**: in or out of this task (it's used everywhere in zigbee2mqtt, ~1,500 uses; `Uint8Array` subclass
   with encodings). Probably its own task next.

## Plan (slices; each ends with fixtures)
1. Measurement + design (the questions above), the virtual-module mechanism with one module (`path`), both
   `require` and `import`, `node:` prefix, identity; gating check (scratchpad `cmp.sh`, see T54's Progress).
2. `process` (global and module) with the WASI imports it needs; `performance`; rerun the T54 package probe.
3. `events`, `util`, `os`, (`assert`).
4. Fixtures, gating, real-package probe (`semver`, `lru-cache`, more), targeted test262 (none of this is in
   test262: check `language/module-code` doesn't change), docs (design.md, keywords.md, README), follow-ups.

## Read first
- `tasks/T54-commonjs.md`: Notes / Result and Progress (the table, `cjs_requests`, gating lessons).
- `crates/frontend/src/link.rs`: `cjs_requests`, `dynamic_imports`, `resolve`, the `Host` trait.
- `crates/frontend/src/packages.rs`: `BUILTINS`, `is_builtin`.
- `crates/lower/builtins/console.js` (T53: an object made on first use) and `timers` in `crates/backend/src/`
  (`process.nextTick`).
- `docs/design.md` D4 (T41/T54 notes), D5 (prelude), D6 (global object).

## Tests
Fixtures under `tests/fixtures/node/` (ES modules, `"type": "module"` from the parent) and
`tests/fixtures/commonjs/<case>/` for `require`, via `scripts/diff.sh` (node is the oracle; `diff.sh` runs node
with `TZ=UTC` and `--no-warnings`: `process.env` differs between node and wasmtime, so print only what's set for the
test or pass env explicitly — decide how `diff.sh` passes env/args to both); `cargo test`; `wasm-tools validate` +
wasmtime. Gating: every non-built-in fixture byte-identical.

## Acceptance
- [x] `path`, `events`, `util`, `os`, `process` (module and global) behave like node for the covered API (fixtures),
      through `require`, `import`, `node:` and `import()`.
- [x] `semver` and `lru-cache@10` build and match node.
- [x] Unsupported built-ins/members are clear errors (build or run time per T54's rule) with follow-ups.
- [x] Programs without built-ins compile to the same bytes.

## Notes / Result
**Decisions** (recorded in design.md D4, D5, Testing): (1) node's built-in modules are **CommonJS files shipped with
the compiler** (`crates/frontend/node/<name>.js`, listed in `frontend/src/node.rs`), loaded by the linker at the path
`node:<name>` for `x` and `node:x`, so T41/T54's `require`, `import`, `import()` and interop apply unchanged, with one
module object per built-in; their code may use the prelude and intrinsics (like the linker's generated code). (2)
Ports of node 24's lib (MIT) where it's pure JS (`path`, `events`, parts of `util`/`assert`), our own otherwise. (3)
`process`/`performance` are prelude objects made on first use (like `console`), `platform` `"linux"`, `env`/`argv`/
`cwd()` from `wasi:cli/environment`; `process.exit` is a second exception tag no JS `catch`/`finally` sees; the exit
status is 0 or 1 (wasmtime 49 gates `exit-with-code`). (4) Named exports: the lexer's, or `node::exports` lists; no
`'module.exports'`. (5) Unsupported built-ins keep T54's rule; missing members throw with a follow-up number. (6)
`Buffer` is its own task (F238).

**What changed**: `crates/frontend/src/node.rs` (new), `node/{path,process,events,util,os,assert,tty}.js` (+
`path/posix`, `util/types`, `assert/strict`); `link.rs` (routing `x`/`node:x` in `cjs_requests`, `resolve`,
`dynamic_imports`, `read`/`format`, `is_node_module`, export lists, node's `No such built-in module`); `lib.rs`
(`is_generated` covers built-in modules); prelude `builtins/node.js` (`$$errInvalidArgType`, `$$errOutOfRange`) and
`builtins/process.js`; `cjs.js` (`require.resolve("node:x")`, shared error helper), `module.js` (the gated `node:`
lookup, and the `$$moduleRequireToken` bug); IR `Intrinsic::{Environment, ProcessExit, MonotonicNow,
InspectDepth}`, `Module::process_at_exit`; backend `host.rs` (`wasi:cli/environment` imports, `EXIT_TAG`,
`G_EXITED`, `Helper::{Environment, Exit, AtExit}`, the exit paths of `run`/tick/loop, all only in programs using
`process`), `inspect::Helper::AtDepth`; component: the command world imports `wasi:cli/environment` (dropped when
unused); lowering: `process`/`performance` values, the intrinsics, `f?.(...xs)`, `Object.create(proto,
descriptors)`, built-in module code doesn't set `Prelude::to_string`. Bugs fixed on the way: `import(x)` in a program
with CommonJS resolved to `undefined` (T54's token started `undefined`); a nested function called with a spread
argument got `f64` parameters (`closures::is_callee`); `scripts/diff.sh` takes `x.args` and `x.env`.

**Tests**: fixtures `tests/fixtures/node/{path,process,process-exit,process-exit-code,events,util,assert,os}.js`
and `commonjs/builtins` (with `emitter.js`), 9/9 equal node; frontend unit test `node_builtins`; `cargo test` all
pass (`unsupported.rs`: `Object.create(o, {})` lowers now); gating (`cmp.sh`, HEAD's compiler in a worktree): every
fixture without CommonJS or built-ins byte-identical (the CommonJS ones change: the shared error helper and the
token fix). Real packages equal node: `semver`, `lru-cache@10`, `inherits`, `minimist`, `picocolors`,
`eventemitter3` (`debug`, `commander`, `yargs-parser` need F238/F244). Full test262 subset on `a-03` (31,235 tests, 1,698 s): 27,826 of 28,799 run pass (96.6%), 3 unsupported → pass, 0 regressions (1 timeout under load passes alone).

**Unsupported / follow-ups**: F238 (other built-in modules, `Buffer`), F239 (`process`), F240 (`util`), F241
(`events`), F242 (`assert`), F243 (`path.win32`, `os`, `tty`), F244 (missing node globals in code that doesn't run),
F245 (global initializers' compile time), F246 (function source text of CommonJS modules). F237 partly: `Object.create`
with descriptors and `performance` done.

**Next smallest step**: `Buffer` and `buffer` (F238; `Buffer.` has ~1,500 uses in zigbee2mqtt), then `fs` on
`wasi:filesystem`; F245 before compiling large programs.

## Checklist
- [x] design questions decided and recorded
- [x] virtual-module mechanism + `path`
- [x] `process`, `performance`
- [x] `events`, `util`, `os` (`assert`)
- [x] fixtures, gating check, real-package probe
- [x] docs (design.md, keywords.md, README), follow-ups

## Progress
- 2026-09-30: handoff written after T54. State: `main` at `453b78b`, pushed; test262 subset 27,824 of 28,799 run
  pass (96.6%).
- 2026-09-30: started. `scripts/check-tools.sh`: all tools unchanged (wasmtime 49.0.1, node 24.21.0). Reading
  `link.rs`/`packages.rs`/`console.js` for the virtual-module design.
- 2026-09-30: design decisions (to record in design.md D4): (1) **virtual CommonJS modules**: each built-in
  js2wasi3 has is a CommonJS file shipped with the compiler (`crates/frontend/node/<name>.js`, `node.rs`), loaded by
  the linker at the path `node:<name>` for `x` and `node:x` (so `require`, `import`, `import()`, interop and
  identity come from T41/T54 unchanged); their code may use the prelude and intrinsics like the linker's generated
  code (`SourceMap::is_node_module` → `Program::is_generated`); a built-in's namespace has no `'module.exports'`
  key; `require.resolve("path")` is `"path"`. (2) **Source**: ports of node's own lib (MIT) where it's pure JS
  (`path` = node 24's lib/path.js POSIX half), our own otherwise. (6) `Buffer`: its own task.
- 2026-09-30: slice 1: `node.rs`, linker routes (`cjs_requests`, `resolve`, `dynamic_imports`, `read`, `format`),
  `node/path.js` (win32: named stubs that throw, same shape as node's), prelude `builtins/node.js`
  (`$$errInvalidArgType`/`$$nodeSpecificType`: node's `ERR_INVALID_ARG_TYPE`, now also used by `$$cjsCheckId`).
  Found and fixed a T54 bug: `$$moduleRequireToken` starts `undefined`, so in a program with CommonJS every
  `import(x)` without options took the synchronous-require path and resolved to `undefined` (the esm-import fixture
  only passed because an earlier `require` of an ES module had set the token). Scratchpad probes (ESM named/default/
  namespace/`import()`, CommonJS `require`/`node:`/`path/posix`/`require.resolve`, `console.log(path)`,
  `ERR_INVALID_ARG_TYPE` messages) equal node.
- 2026-09-30: the `$$DynamicImport` change is in an `__intrinsic_cjs_mode()` block (every built-in is CommonJS, so
  it's on whenever one is used): a `node:x` specifier that's in the table (keyed `dir\nx` by the linker) drops its
  prefix. Unknown `node:x` gives node's `No such built-in module: node:x`; `require.resolve("node:path")` keeps
  the prefix. Gating (`cmp.sh`, scratchpad; HEAD's compiler built in a worktree): 189 identical, 9 fail with both,
  11 differ = exactly the `commonjs/*` fixtures (the shared `$$errInvalidArgType` and the token fix);
  `scripts/diff.sh tests/fixtures/commonjs tests/fixtures/modules`: 25/25 pass. Fixtures `node/path.js` (ESM:
  join/normalize/dirname/basename/extname/relative/resolve/parse/format edge cases, errors, `import()`) and
  `commonjs/builtins` (identity, `require.resolve`, `console.log(path)`, unknown `node:x`): pass. Slice 1 done.
- 2026-09-30: slice 2 design (`process`): (3) a prelude object made on first use like `console` (`$$Process()` in
  `builtins/process.js`; `require("process")` is `module.exports = process`), not an `EventEmitter` instance (its
  own `on`/`once`/`emit`/…; `instanceof EventEmitter` is a follow-up). `env`/`argv`/`cwd()` from
  `wasi:cli/environment` through new intrinsics, imported only by programs that use them (like the clock).
  `platform` `"linux"` (packages take their POSIX paths; node has no WASI platform), `arch` `"wasm32"`, `version`
  `v24.21.0`. `argv` = [WASI arg 0, the entry's absolute path (build time), WASI args 1…]. `process.exit(n)`
  throws a second Wasm exception tag (only in programs that use it) that no JS `catch`/`finally` sees (the
  backend has no `catch_all`), caught by `run`/the timer tick, which stop timers and flush output; the exit status
  is 0 or 1 (`run`'s `result`): wasmtime 49 has `exit-with-code` only behind `-S cli-exit-with-code`
  (`@unstable`), so other codes are a follow-up. `emitWarning` emits `'warning'` but prints nothing, as node with
  `--no-warnings` (node's text has the pid).
- 2026-09-30: slice 2: `process`/`performance`. IR `Intrinsic::Environment(k)` (`host::Helper::Environment`:
  `get-environment`/`get-arguments`/`get-initial-cwd` with a return area from `cabi_realloc`, one NUL-separated
  string; imported only when used), `Intrinsic::ProcessExit` (`host::Helper::Exit`: a second tag
  `host::EXIT_TAG` and a host global `G_EXITED`, both only in commands that use `process`; `run` and the timer tick
  catch it and go straight to flushing; the loop runs no timers once it's set), `Intrinsic::MonotonicNow`,
  `Module::process_at_exit` (the loop calls the prelude's `$$processAtExit(status)` once before flushing: `'exit'`
  listeners, `exitCode`). Prelude `builtins/process.js` (`$$Process()`, `$$Performance()`), `node/process.js`
  (`module.exports = process`), `import wasi:cli/environment` in the command world (dropped when unused). A
  `BigInt(…)` call in an `__intrinsic_bigint_mode()` block linked BigInt's code into every program with `process`
  (invalid module: `$bigint` type missing): gated blocks must not name prelude functions; used the intrinsic.
  Scratchpad probes equal node: env/argv (`wasmtime --env`, args), `hrtime`, `performance.now`,
  `stdout.write`, `exitCode` validation (node's `ERR_INVALID_ARG_TYPE`/`ERR_OUT_OF_RANGE`), `process.exit` in
  `try/finally` (no `finally`, no timers or microtasks after), in a timer, in an `'exit'` listener, `exitCode = 1`,
  uncaught error + `'exit'` listener (statuses too). Real packages: `semver` equals node; `lru-cache@10` needed
  `f?.(...xs)` (optional call with spread, `chain_call_link` now uses `CallList`): equals node.
- 2026-09-30: `scripts/diff.sh`: `x.args` (arguments for both) and `x.env` (`NAME=value` lines: node's
  environment, wasmtime `--env`). `node.rs` `exports()`: explicit named exports for built-ins whose
  `module.exports` the lexer can't see (`process`). Fixtures `node/process.js` (+ `.args`, `.env`: argv, env,
  constants, clocks' shapes, writes, listeners, warnings, `exitCode` validation, `'exit'`),
  `node/process-exit.js`, `node/process-exit-code.js`: 4/4 `tests/fixtures/node` pass. Slice 2 done.
- 2026-09-30: slice 3: `node/events.js` (node 24's `EventEmitter`: listeners, `once`/`prepend*`, `newListener`/
  `removeListener`, `errorMonitor`, unhandled `'error'` (`ERR_UNHANDLED_ERROR`), `captureRejections`, max-listener
  warnings, `events.once`; `events.on`/`addAbortListener`/`EventEmitterAsyncResource` throw), `node/util.js`
  (`format`, `inspect` with `depth` (new `Intrinsic::InspectDepth`/`inspect::Helper::AtDepth`), `inherits`,
  `promisify` (+ `.custom`), `callbackify`, `deprecate` (DEP warnings as events), `debuglog`, `isDeepStrictEqual`,
  `types` (from built-in tags), `stripVTControlCharacters`, `toUSVString`, `isArray` (DEP0044)), `util/types`,
  `node/os.js` (placeholders for a Linux-like host, node's Linux `constants`), `node/assert.js` (+
  `assert/strict`; node's messages for primitives), prelude `$$errOutOfRange`. Named exports listed in
  `node.rs` (the lexer stops at call expressions). Scratchpad probes of each (ESM and CommonJS) equal node, except:
  `util.inspect` past depth 2 doesn't break lines as node's `compact: 3` (follow-up), `%o` on objects traps (as
  before in `console.log`), assert's generated messages for `assert(x)` (node quotes the call's source) and object
  diffs are approximations. Compiler bug found and fixed on the way: a nested function called only with a spread
  argument (`f(a, ...xs)`, every CommonJS module body is a function) got `f64` parameters (`is_callee` counted a
  spread call as a direct call, but it goes through the value): strings and objects became `NaN`.
- 2026-09-30: `Object.create(proto, descriptors)` as a call (F237 item): the value's `$$Object_create$value`, linked
  for such calls: the `inherits` package now equals node. More packages: `minimist`, `picocolors`,
  `eventemitter3` equal node; `node/tty.js` (`isatty` false, as node under pipes) for `debug`, which then stops at
  its browser file's `navigator` (a node global js2wasi3 rejects at build time, even in code that doesn't run:
  follow-up); `commander`/`yargs-parser` need `fs`/`child_process`/`module`.
- 2026-09-30: fixtures `node/{events,util,assert,os}.js` (+ `os.env`), `commonjs/builtins` extended (`emitter.js`:
  `util.inherits` over `EventEmitter`). The assert fixture timed out (10 s): wasmtime spent 9.2 s compiling the
  global initializers (`module_start`: 1,488 globals, 75,025 `i32.const`s of string literals, one 16 KB string):
  `util`/`assert` name `toString`, so every function kept its source text (T25), whole CommonJS modules. Built-in
  module code no longer sets `Prelude::to_string`: 455 → 307 KB, compile 10 s → 2 s. General issue (one init
  function for all string constants, superlinear in regalloc): follow-up. 9/9 node fixtures pass.
- 2026-09-30: user approved the full run on `a-03` and commit + push. Local targeted run stopped (superseded);
  full subset started on `a-03` (`~/t262-t55.log`).
- 2026-09-30: full test262 subset on `a-03`: 27,826 of 28,799 run pass (96.6%), 3 unsupported → pass (optional call
  with spread arguments, `Reflect/getPrototypeOf/skip-own-properties.js`, `for-in/order-enumerable-shadowed.js`),
  0 regressions (`S13.2.1_A1_T1.js` timed out under load, passes alone). Baseline and HISTORY.md updated. Done.
