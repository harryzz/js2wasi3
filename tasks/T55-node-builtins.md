# T55: node's built-in modules, first slice (`process`, `path`, `events`, `util`, `os`)

Status: todo (handoff written 2026-09-30, not started) | Depends on: T41 (linker, packages), T54 (CommonJS,
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
- [ ] `path`, `events`, `util`, `os`, `process` (module and global) behave like node for the covered API (fixtures),
      through `require`, `import`, `node:` and `import()`.
- [ ] `semver` and `lru-cache@10` build and match node.
- [ ] Unsupported built-ins/members are clear errors (build or run time per T54's rule) with follow-ups.
- [ ] Programs without built-ins compile to the same bytes.

## Notes / Result
(to be written)

## Checklist
- [ ] design questions decided and recorded
- [ ] virtual-module mechanism + `path`
- [ ] `process`, `performance`
- [ ] `events`, `util`, `os` (`assert`)
- [ ] fixtures, gating check, real-package probe
- [ ] docs (design.md, keywords.md, README), follow-ups

## Progress
- 2026-09-30: handoff written after T54. State: `main` at `453b78b`, pushed; test262 subset 27,824 of 28,799 run
  pass (96.6%).
