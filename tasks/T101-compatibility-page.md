# T101: a Node.js compatibility page (F380)

Status: done (2026-10-10) | Depends on: T100 | Local

## Goal
Published at **https://harryzz.github.io/js2wasi3** (GitHub Pages; user, 2026-10-10). One generated page that says, per Node.js 24 API, whether js2wasi3 supports it, and how we know, in the spirit of
https://scriptc.dev/compatibility (not a copy). Plus two short tabs the user asked for: the language (test262)
and benchmarks against node. Generated from data in the repo, re-runnable after every task; **doesn't touch the
compiler**.

## What scriptc.dev does (seen 2026-10-09, F380)
3,662 API rows in 55 chapters from Node's `doc/api/all.json` at a pinned commit; per row a status (supported /
partial / not implemented / refused / by design / unreviewed / N/A), a verification basis (test-backed, explicit
refusal, verified absence, declared gap, registry gap, policy, derived) and Node's stability index; chapter
roll-ups, search and filters; one JSON dataset.

## What we have (checked 2026-10-10)
- Node modules: `crates/frontend/src/node.rs` `MODULES` (62 entries: implemented modules and aliases such as
  `path/posix`, `fs/promises`, `util/types`), sources in `crates/frontend/node/*.js`. A module not in the list is
  a **build-time error**: `unsupported: node's built-in module `worker_threads` (planned: F238)`, so a probe
  program must cover one module at a time.
- Refusals: `crates/frontend/node/unavailable.js` (`fn`/`fns`): functions that exist and throw
  "`<module>.<name>()` isn't available in js2wasi3 (<why>)"; used in assert, crypto, dns, http, http2, https, tls,
  module, net, perf_hooks, zlib, util. The `why` text is the row's note.
- Globals: `crates/frontend/src/node.rs` (`("Buffer", "node:buffer", "Buffer")` etc.) and the prelude.
- Tests: `tests/fixtures/node/*.js` (~50, each checked against node by `scripts/diff.sh`), plus other fixture
  directories.
- Language: `tests/test262/baseline.json` (per test: pass/fail/…) and `HISTORY.md` (latest full run: 28,105 of
  28,984 run pass, 97.0%, T98); `docs/keywords.md` (reserved words, probed).
- Benchmarks: programs in `tests/bench/` (`README.md`), runner `scripts/bench.sh`; results only in task files
  (T78's table, then T90–T100 per benchmark). `scripts/bench.sh` names `docs/benchmarks.md`, which doesn't exist.

## Read first
- `crates/frontend/src/node.rs` (module and global lists), `crates/frontend/node/unavailable.js`.
- `scripts/diff.sh` (how fixtures run), `scripts/bench.sh` (output format, `results.tsv`).
- `tests/test262/HISTORY.md` (top entry), `docs/keywords.md` (format of an existing status table).

## Plan
1. **Inventory**: Node v24.21.0's `all.json` (`https://nodejs.org/docs/v24.21.0/api/all.json`; downloaded data,
   so its own new directory, read as JSON only), pinned: keep only the derived inventory in the repo
   (`docs/compat/inventory.json`: module, kind (method/class/property/event/global), name, stability), not the
   download. Write the URL and the date in the generator.
2. **Presence probe** (verified absence/presence): a generator `scripts/compat.mjs` that writes one program per
   module (`require` it, then for each API name `typeof`, and for classes the prototype's members) and runs it
   under node and under js2wasi3 + wasmtime (release build, `wasmtime run -Sp3`); a build error for the module =
   "not implemented (module)"; an API present in node and missing here = "not implemented"; a stub from
   `unavailable.js` (found statically in the module sources: the names passed to `fn(...)`/`fns(...)`, not by
   calling them) = "refused" with the why; present = "present". Globals the same way in one program.
3. **Test-backed**: an API name used in a passing fixture (`tests/fixtures/node/*.js` and others; a plain text match
   on `module.name` / `.name(` per module is enough) upgrades "present" to "supported (test-backed)"; present but
   untested stays "present (untested)". A short hand-kept list `docs/compat/partial.json` (API → note) for known
   partial ones (e.g. what's in README "What works" with gaps, `Buffer` encodings, `fs` options). Don't hand-review
   all rows.
4. **Output**: `docs/compatibility.md` (generated, GitHub renders it): summary per module (supported / present /
   partial / refused / missing counts), then per module a table (API, kind, status, basis, Node stability, note), and
   `docs/compat/compatibility.json` (the dataset). Two short sections at the end: **Language** (test262 totals and
   the per-directory pass% from `baseline.json`, top-level directories only; link `docs/keywords.md`), and
   **Benchmarks** (one table: program, node, js2wasi3, ratio, date, from the latest measured numbers in the task
   files; or, if the user agrees, one `scripts/bench.sh -r 3` run of all programs: ask first, it takes a while).
   This also creates the `docs/benchmarks.md` that `bench.sh` names, or fix that reference to point at the page.
5. **Site for GitHub Pages** (https://harryzz.github.io/js2wasi3): the generator also writes a static site to
   `site/` (committed): `index.html` (the summary, then per module a collapsible table; a search box and status
   filters in plain inline JS, no framework, no external requests), `compatibility.json` (the dataset, fetched by
   the page), and the Language and Benchmarks sections as their own short pages or tabs. Light/dark via
   `prefers-color-scheme`, readable on a phone. Plain HTML/CSS/JS so Pages serves it as is (`.nojekyll`).
   Deploy with a workflow `.github/workflows/pages.yml` (`actions/upload-pages-artifact` + `actions/deploy-pages`
   on pushes to `main` that touch `site/`), or Pages "deploy from a branch" with `/site` not possible (only `/` or
   `/docs`): so the workflow, or the generator writes into `docs/` instead; pick one and write it in Progress.
   **Before enabling** (outward actions, ask the user each time):
   - the repo is **private** (checked 2026-10-10, `gh api repos/harryzz/js2wasi3`): Pages on a private repo needs a
     paid plan (Pro or above), and the site is then **public** even though the repo isn't (no private Pages outside
     Enterprise). Ask: is the account on a plan with Pages for private repos, or should the repo (or a separate
     public repo for the site only) be used? Don't change the repo's visibility without an explicit yes.
   - enabling Pages (`gh api -X POST repos/harryzz/js2wasi3/pages -f build_type=workflow`, or the repo settings)
     and pushing the workflow: ask first; then check the URL loads.
   - the page lists only what's in the repo's own files (no secrets, no paths from the user's machines, no a-03 or
     test-rig addresses): check the generated files before the first deploy.
6. Link the page (the Pages URL and `docs/compatibility.md`) from README ("What works") and `docs/design.md`
   (Testing).
7. Tests: the generator runs from a clean checkout in a few minutes; spot-check 20 rows by hand against node
   (5 supported, 5 missing, 5 refused, 5 partial) and write them in Progress.

## Acceptance
- [x] `scripts/compat.mjs` regenerates `docs/compatibility.md` + the JSON from the repo (and the pinned inventory).
- [x] Every row has a status and a basis; spot-check of 20 rows in Progress.
- [x] Language and Benchmarks sections; README links the page; F380 ticked; follow-ups for anything left.
- [x] `site/` generated; after the user's yes, Pages enabled and https://harryzz.github.io/js2wasi3 serves it.

## Notes / Result
**What it is**: `scripts/compat.mjs` (no compiler changes) generates `docs/compatibility.md`,
`docs/compat/compatibility.json` and `site/` (`index.html` from `scripts/compat-page.html` + the JSON), published
by `.github/workflows/pages.yml` at https://harryzz.github.io/js2wasi3/ (the user made the repo public for it).
Inputs in the repo: `docs/compat/inventory.json` (3,044 rows derived from node v24.21.0's `all.json`; the download
isn't kept), `docs/compat/probe.json` (the last probe), `docs/compat/partial.json` (hand-kept gaps),
`docs/compat/benchmarks.json` (an imported `scripts/bench.sh` run), `tests/test262/baseline.json`.

**How each status is known**: one probe program per runtime (node: every module; js2wasi3: the shipped ones from
`node.rs`) tests every candidate path with `in`; names js2wasi3 rejects at build time are dropped and reported;
stubs that throw are found in `crates/frontend/node/*.js` (`fn`/`fns`, throwing constructors, fs's `unsupported`,
dgram's ENOSYS); "supported" = a passing fixture (not under `node_modules`) that loads the module (or names the
global) names the API. Presence isn't conformance (the page says so). Totals: supported 638, present 306, partial
16, refused 162, missing 570, no module 298, not probed 1,051 (events 254, instance-only properties, unreachable
names), n/a 3. Spot check of 20 rows under both runtimes: all as the page says (Progress).

**Benchmarks** (a-03, `scripts/bench.sh -r 3`, all 18 programs, outputs = node): from 2.9× node (pidigits) to
54× (Octane RegExp); Octane mostly 16–43×, knucleotide 4.4×.

**Commands**: `node scripts/compat.mjs inventory <all.json>` (new node version), `node scripts/compat.mjs` (probe +
render, ~1 min, needs `target/release/js2wasi3`), `render` (from the saved probe), `bench <results.tsv> [machine]`.

**Not done / follow-ups**: events and instance-only properties aren't probed (1,051 rows); test backing is a name
match, not per-behaviour; F387 (`"URL" in globalThis` false), F388 (dgram `bind({ fd })` message). Next smallest
step: re-run `node scripts/compat.mjs` at the end of tasks that add node APIs (and `bench` after benchmark runs).

## Progress
- 2026-10-10: handoff written (user: F380 next, in a new session).
- 2026-10-10: started. User made the repo public so the page can go on GitHub Pages (`gh api`: visibility public).
- 2026-10-10: `scripts/compat.mjs inventory` (from all.json v24.21.0, downloaded into the scratchpad only):
  `docs/compat/inventory.json`, 3,044 rows in 50 chapters (methods 1,738, properties 745, classes 273, events 254,
  static methods 28, constructors 5, the `process` global). `probe`: one program for node (every module) and one
  for js2wasi3 (the 25 shipped modules from `node.rs` `MODULES`; others are a build error), 5,001 candidate paths
  tested with `in`, ~1 min: node has 2,085, js2wasi3 932. Results in `docs/compat/probe.json`.
- 2026-10-10: probe fixes found by checking rows by hand: (1) js2wasi3 links only the built-ins a program names, so
  the probe also lists every path as a member expression in a never-run branch; (2) node globals aren't properties
  of `globalThis` in js2wasi3 (`"URL" in globalThis` false, `typeof globalThis.URL` "function": F387), so global
  roots go through `typeof X` getters; (3) names js2wasi3 rejects at build time even in dead code
  (`console.profile`, `profileEnd`, `timeStamp`: "unsupported") are dropped and reported as build errors; (4) rows
  under the `process` global lack the prefix (`stdout`): an `owner`. Refusals found statically in
  `crates/frontend/node/*.js`: `fn`/`fns` (literal or named lists, zlib's loops), classes whose constructor throws
  `unavailableError` (direct, via a base class, built in a loop), fs's `unsupported(…)`, dgram's ENOSYS methods.
  Fixtures under `node_modules` don't count as test backing. Summary now: supported 643, present 296, partial 16,
  refused 162, missing 553, no module 317, not probed 1,054 (events 254, instance-only properties, unreachable).
- 2026-10-10: renderer: `docs/compatibility.md` (summary, by module, Language from `baseline.json`, Benchmarks
  from `docs/compat/benchmarks.json` once imported, APIs per module in `<details>`), `docs/compat/compatibility.json`,
  `site/` (`index.html` from `scripts/compat-page.html`: search, status filters, tabs; loads `compatibility.json`;
  `.nojekyll`). Fixes: paths js2wasi3 can reach preferred (Console rows via the `console` global, not
  `node:console`); class members named after the instance (`console.log` under `Class: Console`) probe that path
  too; a global's row needs a fixture that names the global (`Array.from` no longer backs `Buffer.from`).
  Spot check, 20 rows, run under node and js2wasi3 (`spot.js`, scratchpad): supported fs.readFileSync,
  Buffer.from, path.join, util.format, EventEmitter#once = node; missing process.umask, fs.promises.cp,
  crypto.hkdfSync, WritableStream, tty.WriteStream#hasColors: undefined here; refused zlib.gzipSync, dns.resolve4,
  http.request, tls.connect, fs.watch: throw "… isn't available/supported"; partial child_process.spawnSync (ENOENT),
  createHash (sha256 ok, sha3-256 not), TextDecoder (shift_jis not), module.createRequire, dgram bind({fd}): all as
  the page says. Found: dgram's bind({fd}) error message is "[object Object] (see error.payload)" (F388).
  Totals: supported 638, present 306, partial 16, refused 162, missing 570, no module 298, not probed 1,051, n/a 3.
- 2026-10-10: user: benchmarks on a-03, then publish. `bench <results.tsv> [machine]` records the machine; a-03 run started (`~/t101-bench.sh`, log `~/t101-bench.log`, `scripts/bench.sh -r 3 -o ~/t101-bench`, all programs). Pages workflow `.github/workflows/pages.yml` (checkout v7, configure-pages v6, upload-pages-artifact v5, deploy-pages v5) deploys `site/`. Links: README, design.md (Testing). `bench.sh` no longer names the missing docs/benchmarks.md. F387, F388 added.
- 2026-10-10: a-03 benchmarks done (7 min, all SAME): imported (machine shown by its hardware, not its host name), re-rendered. Docs, F380 ticked. Publishing (user: yes).
