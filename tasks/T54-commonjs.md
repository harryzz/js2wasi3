# T54: CommonJS modules (`require`, `module.exports`)

Status: done (2026-09-30) | Depends on: T41 (modules, linker, packages, `import()`
patterns), T42 (global object, sloppy `this` in eval code), F194

## Goal
Build programs and packages that node runs as CommonJS, with node 24.21.0's behaviour:
1. Which files are CommonJS, as node decides: `.cjs`; `.js` under a `package.json` with `"type": "commonjs"`; `.js`
   with no `"type"` and no ESM syntax (node 24's syntax detection); never `.mjs` or `"type": "module"`.
2. `require(specifier)` of relative files and packages known at build time (node's CommonJS resolution: extensions
   `.js`/`.json` (`.node` addons are an error), `index.js`, `main`, `exports` with the `require` condition),
   `module.exports`, `exports.x`, `module`, `__filename`, `__dirname`; each module evaluated once, a cycle sees the
   partial `module.exports`; JSON `require`s. Not `require.cache` manipulation.
3. Interop both ways: `import x from "./a.cjs"` (default is `module.exports`; named imports as node's
   cjs-module-lexer finds them) and `require("./esm.mjs")` (node 24: a synchronous ES module's namespace; top-level
   `await` is `ERR_REQUIRE_ASYNC_MODULE`).
4. `require` with a specifier not known at build time: T41's `import()` patterns (template/`+` patterns, `--include`),
   e.g. zigbee-herdsman-converters' `require("./devices/" + name)`.
Programs without CommonJS compile to the same bytes.

## What we know (at handoff)
- **Today** (`crates/frontend/src/link.rs`, "slice 5; F194"): a `.cjs` file, or a `.js` file in `node_modules/`
  without ESM syntax that references `module`/`exports`/`require`, in a package without `"type": "module"`, is the
  build error "unsupported: CommonJS module (…) (planned: F194)". Any other file is an ES module, strict. An entry
  `hello.js` outside a package is compiled as ESM though node 24 runs it as CommonJS: the same output unless it
  observes sloppy mode or top-level `this` (`{}` = `module.exports` in node, `undefined` here).
- **Linker** (T41, design.md D4): modules are joined at the source level (scope hoisting); `SourceMap` maps back;
  `import()` loads modules matched by specifier patterns or `--include` (slice 4), each as an evaluation function.
  `packages.rs` resolves with the `import` conditions (`node`, `import`, `module-sync`, `node-addons`, `default`);
  `package_type()` reads the nearest `"type"`.
- **Sloppy mode**: every compiled file is strict. CommonJS is sloppy unless it starts with `"use strict"`, which
  `tsc`/Babel output does. T42/T44 have partial sloppy support for `Function`/indirect-eval code (`this` only, F204).
- **Motivation**: [docs/zigbee2mqtt-feasibility.md](../docs/zigbee2mqtt-feasibility.md) blocker 1: nearly its
  whole graph is CommonJS (`tsc` output with `__importStar`/`__importDefault`, JSON `require`s, cycles, a computed
  `require` for device files). Most npm packages still ship CommonJS.
- **Next after this**: node's built-in modules (F200); `require("fs")` & co. stay build errors here.

## Design questions (decide first, record in docs/design.md D4)
1. **Representation**: each CommonJS module as a function `(exports, require, module, __filename, __dirname) => {…}`
   in the joined source (like esbuild's `__commonJS` wrapper), with a registry (evaluated flag + `module` object per
   module) and `require` a static call when the specifier is known (`$$require(id)`)? `module.exports` reassignment,
   `this` = `exports` at top level, cycles (a second `require` returns the partial exports). How does this fit
   T41's scope hoisting (no renames needed inside a function; the wrapper's names must not collide)?
2. **Sloppy mode**: (a) compile CommonJS as strict and reject sloppy-only constructs we can detect (`with`, octal
   literals, `arguments.callee`, assignments to undeclared names become global-object writes as in T42/T44, functions'
   `this` = global object when called plainly: detectable?), or (b) real sloppy semantics per function (a flag in
   the IR: `this` coercion, undeclared writes, `arguments` aliasing, Annex B block functions). Measure first: the
   share of CommonJS files starting with `"use strict"` in a real graph (zigbee2mqtt's `node_modules`, installed
   with `npm install --omit=dev --ignore-scripts` in the scratchpad). Recommendation at handoff: (a) plus a clear
   error, (b) only if the measurement says so.
3. **Detection**: node 24's rules exactly (`.cjs`/`.mjs`, `"type"`, syntax detection for typeless `.js`: ESM syntax
   = `import`/`export` declarations, `import.meta`, top-level `await`; ambiguous files are CommonJS). The entry too:
   does `hello.js` outside a package become CommonJS? (Changes nothing observable for most programs, but gating:
   the bytes of every typeless fixture would change if the wrapper is always emitted. Option: an entry with no
   CommonJS names and no top-level `this` stays compiled as ESM.) Fixtures live under `tests/fixtures/package.json`
   `"type": "module"`, so CommonJS fixtures need their own directory with a `package.json` (or `.cjs` files).
4. **Resolution**: CommonJS `require` conditions (`require`, `node`, `node-addons`, `default`); `require.resolve`;
   a `.node` file is an error (native addon); `require` of a node built-in (`fs`, `node:path`) stays the
   "unsupported: node built-in" error until F200.
5. **Interop**: ESM `import` of CommonJS: default = `module.exports`, named exports from a static scan
   (cjs-module-lexer's rules: `exports.x =`, `module.exports = { x }`, `Object.defineProperty(exports, "x", …)`,
   `__esModule` + `__exportStar` reexports); namespace printing like node (`[Module: null prototype] { default: …,
   x: … }`). `require` of ESM: the namespace object (T41's), `module.exports` interop key `"module.exports"`.
6. **Computed `require`**: reuse T41's `import()` pattern machinery; an unknown specifier throws node's
   `Cannot find module 'x'` (code `MODULE_NOT_FOUND`) at run time.
7. **Libraries**: a CommonJS entry with `module.exports` = a WIT export? (node has no such mapping; jco uses ESM.)
   Probably: a library entry must be ESM; its CommonJS dependencies are fine.

## Plan (slices; each ends with fixtures + a targeted test262 run where relevant)
1. Measurement (Q2) and detection (Q3): decide, record, fixtures of which files node treats as CommonJS.
2. The wrapper, registry and static `require` of relative files and JSON; `module.exports`/`exports`, cycles,
   `__filename`/`__dirname` (node prints absolute paths: decide what the component sees, e.g. the build-time path
   or a virtual root), errors thrown inside a module during `require`.
3. Packages with the `require` conditions; ESM ⇄ CommonJS interop (Q5), including `tsc`'s `__importStar`,
   `__importDefault`, `__exportStar` helpers.
4. Computed `require` (Q6).
5. Gating check (every fixture old vs new build, scratchpad `cmp.sh`), a scale probe on a real CommonJS package
   (e.g. `semver`, `js-yaml`, `ajv` from npm, built from the scratchpad), docs (design.md D4, keywords.md, README,
   HISTORY.md if test262 ran), follow-ups.

## Read first
- `crates/frontend/src/link.rs`: the module header doc, the CommonJS detection (search "F194"), how `import()`
  patterns and `--include` load modules, `SourceMap`.
- `crates/frontend/src/packages.rs`: resolution, `CONDITIONS`, `package_type`, `legacy_main`.
- `docs/design.md` D4 (T41 notes) and D6 (sloppy `this`, T44 undeclared `var`s in eval code).
- `tasks/T41-modules.md`: design questions and Notes / Result (how the join works, gating).
- `docs/zigbee2mqtt-feasibility.md` (the CommonJS patterns that matter).

## Tests
Fixtures under a new `tests/fixtures/commonjs/` (with its own `package.json`, no `"type"` or `"commonjs"`, plus
`.cjs`/`.mjs` files) via `scripts/diff.sh` (node is the oracle); `cargo test` (linker unit tests in `link.rs`);
`wasm-tools validate` + wasmtime. Gating: old vs new build of every fixture byte-identical except CommonJS ones.
test262 has no CommonJS tests; check `language/module-code` and `language/import` don't regress (targeted run).

## Acceptance
- [x] node's CommonJS detection, `require`/`module.exports`/`exports`, JSON, cycles, packages with `require`
      conditions behave like node (fixtures).
- [x] ESM ⇄ CommonJS interop and `tsc`-style CommonJS output work (fixtures).
- [x] Computed `require` works through patterns/`--include`.
- [x] Sloppy-mode decision recorded; what isn't supported is a clear build error with a follow-up.
- [x] Programs without CommonJS compile to the same bytes; 0 test262 regressions in the module directories.

## Notes / Result
**Decisions** (recorded in design.md D4): node's detection (`Format`: `.mjs`/`.cjs`, nearest `"type"`, else ESM
syntax); an entry that can't tell stays ESM (same bytes); CommonJS compiled as **strict** code (measurement: 81% of
zigbee2mqtt's CommonJS files, 94% of the bytes, are `"use strict"`, none of the rest uses sloppy-only syntax;
sloppy semantics: F232); a CommonJS file is the body of node's wrapper, joined as `function cjs$k(exports, require,
module, __filename, __dirname)` with a generated table and a prelude loader (`builtins/cjs.js`); `__filename` is the
build-time absolute path (like `import.meta`); a library entry must be ESM (a CommonJS entry has no ESM exports, so
it's a command). Unlike `import()`, a computed `require` the build can't work out isn't a build error, and a
node built-in `require` is one only where it always runs.

**What changed**: `crates/frontend/src/link.rs` (detection, `parse_cjs`, `cjs_requires`/`cjs_computed`/
`cjs_exports`, bindings `Cjs`/`CjsValue`/`CjsNamed`/`CjsTable`, the table, `$$cjsKnown`, the ESM snapshot `const`s,
`$$cjsMain`; `.cjs` in `import()` patterns); `packages.rs` (`resolve_require`, `load_as_file_or_directory`, the
`require` conditions); `eval.rs` (`EvalMap::first`); `lib.rs` (`Program::is_generated` never in the prelude);
`crates/lower/builtins/cjs.js` (new), `module.js` (`$$moduleRequire`, `__intrinsic_cjs_mode()` blocks),
`object.js` (`$$Object_create$value`); `crates/lower/src/builtins.rs`/`lib.rs` (`__intrinsic_cjs_mode`); two
lowering fixes CommonJS exposed (`hoist`: TDZ cells for destructured bindings; `closures.rs`: a class expression's
own name inside a function); `scripts/diff.sh` (`commonjs/<case>/main.js`), `scripts/test262.mjs` (work dir
`"type": "module"`).

**Tests**: 12 fixtures under `tests/fixtures/commonjs/` (basic, cycle, resolve, errors, detect, packages, tsc (real
tsc 7.0.2 output), esm-import, require-esm, computed, include) + `closures/function-scope-tdz.js`, all equal to
node; frontend unit tests (23: `commonjs_detection`, `commonjs_requires`, `commonjs_export_names`); gating
(`cmp.sh`, every fixture built by the old and the new compiler): identical except the CommonJS fixtures and
`builtins/set-prototype-of.js` (it uses `Object` as a value and names `create`, which is now on it); real
packages `ms`, `deepmerge`, `fast-deep-equal`, `balanced-match`, `brace-expansion`, `yallist`, `sprintf-js` equal
node. Targeted test262 (`language/module-code`, `language/import`, `language/export`, 949 tests, laptop -j 4): 594 pass, 9 fail, 9 unsupported, no change from the baseline. Full subset on `a-03` (31,235 tests, 1,691 s): 27,824 of 28,799 run pass (96.6%), 0 regressions (1 load timeout passes alone).

**Unsupported / follow-ups**: F232 (sloppy mode), F233 (`['__proto__']` printing), F234 (`module`/`require`
extras), F235 (lexer gaps, ESM ⇄ CJS cycles), F236 (computed `require` leftovers), F237 (built-ins the probe
needed); `process`/`performance` and `require("fs")` & co.: F200/F112.

**Next smallest step**: node's built-in modules (F200): the real packages stop at `process` (`semver`,
`lru-cache`) and `util`/`events`/`path` first.

## Checklist
- [x] design questions decided and recorded (after the measurement)
- [x] detection
- [x] wrapper, registry, static `require`, JSON, cycles
- [x] packages (`require` conditions), interop
- [x] computed `require`
- [x] fixtures, gating check, real-package probe, targeted test262 run
- [x] docs (design.md, keywords.md, README, HISTORY.md if run), follow-ups

## Progress
- 2026-09-30: handoff written after T53. State: `main` at `94cfd65`, pushed; test262 subset 27,824 of 28,799 run
  pass (96.6%).
- 2026-09-30: started. `scripts/check-tools.sh`: all tools unchanged (wasmtime 49.0.1, node 24.21.0). Slice 1:
  measuring `"use strict"` share in zigbee2mqtt's `node_modules` (scratchpad install).
- 2026-09-30: measurement (Q2), `npm install zigbee2mqtt --omit=dev --ignore-scripts` (114 packages) in the
  scratchpad, files classified with node's rules (`.cjs`, nearest `"type"`, else `vm.compileFunction` parses = CJS):
  1,694 CommonJS files, 342 ESM. 1,373 CommonJS files (81%; 94% of 21.6 MB) start with `"use strict"`. The 321
  sloppy ones (mostly `@babel/runtime` helpers, `@colors/colors`, `readable-stream`, `events`, `debug`) all compile
  with `"use strict"` prepended (the 7 failures were `#!` bin files, my prefix broke the shebang): no `with`, octal
  literals or other strict early errors. Decision: option (a), compile CommonJS as strict; sloppy-only run-time
  differences (plain-call `this` = global object, undeclared writes creating globals, `arguments` aliasing) are a
  follow-up.
- 2026-09-30: slice 2 first cut. `link.rs`: `Format` (node's rule: `.mjs`/`.cjs`, nearest `"type"`, else
  detection: ESM syntax = import/export statements, `import.meta`, top-level `await`); an ambiguous entry that
  doesn't observe CommonJS (`module`/`exports`/`require`/`__filename`/`__dirname`/`arguments`, top-level `this`) stays
  an ES module (same bytes). CommonJS files are parsed inside node's wrapper (`CJS_PRE`/`CJS_POST`, offsets mapped
  back through an `EvalMap` layer), joined as `function cjs$k(exports, require, module, __filename, __dirname)`,
  with a table `modules$1 = [[fn, filename, dirname, [[spec, index]…], module, parent]…]` and `$$cjsMain(table, k)`
  after a CommonJS entry; static `require("…")` resolved with LOAD_AS_FILE/LOAD_AS_DIRECTORY, JSON as
  `module.exports = <JSON>`. Prelude `builtins/cjs.js`: `$$cjsLoad` (once, cycles, dropped from the cache on throw),
  `require` (static table, node's `MODULE_NOT_FOUND` with the require stack, `ERR_INVALID_ARG_TYPE`),
  `require.resolve`/`main`, `module.parent` getter on a `Module` prototype, `paths`. Bug found: a linker insertion
  at the very end (`$$cjsMain(…)`) made `SourceMap::is_inserted` true for the whole prelude, so every prelude
  reference became a root (all 499 functions linked, BigInt code without the BigInt types: invalid module); fixed in
  `Program::is_generated` (never in the prelude). Scratchpad programs (cycle, JSON, not found, throw + re-require,
  `module.parent`) match node.
- 2026-09-30: fixtures `tests/fixtures/commonjs/{basic,cycle,resolve,errors,detect}` (own typeless
  `package.json`; `scripts/diff.sh` treats a `commonjs` dir like `modules`: `<case>/main.js`): 5/5 pass. Own
  `__proto__` keys print as `__proto__:` instead of node's `['__proto__']:` in every program (inspect, not CommonJS):
  follow-up F233, kept out of the fixture. `require.resolve("lit")` specifiers are static requests too. An ES module
  importing a CommonJS one is a build error until slice 3 (its function would never run). Sloppy-only syntax in
  CommonJS: the build error gets a help pointing at F232 when the wrapped text parses as a sloppy script.
  `cargo test -p js2wasi3-frontend` 22 pass (new: `commonjs_detection`, `commonjs_requires`). test262 runner: the
  work dir gets `package.json` `{"type": "module"}` (tests stay ES modules under node's detection).
- 2026-09-30: gating check (scratchpad `cmp.sh`: every fixture entry built by `HEAD`'s compiler, from a scratch
  worktree, and the new one): 188 identical, 8 fail with both (rejection fixtures), 5 differ: the new
  `commonjs/*` ones only. `wasm-tools validate` ok. Slice 2 done. node interop probes for slice 3 (scratchpad
  `interop/`): ESM default = `module.exports`, named exports from the lexer (`exports.a`, `module.exports.b`,
  `exports['d-e']`, even in dead code; not a `defineProperty` getter), snapshot values, namespace with
  `'module.exports'` and `__esModule` keys; `require(esm)` = namespace (plus `__esModule: true` when it has a
  default), or its `"module.exports"` export when there is one.
- 2026-09-30: slice 3a: `require` of packages. `packages::resolve_require` (node's CommonJS LOAD_NODE_MODULES:
  `exports`/`imports` with the `require` conditions `node`, `require`, `module-sync`, `node-addons`, `default`;
  without `exports`, LOAD_AS_FILE/LOAD_AS_DIRECTORY of `node_modules/X`; self-reference) and
  `packages::load_as_file_or_directory` (moved from the linker). A package that isn't there is `-1` in the table
  (`MODULE_NOT_FOUND` when it runs: optional dependencies in `try` work); other resolution errors are
  `[spec, -1, code, message]`, thrown with node's code. Fixture `commonjs/packages` (conditions, nested
  conditions, `main`, subpaths, JSON, scoped, `#imports`, not exported, not installed): 6/6 commonjs fixtures pass.
- 2026-09-30: slice 3b: ES modules import CommonJS. After loading, each CommonJS module gets `locals`
  (`default`/`'module.exports'` → `Binding::CjsValue`, each name → `Binding::CjsNamed`), so ResolveExport,
  namespaces, `export … from` and `import()` work unchanged. Names: `cjs_exports()`, an AST version of
  cjs-module-lexer (`exports.x`/`module.exports.x`/`["x"]` assignments, `module.exports = {…}` with the lexer's
  stop rules probed against node, `Object.defineProperty(exports, "x", {value | get() { return a.b }})`,
  re-exports `module.exports = require(…)`, `...require(…)`, `__exportStar(require(…), exports)`/`__export`),
  re-exports followed transitively. Emitted after the module's function: `const exports$k = $$cjsImport(table, k)`
  and `const x$k = $$cjsNamed(exports$k, "x")` (snapshots, as node). node's message for a missing named export.
  `import()` patterns and `--include` accept `.cjs`. tsc's helpers read `Object.create` as a value:
  `$$Object_create$value` added (with ObjectDefineProperties for the second argument). Fixtures
  `commonjs/tsc` (real tsc 7.0.2 output: `__importStar`, `__importDefault`, `__exportStar`, `__createBinding`) and
  `commonjs/esm-import`: 8/8 commonjs fixtures pass; frontend unit tests 22 pass.
- 2026-09-30: slice 3c: `require` of ES modules. A CommonJS request of an ES module is a table row
  `[spec, -3, evaluation index, namespace, filename]`; such a program uses T41's evaluate mode (every module a
  function run by `$$EvaluateModules`), and `$$moduleRequire(m)` (module.js) evaluates it synchronously: a graph
  with top-level `await` runs nothing and throws node's `ERR_REQUIRE_ASYNC_MODULE` (message with the require stack);
  a failed evaluation throws the same error again. `__esModule: true` namespace (made once) when it has a default
  export; its `"module.exports"` export when it has one. Bug found on the way: the nested evaluation reset the
  shared DFS `stack` of the evaluation running the CommonJS entry, so the outer `while (!done) stack.pop()` spun
  forever; fixed (stack saved/restored, nested DFS indices after `n`). `$$cjsLoad` rethrows from a `catch` instead
  of a flag set in `try` and read in `finally` (F224's pattern). Fixture `commonjs/require-esm`: 9/9 pass.
- 2026-09-30: slice 4: computed `require`. `cjs_requires` also returns non-literal calls with T41's `import()`
  patterns; `cjs_computed` expands relative patterns (adding `.js`/`.json` when the pattern has no extension, as
  LOAD_AS_FILE does), loads the files (JSON as CommonJS JSON) and records them in `cjs_known` (absolute path →
  module); `--include`d modules join it when the program has CommonJS. Generated `$$cjsKnown([...])`; at run time
  `require` of a specifier with no static row resolves it against the module's directory (`$$normalizePath`, then
  `""`/`.js`/`.json`/`/index.js`/`/index.json`) among those. Decision: unlike `import()`, a specifier the build can't
  work out is not a build error (CommonJS code does this on paths that don't run: plugins, `bindings`,
  `node-gyp-build`; the `errors` fixture's `require(123)`); when it runs it finds only what the build has, else node's
  `MODULE_NOT_FOUND` plus a second line pointing at `--include`. Fixtures `commonjs/computed` (`+`, template, `?:`,
  `const` patterns, JSON, index, missing) and `commonjs/include` (`--include 'plugins/*'`, a CommonJS and an ES
  module): 11/11 commonjs fixtures pass; frontend unit tests 22 pass.
- 2026-09-30: gating. The first `$$moduleRequire` (a closure in `$$EvaluateModules`, then a gated block
  publishing `inner`) changed the bytes of every program using the module evaluator (`modules/tla`, `dynamic`, …):
  the closure analysis (captures, `fn_values`) runs over dropped `if (__intrinsic_…())` blocks too, so `n` became a
  cell and `inner`'s `index` a `jsval`. New gate `__intrinsic_cjs_mode()` (`Prelude::cjs`: the joined source has the
  linker's `$$cjs…` code) around blocks that hold no functions and use only what their function captures anyway:
  `evaluate(-m - 1)` is the synchronous require evaluation, `$$moduleImport` forwards to it for a private token,
  and the top-level `$$moduleRequire` does the top-level-`await` check on `$$moduleState = [tla, requests, n]`.
  `cmp.sh`: all non-CommonJS fixtures identical except `builtins/set-prototype-of.js`, which uses `Object` as a
  value and names `create`: `Object.create` is now on it (`$$Object_create$value`; before it was missing,
  `const o = Object; o.create` was `undefined`). 11/11 commonjs fixtures pass.
- 2026-09-30: real-package probe (scratchpad `real/`, npm latest). Two lowering bugs that CommonJS exposes (a
  module body is a function) fixed: (1) a closure made before a *destructured* `const`/`let` it captures, inside a
  function (`class C { m() { re } } const { safeRe: re } = require(…)`, semver) was "closure capturing before its
  declaration": `hoist` now makes TDZ cells for every name a pattern binds; (2) a named class expression referring
  to itself from a method inside a function (`var X = class X { … new X … }`, every rollup/Babel bundle,
  js-yaml) was "internal: a closure without its function object": the capture walk for a class's own name now
  stops at the function evaluating the class (`closures.rs`, `class_owner`). `require` of a node built-in is a
  build error only where it always runs (module top level, not in `try` or a function); elsewhere
  (`try { require('util') } catch`, `inherits`) it throws `ERR_UNKNOWN_BUILTIN_MODULE` when it runs. The sloppy
  hint now needs the strict parse to fail. Result: `ms`, `deepmerge`, `fast-deep-equal`, `balanced-match`,
  `brace-expansion` (requires `balanced-match` as a package), `yallist`, `sprintf-js` match node; `semver` needs
  `process` (F200), `lru-cache` `process`/`performance` (F200), `inherits` `Object.create(p, descriptors)` as a
  call, `js-yaml` `atob`/`btoa`, assignment to a function binding, `Object.defineProperties`,
  `Object.getOwnPropertyDescriptors` as a value; `ajv` `new Function` of generated code (F191).
- 2026-09-30: targeted test262 `language/module-code language/import language/export`: 594 pass / 9 fail / 9
  unsupported of 949, "No changes since the baseline". Docs (design.md D4/D6/Testing, README, keywords.md),
  follow-ups F232–F237, F194 ticked. Done.
- 2026-09-30: full test262 subset on `a-03`: 27,824 of 28,799 run pass (96.6%), 0 regressions (`statements/function/S13.2.1_A1_T1.js` timed out under load, passes alone). HISTORY.md updated.
