# T41: Multi-file ES modules (F7, F79)

Status: in progress (started 2026-09-28) | Depends on: T20 (user worlds: WIT imports), T21 (prelude)

## Goal
A program can be several files: static `import`/`export` between them (named, default, namespace, re-exports,
`export *`), live bindings, cycles, the spec's evaluation order, top-level `await` in a dependency; then dynamic
`import()` of modules known at build time. The entry file's exports stay the component's WIT exports; the other
modules' exports are internal. A one-file program compiles exactly as before (same IR/Wasm).

## What we know (at handoff)
- The whole pipeline assumes **one oxc `Program`**: `frontend::parse_with_prelude` (`crates/frontend/src/lib.rs`)
  parses the user's file, then parses it again with the built-ins prelude appended as one module
  (`Program::user_end`/`in_prelude` split the offsets; prelude errors are remapped to `<builtins>`). The lowering,
  the closure analysis, the prelude scan (`crates/lower/src/builtins.rs`, `Prelude::new`) and every diagnostic use
  that one `Program`'s semantic (symbols, scopes, spans → `line_col`).
- Today (`crates/lower/src/lib.rs`, pass 1 around `Statement::ImportDeclaration`): `declare_import` accepts only a
  user world's interface imports (`import { f } from "ns:pkg/iface"`, T20 slice 5, specifiers with a `:`); any
  other import is "import of another module (multi-file modules, F7)"; re-exports and `export *` are rejected;
  `import()` is `expression ImportExpression`, `import.meta` rejected (F79). The CLI (`crates/js2wasi3/src/main.rs`)
  reads one entry file.
- Dynamic `import(x)` needs no JS engine when the possible modules are known at build time (decided with the user,
  2026-09-28): every bundled module goes in a table keyed by its resolved specifier; `import(v)` looks `v` up at run
  time, evaluates that module once and returns a promise of its namespace; unknown → a rejected promise (node's
  `ERR_MODULE_NOT_FOUND` text). What's in the table: literal specifiers, constants, `cond ? "./a.js" : "./b.js"`,
  template patterns (``import(`./locales/${l}.js`)`` → every matching file, as esbuild/webpack do), and later a
  `--include <glob>` option for specifiers the compiler can't work out. An `import()` it can't resolve and no
  `--include` covers is a build error (never a program that always fails). Code that doesn't exist at build time
  (plugins, user paths) is out of scope for good: like `eval` (it would need an engine inside the output, or a
  second component, and WASI 0.3 can't load components at run time).
- test262 (subset, baseline at T40): 1,004 tests contain `import(` (536 with a literal specifier): 135 pass
  (mostly syntax/early-error tests), 416 unsupported, 453 skipped; 32 use static imports (27 skipped, 5
  unsupported). The runner (`scripts/test262.mjs`, `skipReason`) skips `_FIXTURE.js` files and
  `negative.phase: resolution`; it writes each test (harness + test) to a temp file, so relative imports of
  `*_FIXTURE.js` files don't resolve: it must compile the test next to its fixtures (or copy them). Many
  `import()` tests also need other missing features (`eval`, `globalThis`, `$262`): not all will pass.
- Node's ESM resolution: relative/absolute specifiers need the extension (`./x.js`; `./x` is an error in node,
  keep it one); `node:` built-ins aren't available (reject: "`node:fs` isn't available"); bare specifiers
  (`lodash`) go through `node_modules` and `package.json` `exports`/`module`/`main` (later slice). Specifiers with
  `:` that name a WIT interface (`ns:pkg/iface`) stay user-world imports.

## Prior art: scriptc (vercel-labs/scriptc, checked 2026-09-28)
TS/JS → typed IR → C / LLVM → native or WASI **P1** wasm; TypeScript's checker drives the types; refcounting with
cycle collection at fixed points; node-vs-binary byte comparison like our `diff.sh`. What matters here:
- The module graph is fixed at build time; **literal `import()` of compiled modules needs no engine**; computed
  specifiers need their `--dynamic` mode. Same line as our plan.
- npm packages: resolved with Node's algorithm (`package.json` `exports` conditions) **at build time**; the
  binary never reads `node_modules`. They shim some Node built-ins (`node:events`, `node:path`, `node:process`):
  worth considering in slice 5 instead of rejecting every `node:` specifier.
- `--dynamic` embeds quickjs-ng (~620 KB) as a "dynamic island" for npm JS, `any`-typed code and `eval`; values
  cross **by copy** (mutations on one side are invisible to the other; class instances and promises can't cross;
  cycles across the boundary leak). An option for us later, not in this task: F191.
- `scriptc coverage`: a per-site report of what compiles statically. A `js2wasi3 check`/coverage over a whole
  module graph would help once programs are multi-file (idea; not required here).

## Design questions (decide first, record under D4/D5 in docs/design.md)
1. **Where modules are joined.**
   (a) *Scope hoisting at the source/AST level* (Rollup/esbuild style): rename each module's top-level bindings so
   they're unique, replace imported names by the exporting module's (renamed) bindings, concatenate the modules
   in evaluation order (post-order DFS) before the prelude, and keep feeding one `Program` to the lowering. Live
   bindings and cycles come for free (same variable; TDZ checks as today), function hoisting across a cycle works;
   the lowering barely changes. Needs: text edits by span (or oxc codegen), a map from joined offsets back to
   (file, line, col) for diagnostics (generalize `user_end`), and care with `export default <expr>` (a synthetic
   binding), `import * as ns` (a namespace object: a frozen, null-prototype object with getter-like live access;
   or resolve `ns.x` statically when `ns` is only used as `ns.<name>`), `export *` conflicts, names shadowing
   renamed ones, `this`/top-level `await` per module.
   (b) *Lower each module separately and link in the IR*: a `Program` per file, module-scoped globals, cross-module
   references as globals. Cleaner per-module errors but every piece of the lowering that assumes one semantic (the
   closure analysis, TDZ flags, prelude scan, facts/rounds) would need a module dimension. Recommendation at
   handoff: (a); check how oxc's semantic can drive the renames (symbol → references) and whether `oxc_codegen`
   or span edits are simpler.
2. **Evaluation**: each module's top-level code runs once, in the spec's order (dependencies first, cycles by DFS
   order); a module with top-level `await` makes its dependents wait (async module evaluation). Static graph: one
   `main`/`init` running the modules in order (a module with TLA already makes `main` async today, T19). Modules
   reached only through `import()`: an evaluation function per module (a flag + a promise), called by the lookup.
3. **Namespace objects** (`import * as ns`, `import()` results): representation (an object with accessors reading
   the live bindings, `Symbol.toStringTag` "Module", null prototype, non-extensible), printed like node
   (`[Module: null prototype] { a: 1 }`).
4. **Gating**: a single-file program must compile to the same bytes (check with an old-vs-new build of every
   fixture, as T40 did; beware F186, the three fixtures whose output is already nondeterministic).
5. **Libraries**: exports of the entry only; a dependency's top-level code joins `init` (T20 slice 2).

## Plan (slices; each ends with fixtures + a targeted test262 run)
1. **Static imports, relative specifiers**: resolve and load the graph (CLI: the entry's directory; cycles),
   named/default imports and exports, `export { a as b }`, `export default` (function, class, expression),
   evaluation order, live bindings, cycles with hoisted functions and TDZ errors, diagnostics with the right file
   and position, rejections: bare specifiers, `node:`, missing files/exports (node's SyntaxError text for a
   missing export: "The requested module './x.js' does not provide an export named 'y'").
2. **Namespaces and re-exports**: `import * as ns`, `export * from`, `export * as ns from`, `export { x } from`,
   ambiguous `export *` names, namespace objects (printing, `Object.keys`, `in`, TypeError on writes).
3. **Top-level `await` in dependencies** (async evaluation order), `import.meta` (`url` as `file:///…` of the
   source path? decide; node gives the absolute path), JSON modules (`with { type: "json" }`) if cheap.
4. **Dynamic `import()`** of known modules: literals, constants, conditionals, template patterns (a glob relative
   to the importing file), the lookup table, lazy evaluation, rejected promises for unknown specifiers,
   `--include <glob>`; build error for unresolvable ones.
5. **Bare specifiers** (`node_modules`, `package.json` `exports` conditions `import`/`default`, `module`, `main`)
   — maybe a separate task; also the test262 runner changes (compile tests in place, stop skipping resolution-
   phase negatives) can land with slice 1.

## Read first
- `docs/design.md` D4 (the entry's exports → WIT, library `init`) and D5 (the prelude as one module with the user's
  code, `builtins::Prelude`).
- `crates/frontend/src/lib.rs` (`parse`, `parse_with_prelude`, `Program`, `collect_exports`, diagnostics mapping).
- `crates/lower/src/lib.rs`: the crate docs, `lower_with` (rounds), pass 1 (`Statement::ImportDeclaration`,
  `declare_import`, exports), `main`/`init` construction, `undeclared`/`resolve`; `crates/lower/src/builtins.rs`
  (`Prelude::new`: what counts as user code).
- `crates/js2wasi3/src/main.rs` (build command), `scripts/test262.mjs` (`skipReason`, how a test file is written).
- `tasks/T19-async-await-event-loop.md` (top-level `await`), `tasks/T20-wit-async-user-worlds.md` slices 2 and 5
  (library `init`, WIT imports) — Notes / Result only.
- F7, F79, F186 in `tasks/README.md`.

## Tests
Fixtures in `tests/fixtures/modules/<case>/main.js` with their sibling files (`scripts/diff.sh` runs node on the
entry; check that it takes a directory's `main.js` without treating the siblings as fixtures — adjust it, e.g.
only `main.js` in `tests/fixtures/modules/*/`), a library case under `crates/js2wasi3/tests/build.rs`, `cargo test`,
`wasm-tools validate` + a wasmtime run. test262: the tests using `import`/`import()` in the subset (select by
content like T40 did; `language/module-code`, `language/expressions/dynamic-import`, `language/import` — consider
adding `language/module-code` to the subset and sparse checkout). Whole subset at the end (ask before using
`a-03`; or a targeted `--update` over every test whose output can change, as T40 did). Ask the user before
committing the baseline.

## Acceptance
- [x] One-file programs compile exactly as before (same bytes for the existing fixtures, modulo F186).
- [x] Multi-file fixtures match node: named/default/namespace imports, re-exports, live bindings, cycles, TDZ,
      evaluation order, top-level `await` in a dependency, errors (missing file/export) at build time.
- [x] `import()` of build-time-known modules (literal, constant, conditional, pattern, `--include`) works; an
      unresolvable one is a build error; unknown at run time → a rejected promise like node's.
- [x] test262: the runner resolves fixtures; the affected tests' results are in the baseline; 0 regressions.

## Notes / Result
**Slices 1–2 (2026-09-28).** Design: (a) scope hoisting, recorded in `docs/design.md` D4.
- `crates/frontend/src/link.rs`: loads the graph (relative specifiers resolved lexically against the importer's
  path; node's rules: extension required, `node:` and bare specifiers rejected, WIT `ns:pkg/iface` specifiers left to
  the lowering), ResolveExport (locals, indirect, stars, cycles, ambiguity), InitializeEnvironment's checks (node's
  SyntaxError texts at build time), names (a binding keeps its name unless it clashes: another module's top-level
  name, a global some module reads, a nested binding where an importer's reference would be captured; new names
  `x$1`), text edits by span (imports deleted, dependencies' `export` keywords removed, `export default <expr>` →
  `const default$1 = …`, shorthand `{ x }` → `{ x: x$1 }`), namespace objects first, modules in post-order joined
  with `\n;\n`. `SourceMap`: segments (joined offset → file offset), original names, written names at replaced
  references, import references, the generated prefix.
- `frontend::Program`: `map`, `locate`, `original_name`, `written_name`, `is_import`, `is_generated`;
  `parse_linked`. `lower::parse_entry(alloc, path, source, command, read)` (CLI `build`, `ir`).
- Lowering: diagnostics through `locate`; `Var.guard` carries the reference offset (TDZ message names what the
  importer wrote); `Var.constant` (a `const` or an import: writes throw); function/class `name` through
  `original_name`; generated code may call the prelude.
- Runtime (gated on `$module`): `module_new` / `CLASS_MODULE`, descriptors, write errors, `isFrozen`, tag,
  printing. Prelude: `builtins/module.js`.
- CLI: `build --command`. Scripts: `diff.sh` runs `modules/<case>/main.js`; `test262.mjs` compiles module tests
  in place with their imports linked, runs resolution negatives, uses `--command`; three module dirs in the subset.
- Tests: fixtures `tests/fixtures/modules/{basic,live,cycle,tdz,names,defaults,namespace}` (all match node),
  `modules/library/lib.js` (build test), `crates/js2wasi3/tests/build.rs` (`library_from_several_modules`,
  `module_errors_are_diagnostics`), `frontend` unit tests (`link::tests`). Single-file fixtures: byte-identical
  (160 of 162; the other 2 are F186 and give only old variants).
- test262: module-code 88.4% (namespace 31/38, top-level-await 237/250), subset 25,556 of 28,118 run (90.9%),
  0 regressions.
- Unsupported / follow-ups: F192 (TLA order), F193 (namespace internals), F194 (CommonJS detection), F195
  (`toString` text, positions), F196 (runner harness), F197 (namespace exports from a library).
- (Slice 3's plan, as written at the end of slice 2) F192. Idea: keep declarations shared, but let the lowering build `main` from
  per-module segments (the linker can record each module's statement range): a module with top-level `await`,
  or one waiting on such a dependency, becomes an async step started when its dependencies' promises settle, the
  others run in order at once (the spec's async module evaluation). Then `import.meta` (`url` as `file://` of the
  source path?) and JSON modules (`with { type: "json" }`: a module whose default export is the parsed value).

**Slice 3 (2026-09-28).** Top-level `await` in dependencies, `import.meta`, JSON modules.
- F192: when a command's dependency (not the entry) has top-level `await`, `link` records each module's joined range
  in evaluation order (`SourceMap::modules`) and the generated prefix ends with `await
  $$EvaluateModules(__intrinsic_module_bodies(), [tla…], [[requests…]…])`. The lowering (`Lowerer::module_bodies`,
  `lower_top`, `is_top_code`) makes each module's top-level statements a function `module$k` (globals shared, TDZ as
  in `main`, an `await` makes it async); the intrinsic `module_bodies` (allowed in generated code) is an array of
  their function objects. `builtins/module.js` `$$EvaluateModules`: the spec's Evaluate / InnerModuleEvaluation
  (Tarjan over cycles) / ExecuteAsyncModule (an async wrapper awaiting the body, no observable `then`) /
  AsyncModuleExecutionFulfilled + GatherAvailableAncestors / Rejected. Ticks match node against promise chains.
  Libraries keep one synchronous `init` (a TLA dependency is rejected, as before); programs without an async
  dependency keep one `main` (same bytes).
- `import.meta`: `Binding::Meta`; each use is replaced by a generated `const meta$k = $$ImportMeta(url, filename,
  dirname, main)` (canonical absolute path at build time; `url` as `pathToFileURL`), an extensible null-prototype
  object with node's keys in node's order; `resolve` (prelude) handles `./`, `../`, `/` (URL-encoded, dot segments),
  URLs as they are, bare specifiers → `ERR_MODULE_NOT_FOUND` like node without the package. A one-file program using
  `import.meta` now goes through the linker.
- JSON modules: attributes checked in `load` (only `type: "json"`); `.json` without it, it on a non-`.json` file and
  other attributes are build errors (node's ERR_IMPORT_ATTRIBUTE_* wording); `json_module` validates (RFC 8259, BOM
  stripped, errors at the file's line/col) and gives `export default <text>;` with `"__proto__"` keys (also escaped
  ones) computed, so they stay own properties.
- Tests: fixtures `modules/{tla,tla-error,tla-cycle,meta,json}` (all 13 module fixtures match node), frontend tests
  `async_dependency_makes_module_functions`, `import_meta_is_a_generated_binding`, `json_modules`, build tests
  (`library_from_several_modules`: JSON + `import.meta` in a library; `module_errors_are_diagnostics`: a TLA dependency
  of a library). `cargo test --no-fail-fast`: all pass. Gating: every fixture the old compiler builds is
  byte-identical except the 3 TLA fixtures and F186's 3 (new output among the old variants).
- test262: targeted `--update` (1,351 tests): 0 regressions, 25 unsupported → pass; `import-text`/`import-bytes`
  skipped (node 24 lacks them); `import.meta/syntax/goal-script.js` fails (a script-goal early error; every file is
  a module here). The TLA-order tests need `globalThis`/`import()`; three checked by hand with a stand-in module
  match node. Subset 25,581 of 28,108 run (91.0%), module-code 90.9%.
- Unsupported / follow-ups: F198 (`console.log` of an own `__proto__` key). Commands only for TLA dependencies
  (a library's `init` is synchronous: would need async `init` with async exports).
- **Next smallest step (slice 4)**: dynamic `import()` with a string-literal specifier: the linker collects the
  literals, loads those modules into the graph (not evaluated up front), and the lowering turns `import("./x.js")`
  into a call of a prelude `$$DynamicImport(k)` that evaluates module `k` once through `$$EvaluateModules`' machinery
  (module functions are already there when a module is async; for dynamic ones every module in the graph becomes a
  function) and returns a promise of its namespace. Then constants, conditionals, template patterns, `--include`.

**Slice 4 (2026-09-29).** Dynamic `import()` of modules known at build time.
- Linker (`frontend::link`): `Host` (read + list directories; `Fs`), `Options { command, include }`. Each `import()`'s
  specifier patterns (`patterns`: string literals, template literals and `+` with `*` for the unknown parts, both sides
  of `?:`/`||`/`??`, `const`s initialized with those), matched against the files (`expand`/`walk`/`glob_match`: `*`
  within a segment, `**` in `--include`; `.js`/`.mjs`/`.json` only). Missing files and bare/`node:` literals are left
  out (they reject at run time, as in node). An `import()` without a relative pattern is a build error unless
  `--include` is given. The modules only `import()` reaches load after the static graph (`lazy`): their parse, link and
  resolution errors mark them `failed`, and every table entry whose static closure reaches one is `poisoned` (not in
  the program; rejects with the error). The table: [absolute path, module, JSON, namespace] or [path, -1, JSON, null,
  class, message]. With any `import()` every module is a function (commands `await` the evaluator, a library's `init`
  runs it synchronously); `import(x, o)` → `$$DynamicImport(k, x, o)`; inserted text may call the prelude
  (`SourceMap::is_inserted`).
- Prelude: `$$EvaluateModules(bodies, tla, requests, paths, table, entry, sync)`: the spec's Evaluate with a
  capability per module; `$$moduleImport`: EvaluateImportCall's specifier/options checks, resolution against the
  importer's absolute directory, node's errors (`ERR_MODULE_NOT_FOUND` for files and packages,
  `ERR_IMPORT_ATTRIBUTE_*`), the load one job later (a synchronous load ran the module before the importer's next
  statement, and a nested Evaluate reset the outer DFS stack: a hang), resolve with the namespace.
- CLI: `build --include <glob>` (repeatable). `scripts/diff.sh`: `x.flags` (extra build options, built from the
  fixture's directory). Runner: `--include` of the test and its named fixtures for tests with `import(`.
- Tests: fixture `modules/dynamic` (+ `main.flags`), `modules/library-dynamic` (build test
  `library_with_dynamic_import`), frontend tests `dynamic_import_patterns`, `glob_segments`; `cargo test`: all pass.
  Gating: all fixtures byte-identical to the slice-2 build except the TLA fixtures and F186's.
- test262: 0 regressions, 330 → pass; `dynamic-import` 454 of 548 run (82.8%); subset 25,911 of 28,107 run (92.2%).
- Unsupported / follow-ups: F199 (node's loader ticks). `import.source()`/`import.defer()` rejected (node lacks them).
- **Next smallest step (slice 5)**: bare specifiers. Resolve `pkg` / `pkg/sub` from the importer's directory up
  through `node_modules/<pkg>/package.json` (`exports` with the `import`/`node`/`default` conditions, else `module`,
  else `main`), at build time, in `Graph::resolve` (today: "package import" rejected); then `import()` of a bare
  literal too. Consider shims for a few `node:` modules (scriptc does `events`, `path`, `process`).

## Checklist
- [x] design questions decided and recorded
- [x] slice 1: static imports
- [x] slice 2: namespaces and re-exports
- [x] slice 3: top-level `await` in dependencies, `import.meta` (and JSON modules)
- [x] slice 4: dynamic `import()`
- [ ] slice 5 (or a new task): bare specifiers
- [ ] test262 + baseline, follow-ups, docs (HISTORY.md, keywords.md, design.md, README)

## Progress
- 2026-09-28: handoff written after T40 (the user chose multi-file modules next; dynamic `import()` scoped with
  them: build-time-known modules only, see "What we know"). State at handoff: `main` at `3b4ce2a`, pushed; test262
  subset 25,018 of 27,496 run pass (91.0%).
- 2026-09-28: started. Tools unchanged (`check-tools.sh` ok). Design question 1 decided: (a) scope hoisting at the
  source level, in a new `frontend::link` (per-module parse + semantic drive renames by symbol; span edits, no
  codegen, so comments/JSDoc stay), recorded in `docs/design.md` D4. node checks: a TDZ error on an imported binding
  names the importer's local (`import d from` → "Cannot access 'd' before initialization"), so replaced references
  keep their original text for messages; anonymous `export default function` is named `default`; a missing file is
  `ERR_MODULE_NOT_FOUND` (a build error here). Evaluation (Q2): post-order DFS, one `main`/`init`; namespaces (Q3):
  slice 2; gating (Q4): an entry without module imports takes the old path unchanged; libraries (Q5): entry exports only.
- 2026-09-28: slice 1 core in place: `crates/frontend/src/link.rs` (load, resolve, ResolveExport with stars,
  names, edits, join + `SourceMap`), `Program::{map, locate, original_name, written_name}`,
  `frontend::parse_linked`, `lower::parse_entry` (CLI `build`/`ir` use it). Lowering: `unsupported`/`pos` go through
  `locate`; TDZ messages take the name written at the reference (`Var.guard` carries its offset); function/class
  names through `original_name`. Fixtures `tests/fixtures/modules/{basic,live,cycle,tdz,names}/main.js`: all 5 match
  node (`scripts/diff.sh tests/fixtures/modules/*/main.js`). Frontend unit tests: 11 pass.
- 2026-09-28: gating checked: old (`HEAD` worktree) vs new build of all 162 non-module fixtures: 161 byte-identical,
  `generators/basics.js` is F186 (its new outputs are among the old build's 6 variants). Error paths checked (missing
  export/file, bare, `node:`, parse and lowering errors in a dependency point at its file). Library entry: deps' code
  joins `init`, `export { double } from './util.js'` becomes a WIT export (`crates/js2wasi3/tests/build.rs`:
  `library_from_several_modules`, `module_errors_are_diagnostics`). Fixed: `export { K as default }` was dropped.
  Fixture `modules/defaults` (default forms, ASI at joins, a module shadowing `JSON`, one module via two paths).
  `diff.sh`: `modules/<case>/main.js` are entries. `cargo test --no-fail-fast`: all pass
  (`streams_compose_with_wac` failed once under parallel load, passes alone 3/3: F134's GC flakiness).
- 2026-09-28: test262 runner: tests with `./` imports are compiled under their own name in a per-test dir with the
  files they import symlinked (`relativeImports`/`linkImports`), resolution-phase negatives run (a build error
  passes), every test is built with the new `build --command` (a command even if the entry has exports, as node runs
  it: the linker strips the entry's exports like a dependency's). Added `language/module-code`, `language/import`,
  `language/export` to the sparse checkout and `SUBSET`. Fixes from the first runs: `export { x } from` must
  resolve (SyntaxError, InitializeEnvironment), attributes rejected before reading the file, the entry's anonymous
  default gets a binding when joined, an entry export clashing with a global another module reads is renamed and
  re-exported (`export { A$1 as A }`), writes through an import throw "Assignment to constant variable."
  (`SourceMap::imports`, `Var::constant`). Module dirs + the 1,031 earlier tests: 631 pass, 3 fail (now fixed),
  0 regressions; module-code 81.3% (namespaces 0/38: slice 2), top-level-await 94.8%.
- 2026-09-28: slice 2 (namespaces): the linker emits `const namespace$N = $$ModuleNamespace([keys], [() => b…]);`
  before the modules (keys in code-unit order, `export *` names resolved, ambiguous ones left out; nested
  namespaces too); that prefix is `SourceMap::generated` and may call the prelude (`prelude_leak`, prelude roots).
  `builtins/module.js` (`$$ModuleNamespace`, appended last to `PRELUDE`), intrinsic `module_new` (`$module_new`,
  `CLASS_MODULE` = 12, null prototype; the lowering adds name `$module`). Runtime hooks, all gated on `$module`:
  `object_desc` "[object Module]" (delete message), getter-only write → node's read-only message, descriptors
  are `{ value, writable: true, enumerable: true, configurable: false }`, `isFrozen` false, builtin tag "Module",
  `console.log` `[Module: null prototype] { … }` (values through the getters; empty `{  }` as node prints).
  Fixture `modules/namespace` passes; all 7 module fixtures pass. Gating re-checked: 160 of 162 byte-identical,
  2 are F186 with only old variants (a `local.tee` I had turned into set+get was the first diff).
  Not done: `Object.defineProperty(ns, k, { value: same })` succeeds in node (spec [[DefineOwnProperty]]); here
  "Cannot redefine property"; printing an export still in its TDZ (node: `<uninitialized>`) throws.
- 2026-09-28: checkpoint after slices 1–2: targeted test262 `--update` (module dirs + 1,032 import/export tests,
  1,981 tests): 0 regressions; subset 25,556 of 28,118 run (90.9%). Docs: HISTORY.md, keywords.md (`import`,
  `export`, `default`), design.md D4, README, follow-ups F192–F197, F7 ticked. Notes / Result written for slices 1–2.
- 2026-09-28: slice 3 started. node probe: `a.js` awaits, sibling `b.js` runs meanwhile, `c.js` (imports a) and the entry run one job after a's body settles (spec async module evaluation, exact ticks visible against a promise chain); js2wasi3 today runs a, b, c in sequence. Design: when a command's dependency has top-level `await`, the linker records each module's joined range, TLA flag and requests (`SourceMap::modules`) and writes `await $$EvaluateModules(__intrinsic_module_bodies(), tla, requests)` in the generated prefix; the lowering makes each module's top-level statements its own function (globals shared, main-like TDZ), and `builtins/module.js` runs the spec's InnerModuleEvaluation / ExecuteAsyncModule / AsyncModuleExecutionFulfilled/Rejected over them. Other programs: unchanged.
- 2026-09-28: `$$EvaluateModules` (prelude), `SourceMap::modules`, per-module functions (`Lowerer::module_bodies`, `lower_top`, `is_top_code`), intrinsic `module_bodies` (allowed in generated code). Probe matches node exactly; fixtures `modules/{tla,tla-error,tla-cycle}` pass. Gating: 173 fixture builds old vs new, all byte-identical except the 3 new ones and the 2 F186 fixtures (new output among old variants).
- 2026-09-28: test262's TLA ordering tests need `globalThis` or `import()`; checked by hand with a module standing in for `globalThis`: `async-module-does-not-block-sibling-modules`, `dfs-invariant`, `pending-async-dep-from-cycle` match node. F192 done.
- 2026-09-28: `import.meta` (decided: node's properties with build-time values, so output matches node where it was built): the linker replaces each `import.meta` by a generated `const meta$k = $$ImportMeta(url, filename, dirname, main)` (`Binding::Meta`; a one-file program using it now goes through the linker); `url` like `pathToFileURL` of the canonical path; `resolve` in the prelude (relative/absolute paths against the URL, URLs as they are, bare → ERR_MODULE_NOT_FOUND). Fixture `modules/meta` passes (first version pulled the regex engine: 2.6 s; char tests instead).
- 2026-09-28: JSON modules: the linker checks import attributes (only `type: "json"`; others, a missing one on `.json`, one on a non-JSON file: build errors worded after node's ERR_IMPORT_ATTRIBUTE_*), validates the file with a small JSON checker (`json_module`: errors at the JSON file's line/col; BOM stripped) and loads `export default <text>;` with `"__proto__"` keys made computed (own properties, as JSON.parse). Fixture `modules/json` passes (default, namespace, `export { default as x } from … with`, one object shared, mutation). Found: `console.log` prints an own `__proto__` key bare, node `['__proto__']` (F198; changing inspect would touch every program's bytes).
- 2026-09-28: frontend tests `async_dependency_makes_module_functions`, `import_meta_is_a_generated_binding`, `json_modules`; build test: library with a JSON module + `import.meta` (`modules/library/config.json`, `describe()`), a library with a TLA dependency is rejected at the dependency; the old `import.meta` rejection case now uses `eval`. test262 (import, module-code, export, import.meta): 36 changed, 0 regressions; runner skips `import-text`/`import-bytes` (node 24 lacks them); `import.meta/syntax/goal-script.js` unsupported → fail (expects a script-goal SyntaxError; every file is a module here, as node's ESM detection also runs it).
- 2026-09-28: targeted test262 `--update` (module dirs + 402 tests with imports/exports/`import.meta`/attributes, 1,351 tests): 37 changed, 0 regressions (2 load timeouts pass alone); subset 25,581 of 28,108 run (91.0%), module-code 90.9%. Gating re-checked with the final code: 163 fixtures built by the old compiler, all byte-identical except the 3 TLA fixtures (now split into module functions) and the 3 F186 fixtures (new output among old variants).
- 2026-09-28: slice 4 started (dynamic `import()`).
- 2026-09-28: slice 4 core: `frontend::Host` (read + list; `Fs`), `frontend::Options { command, include }`, CLI `build --include <glob>`. Linker: `import()` targets from specifier patterns (literals, `const`s, `?:`/`||`/`??`, template and `+` with `*` inside one segment; `.js`/`.mjs`/`.json` files), `--include` globs (`**`), a table of [absolute path, module, JSON, namespace]; missing files are left out (their `import()` rejects, as node); an unresolvable `import()` without `--include` is a build error. Dynamic programs always use module functions; `import(x, o)` → `$$DynamicImport(k, x, o)` (inserted text may call the prelude: `SourceMap::is_inserted`). Prelude: `$$EvaluateModules` generalized (spec Evaluate with per-module capabilities, entry index, `sync` for a library `init`), `$$moduleImport` (EvaluateImportCall's checks, node's ERR_MODULE_NOT_FOUND / ERR_IMPORT_ATTRIBUTE_* errors, load one job later). Bugs found: evaluating inside `import()` synchronously ran the module before the importer's next statement and a nested Evaluate reset the outer DFS stack (hang): the load now waits a job. node's loader takes 4–7 extra jobs (differs for new vs. loaded modules): not matched (follow-up). Fixture `modules/dynamic` (+ `main.flags`; `diff.sh` reads `x.flags`, builds from the fixture's dir) passes.
- 2026-09-29: two-phase loading (modules only `import()` reaches load after the static graph; their errors reject at run time: `lazy`/`failed`/`poisoned`); runner passes `--include` for tests with `import(`; test262 dynamic-import 454/548; targeted `--update` (1,984 tests): 0 regressions, 330 → pass, subset 92.2%. Docs, follow-up F199, Notes / Result for slice 4.
