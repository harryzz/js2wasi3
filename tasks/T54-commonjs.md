# T54: CommonJS modules (`require`, `module.exports`)

Status: todo (handoff written 2026-09-30, not started) | Depends on: T41 (modules, linker, packages, `import()`
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
- [ ] node's CommonJS detection, `require`/`module.exports`/`exports`, JSON, cycles, packages with `require`
      conditions behave like node (fixtures).
- [ ] ESM ⇄ CommonJS interop and `tsc`-style CommonJS output work (fixtures).
- [ ] Computed `require` works through patterns/`--include`.
- [ ] Sloppy-mode decision recorded; what isn't supported is a clear build error with a follow-up.
- [ ] Programs without CommonJS compile to the same bytes; 0 test262 regressions in the module directories.

## Notes / Result
(to be written)

## Checklist
- [ ] design questions decided and recorded (after the measurement)
- [ ] detection
- [ ] wrapper, registry, static `require`, JSON, cycles
- [ ] packages (`require` conditions), interop
- [ ] computed `require`
- [ ] fixtures, gating check, real-package probe, targeted test262 run
- [ ] docs (design.md, keywords.md, README, HISTORY.md if run), follow-ups

## Progress
- 2026-09-30: handoff written after T53. State: `main` at `94cfd65`, pushed; test262 subset 27,824 of 28,799 run
  pass (96.6%).
