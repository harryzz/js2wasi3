# T65: node built-ins that only need to load, and a `crypto` subset (F267)

Status: not started | Depends on: T64

## Goal
The 37 link errors left in zigbee2mqtt's graph (T62) are node built-ins js2wasi3 lacks. Most are required at load
but used only on paths the milestone doesn't take; each should match node on its own fixture where it runs:
1. Modules that load, with node's shape (exports, classes, constants) and functions that fail like node does
   without the facility: `child_process` (`exec` reports `ENOENT` through its callback, as with no `git`), `dns`,
   `dgram`, `tls`, `http`, `https`, `http2`, `zlib`, `perf_hooks`, `module`.
2. A `crypto` subset zigbee-herdsman and zigbee2mqtt use: `randomBytes`, `randomUUID`, `getRandomValues` (and the
   `crypto` global's), `createHash` (`md5`, `sha1`, `sha256`), AES `createCipheriv`/`createDecipheriv` (Green Power
   frames, zspec).
Programs that don't name them keep their bytes.

## What we know (at handoff)
- T62/T64 counts (docs/zigbee2mqtt-feasibility.md "Survey 2026-10-01"): `crypto` 9, `http`/`https`/`http2` 8,
  `net` 7 (T66's), `tls` 4, `dgram` 3, `zlib` 3, `child_process` 2, `dns` 1, `perf_hooks` 1 (mqtt, ws, srvx,
  winston(-syslog), socks, multicast-dns, zigbee-herdsman, zigbee-on-host, @serialport/bindings-cpp, zigbee2mqtt).
  T62's `tasks/T62-files/` stubs show the shapes each package touches at load (`stubs/*.js`).
- Built-ins are CommonJS modules in `crates/frontend/node/` listed in `crates/frontend/src/node.rs` `MODULES`
  (T55); a module can import WASI interfaces directly (`import { f } from "wasi:…"`, T59's `fs` over
  `wasi:filesystem`, ES module `wasi_fs.mjs`), so `crypto.randomBytes` can use `wasi:random/random`
  `get-random-bytes`. Hashes and AES are plain JS over typed arrays (keep them small; node's `crypto` errors).
- node's own lib for `child_process`, `http`, … depends on C++ bindings throughout: write minimal modules with
  node's exports rather than porting (T64's `url` port worked because it is mostly JS).

## Plan
1. Load-only modules, one fixture per family (`node/child-process.js`, `node/http.js`, …): exports, `typeof`s, the
   failure path (`exec("git …", cb)` → `ENOENT`).
2. `crypto`: random first (WASI), then `createHash`, then AES; fixture `node/crypto.js` against node.
3. Probe: `T63_GLOBALS=0 tasks/T63-files/build-run.sh` with `patch.sh`'s built-in stubs removed for what this task
   provides; record which stubs remain.

## Read first
- `tasks/T64-url-text-dataview.md` (Notes / Result), `tasks/T59-fs.md` (WASI imports from a built-in module),
  `tasks/T55-node-builtins.md`, `docs/zigbee2mqtt-feasibility.md` (Survey 2026-10-01).

## Tests
Each fixture through `scripts/diff.sh`; `cargo test`.

## Acceptance
- [ ] Items 1–2 equal node on their fixtures; programs without them keep their bytes.
- [ ] zigbee2mqtt builds without the T62 built-in stubs this task replaces.
- [ ] Follow-ups updated.

## Notes / Result
(to be written)

## Checklist
- [ ] load-only modules
- [ ] `crypto` subset
- [ ] zigbee2mqtt probe, docs, follow-ups

## Progress
- 2026-10-02: handoff written after T64.
