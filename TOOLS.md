# Pinned tools

Checked 2026-09-25 against the latest GitHub releases (node: 2026-09-26, latest LTS; wasmtime: 2026-10-16). Rule: always the latest
version of every tool. Verify with `scripts/check-tools.sh`.

| Tool | Version | Used for |
|---|---|---|
| wasmtime | 50.0.0 | the only test host (WASI 0.3 via `-Sp3`; 50.0.0 since 2026-10-21 (released 2026-10-20; T146; 49.0.3 kept as `wasmtime-49.0.3`); 49.0.3 from 2026-10-16: security/panic fixes in Component Model async, streams, exception tags, recursive types (T122); 49.0.2 since 2026-10-03: GC rooting across `try_call`, F224). 50.0.0-rc.1 was measured in T123 (pidigits −26.4%, splay −9.1%, binarytrees −7.2%, test262 no changes). 49.0.4 (2026-10-19: wasi-http content-length/trailer checks, aarch64 `br_table`, a stream-type panic, rustls) skipped: user, 2026-10-20, wait for 50 |
| wasm-tools | 1.259.0 | validate, print, `component embed/new/wit` |
| wasm crates | 0.259.0 | `wasm-encoder`, `wasmparser`, `wit-parser`, `wit-component` (same code as wasm-tools 1.259) |
| oxc crates | 0.151.0 | JS/TS parser and semantic analysis |
| wkg | 0.16.1 | fetch WASI WIT packages (`wkg wit fetch`) |
| WASI | 0.3.1 | `wasi:cli`, `wasi:clocks`, ... `@0.3.1` |
| wit-bindgen | 0.62.0 | reference only (naming/ABI questions) |
| wac | 0.11.0 | composing components in tests (later) |
| binaryen (wasm-opt) | 133 | optional optimisation experiments only |
| rustc/cargo | 1.98.1 | pinned in `rust-toolchain.toml`; targets `wasm32-unknown-unknown` (the crypto helper, T149; `wasm32-wasip2` is also installed but its std imports WASI 0.2) |
| zlib helper crates | wit-bindgen 0.62.0, libz-rs-sys 0.6.8 (zlib-rs) | `crates/zlib-helper` (T150), `scripts/build-zlib-helper.sh`; zstd (C 1.5.7 via clang) planned |
| brotli helper crates | wit-bindgen 0.62.0, brotli 9.0.0 (brotli-decompressor 6.0.1) | `crates/brotli-helper` (T150 slice 2), `scripts/build-brotli-helper.sh` |
| wac-graph / wac-types | 0.12.0 | the compiler composes the helpers into the program (T150, `crates/js2wasi3/src/compose.rs`; they use wasm crates 0.258) |
| crypto helper crates | wit-bindgen 0.62.0, pbkdf2 0.13.0, scrypt 0.12.0, hkdf 0.13.0, hmac 0.13.0, md-5 0.11.0, sha1 0.11.0, sha2 0.11.1 | `crates/crypto-helper` (T149), versions in its `Cargo.lock`; `scripts/build-crypto-helper.sh` |
| node | 24.21.0 | differential oracle for JS behaviour (latest LTS; was 20.19.2 until 2026-09-26) |
| cldr-json | 48.2.0 | `Intl` locale data (`scripts/gen-intl.mjs`, T77): node 24.21.0's ICU 78.3 carries CLDR 48.2 (48.0.0 differs, e.g. en `Hv`); zone data is probed from node itself (tzdata 2026c) |
| test262 | `7ab7fafa0003f73fc85c1b95d88094d33f7eb8bd` | conformance subset (`scripts/fetch-test262.sh`, `scripts/test262.mjs`; `main` on 2026-09-27) |

## Wasm feature status on wasmtime 50.0.0 (verified by `probes/run.sh`; same output as on 49.0.3, 49.0.2 and 49.0.1)

| Feature | Default? | Notes |
|---|---|---|
| GC (structs, arrays, i31, casts) | on | |
| Exceptions (`try_table`, `exnref`) | on | works inside async callbacks |
| Component Model async (callback lift, streams, futures) | on | WASI 0.3 host needs `-Sp3` |
| Stackful async lift (🚟) | off | `-W component-model-async-stackful=y` |
| Component Model threads (🧵) | off | `-W component-model-threading=y`; ~0.75 µs per switch |
| Stack switching (`cont.new`/`resume`/`suspend`) | off | `-W stack-switching=y`; ~25× or more faster than 🧵 |
| GC Canonical ABI option (🛸) | off | scalars only; strings/lists **not implemented** |
| Timers (`monotonic-clock` `[async-lower]wait-until`, `[subtask-cancel]`) | on | probe 8: concurrent waits overlap; a subtask must leave its waitable set (`waitable.join h 0`) before `subtask.cancel`, else trap |

## Other runtimes (not test hosts; for future testing)

wasmtime is the only test host because no other standalone runtime with WasmGC and WASI P3 was known. Candidates
to recheck when they release:

| runtime | version tried | status with js2wasi3 output |
|---|---|---|
| node 24.21.0's V8 through [jco](https://github.com/bytecodealliance/jco) 1.37.0 + preview3-shim 0.9.0 (in `~/.local/jco`; on a-03 too since 2026-10-18, same versions) | 2026-10-13, T116 | **runs all 18 benchmarks** (outputs = node) with `jco transpile --bindgen-enable-wasm-exnref` and `--map wasi:<pkg>/*@0.3.1=@bytecodealliance/preview3-shim/<pkg>#*` (jco knows WASI 0.3.0 only); runner `scripts/v8run.mjs`, comparison `scripts/v8bench.sh`. A measurement engine, not a test host |
| [zwasm](https://github.com/zwasm/zwasm) (Zig, Apache-2.0; interpreter, single-pass JIT, AOT `.cwasm`) | v2.7.0 (2026-09-14), release binary in `~/tools/zwasm-2.7.0/`; probed 2026-10-04 | **can't run it yet**, see below |

zwasm v2.7.0, what stops it now:
1. **Validation bug**: the typed `select` with a concrete reference result type (`select (result (ref $t))` or
   `(ref null $t)`) is rejected: `BadValType at op 0x1c`. Abstract types (`eqref`, `anyref`, `i31ref`,
   `funcref`) pass. Valid Wasm 3.0 (wasm-tools and wasmtime accept it); every js2wasi3 module has such selects
   (`hello.js`: 21). Not reported in zwasm's issues and not fixed by 2026-10-04. Repro:
   `(module (type $s (array (mut i16))) (func (export "run") (param $c i32) (result i32)
   (drop (select (result (ref null $s)) (ref.null none) (ref.null none) (local.get $c))) (i32.const 1)))`,
   `zwasm run --invoke run=1 x.wasm`.
2. **WASI P3 async**: a P3 component with our 0.3.1 imports starts and writes to stdout (`probes/cmd.wat`), but
   the async-lowered `monotonic-clock.wait-for` ends the program (exit 1, no message). js2wasi3 uses async lowers
   for timers, streams and sockets. (zwasm implements WASI 0.3.0; ours is 0.3.1.)
3. Not a blocker, measured: WasmGC allocation (a loop of 50M small structs) takes 2.38 s vs wasmtime's 0.21 s.
   What works: GC structs/arrays, exceptions (`try_table`/`throw`), a P3 component's start and stdout.

Recheck: `zwasm run` on a js2wasi3 build of `tests/fixtures/hello.js` (1) and on `probes/cmd.wat`'s component (2).

## Syntax gotchas (wasm-tools 1.259)

- `(canon lift ... async (callback (core func $i "cb")))`: the `core` keyword is required.
- `(canon thread.new-indirect $ft (core table $i "t"))`.
- Async core names from wit-component: `[async-lower]f`, `[async-lift]iface#f`,
  `[callback][async-lift]iface#f`, `[task-return]f`, `[stream-new-0]f`,
  `[async-lower][stream-write-0]f`, `[stream-drop-writable-0]f`, `$root` / `[waitable-set-new]` etc.
  Print them with `wasm-tools component embed wit --dummy-names legacy --async-callback -t`.

When a tool is upgraded, rerun `probes/run.sh` and update this file first.
