# T41: Multi-file ES modules (F7, F79)

Status: todo (handoff written 2026-09-28, not started) | Depends on: T20 (user worlds: WIT imports), T21 (prelude)

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
- [ ] One-file programs compile exactly as before (same bytes for the existing fixtures, modulo F186).
- [ ] Multi-file fixtures match node: named/default/namespace imports, re-exports, live bindings, cycles, TDZ,
      evaluation order, top-level `await` in a dependency, errors (missing file/export) at build time.
- [ ] `import()` of build-time-known modules (literal, constant, conditional, pattern, `--include`) works; an
      unresolvable one is a build error; unknown at run time → a rejected promise like node's.
- [ ] test262: the runner resolves fixtures; the affected tests' results are in the baseline; 0 regressions.

## Notes / Result

## Checklist
- [ ] design questions decided and recorded
- [ ] slice 1: static imports
- [ ] slice 2: namespaces and re-exports
- [ ] slice 3: top-level `await` in dependencies, `import.meta`
- [ ] slice 4: dynamic `import()`
- [ ] slice 5 (or a new task): bare specifiers
- [ ] test262 + baseline, follow-ups, docs (HISTORY.md, keywords.md, design.md, README)

## Progress
- 2026-09-28: handoff written after T40 (the user chose multi-file modules next; dynamic `import()` scoped with
  them: build-time-known modules only, see "What we know"). State at handoff: `main` at `3b4ce2a`, pushed; test262
  subset 25,018 of 27,496 run pass (91.0%).
