# js2wasi3

A compiler from **JavaScript to WebAssembly components for WASI 0.3**, written in Rust.

js2wasi3 compiles JavaScript ahead of time straight to Wasm: there is no JS engine or interpreter inside the output.
It targets the Wasm features that make this practical today:

- **WasmGC** for the JS heap: objects, closures, arrays, strings and promises are GC structs and arrays.
- **Native Wasm exception handling** for `throw` / `try` / `catch` / `finally`.
- **The Component Model's async ABI (WASI 0.3, "P3")**: a program without exports is a `wasi:cli/run` command,
  whose event loop runs on the async callback ABI. A program with exports is a component library with a
  generated WIT world.

Generators and `async`/`await` are compiled to resumable state machines by an IR pass, so they need no
stack-switching proposal.

> **Status: early and experimental.** A good part of the language works (see below and
> [docs/keywords.md](docs/keywords.md)). Much of the built-in library (most of `Intl`, `Proxy`…; `Math`, strings, arrays, `JSON`, `Map`/`Set`, `Object` methods, `Reflect`, `RegExp`, `Date`, BigInt and number parsing/formatting, `Intl.DateTimeFormat` in en-US are done)
> doesn't exist yet. Unsupported code is rejected at build time with a clear diagnostic rather than
> miscompiled. The known exception, built-in methods that crash when called, is listed in the docs.

## Quick start

Requirements: Rust (pinned in `rust-toolchain.toml`), and [wasmtime](https://wasmtime.dev) 50.0.0 to run the output.
The other tools and versions are in [TOOLS.md](TOOLS.md).

```sh
cargo build --release

# A command: a file without exports.
echo 'console.log("hello from js2wasi3");' > hello.js
target/release/js2wasi3 build hello.js -o hello.wasm
wasmtime run -Sp3 hello.wasm          # -Sp3: enable WASI 0.3
# hello from js2wasi3
# GC heap: wasmtime 49's copying GC grows its heap only once the live data fills a whole semi-space (F304), so on
# its small default heap it collects constantly. Every program therefore reserves 16 MiB of GC heap at start
# (`build --gc-reserve=<MiB>`, 0 = off; costs ~2x that in memory and ~8 ms; T98: Octane RayTrace +73%,
# EarleyBoyer +28%). Large or long-running programs: a bigger initial heap (in bytes) on top; zigbee2mqtt: 512 MiB
# used 3.7x less CPU than 256 MiB for +330 MB RSS (T79). The same -O must be passed to `wasmtime compile` for a
# .cwasm.
wasmtime run -O gc-heap-initial-size=536870912 -Sp3 hello.wasm

# A library: exported functions become the component's WIT exports.
echo 'export function add(a, b) { return a + b; }' > add.js
target/release/js2wasi3 build add.js -o add.wasm
wasmtime run --invoke 'add(1, 2)' add.wasm
# 3
wasm-tools component wit add.wasm
# world root { export add: func(a: f64, b: f64) -> f64; }

# Or implement your own world: its types decide the boundary (scalar types so far).
printf 'package acme:math;\nworld calc { export add: func(a: u32, b: u32) -> u32; }\n' > calc.wit
target/release/js2wasi3 build add.js --wit calc.wit -o calc.wasm
wasmtime run --invoke 'add(4000000000, 1000000000)' calc.wasm
# 705032704
```

Other subcommands: `js2wasi3 parse <file>` (check a module) and `js2wasi3 ir <file>` (print the IR).

## What works

- Numbers, BigInts, strings (UTF-16), booleans, `null`/`undefined`, symbols; operators, including `**`, `&&=`/`||=`/`??=`,
  template literals, tagged templates (``tag`…` ``, `String.raw`).
- `let`/`const`/`var` with TDZ checks, closures, arrow functions, `this`, default/rest parameters, destructuring,
  spread.
- Control flow: `if`, loops (`for`, `for-in`, `for-of`, `while`, `do`), labels, `switch`, `break`/`continue`,
  `try`/`catch`/`finally`, `throw`.
- Objects and prototypes, getters/setters, `Object.defineProperty` (also on array elements and `length`),
  `freeze`/`seal`; arrays and the iterator protocol.
- Built-ins written in JS and compiled with the program (only what's used): `Math`, `String`/`Number` methods and
  parsing, `Array.prototype` methods (`map`, `filter`, `reduce`, `sort`, `splice`, …, generic over array-likes, with species), `Array.from`/`of`/`fromAsync`, `JSON.stringify`/`parse`, `Map`/`Set`, typed arrays and `ArrayBuffer`, `Object.assign`/`fromEntries`,
  `hasOwnProperty` and the other `Object.prototype` methods, `Symbol` (`Symbol.for`/`keyFor`, the well-known
  symbols, `Symbol` as a value), `encodeURIComponent` & co., `escape`/`unescape`, `structuredClone`.
- Classes: fields (also computed keys), static members and `static {}` blocks, private `#x` fields, methods and
  getters/setters, `extends` (also built-in errors), `super`.
- Generators (`function*`, `yield`, `yield*`); `Promise` (`then`/`catch`/`finally`, `all`/`allSettled`/`race`/`any`,
  `resolve`/`reject`/`withResolvers`/`try`, also on subclasses), `AggregateError`, `queueMicrotask`; `async`/`await`, including top-level `await`; `for await`
  and async generators.
- Timers (`setTimeout`, `setInterval`, `setImmediate`, `clear*`, `process.nextTick`) on the WASI 0.3 event loop,
  using `wasi:clocks`.
- Libraries: `export async function` becomes a WIT `async func` (callback lift); the call runs jobs and timers
  until its promise settles. `--wit <file|dir> [--world <name>]` implements a user world: scalar types
  (`bool`, integers, `f32`/`f64`, `char`, `string`) and `option`, `result`, `tuple`, `record`, `enum`, `flags`,
  `variant`, `list` with jco's JS shapes (numeric lists as typed arrays: `list<u8>` ↔ `Uint8Array`);
  imported interfaces as ES imports (`import { f } from "ns:pkg/iface"`, an `async func` gives a Promise) and
  exported interfaces as objects (`export const iface = { … }`), so components compose with `wac plug`; see
  [docs/user-worlds.md](docs/user-worlds.md).
- `console.log` and friends, printed like node (also `console` as a value, `console.log(...xs)` and replaced
  methods); uncaught errors and unhandled rejections reported like node.
- Programs of several files: `import`/`export` between ES modules (relative paths), joined into one component at
  build time (namespaces, re-exports, live bindings, cycles, top-level `await` in node's order, `import.meta`, JSON
  modules); `build --command` makes an entry with exports a command, as node runs it.
- Dynamic `import()` of modules known at build time: the build finds what each `import()` can load (string
  literals, `const`s, `?:`, template and `+` patterns like ``import(`./locales/${lang}.js`)``); `--include <glob>`
  adds modules for specifiers only known at run time (an `import()` the build can't work out is an error without
  it). Code that doesn't exist at build time can't be loaded.
- Packages: bare specifiers (`import x from "pkg"`) resolved from `node_modules` at build time as node does
  (`exports` conditions and patterns, `imports`, `main`).
- CommonJS (T54): files node runs as CommonJS (`.cjs`, `"type": "commonjs"`, `.js` without ESM syntax) with
  `require`, `module.exports`/`exports`, `__filename`/`__dirname`, JSON files, cycles, packages with the `require`
  conditions, computed `require` of files known at build time (patterns, `--include`), and interop both ways
  (`import x from "./a.cjs"` with node's named exports, `require("./esm.mjs")`); compiled as strict code. tsc's
  CommonJS output works.
- node's built-in modules, first slice (T55): `path` (POSIX), `events` (`EventEmitter`), `util` (`format`,
  `inspect`, `inherits`, `promisify`, `callbackify`, `deprecate`, `types`, `isDeepStrictEqual`), `assert`, `os`,
  `tty`, `process`, through `require`, `import`, `node:` and `import()`; the `process` global (`env`, `argv`,
  `exit`/`exitCode`, `'exit'` listeners, `hrtime`, `stdout.write`, …, from `wasi:cli/environment`) and
  `performance.now()`. Packages such as `semver`, `lru-cache`, `inherits`, `minimist` build and match node.
- `Buffer` (T56): the global and `node:buffer` (every encoding, the read/write number methods, node's errors,
  printed `<Buffer 68 69>`), `string_decoder`, `atob`/`btoa`, and `util.inspect.custom` methods when printing;
  `safe-buffer`, `buffer-crc32` and `iconv-lite` match node.
- `fs` (T59) over `wasi:filesystem`, in commands: the sync API (files, directories, stats, links, file descriptors,
  node's errors such as `ENOENT: no such file or directory, open 'x'`), callbacks, `fs/promises`, and `constants`;
  paths must lie under a directory given to `wasmtime run --dir host::guest`. A command may also import WASI
  interfaces directly (`import { getRandomBytes } from "wasi:random/random@0.3.1"`): an `async func` blocks, a
  resource handle is a number, streams are read and written whole.
- `stream` (T60): node 24's own stream code (`Readable`, `Writable`, `Duplex`, `Transform`, `PassThrough`,
  `pipeline`, `finished`, async iteration, operators, `duplexPair`), `stream/promises`, `fs.createReadStream`/
  `createWriteStream`, `fs.opendir`, with node's event order; `graceful-fs` and `fs-extra` match node.
- node's other globals that are built-in modules' exports (T61): `WeakMap`, `WeakSet`, `AbortController`,
  `AbortSignal`, `EventTarget`, `Event`; `console.assert`. Packages bundling npm's `readable-stream` build.
- From the zigbee2mqtt survey (T63): `error.stack` (one `at <anonymous>` frame, made on first read);
  node globals js2wasi3 lacks (`navigator`, `URL`, …) and missing statics (`Reflect.decorate`) are build errors
  only where they always run, else run-time errors (`typeof` gives `"undefined"`); `Object.getOwnPropertyDescriptors`,
  a rest parameter with `arguments`, `arguments` in class constructors, `delete a?.[b]`, `console._stdout`.
- Web globals node has (T64): `URL`/`URLSearchParams` (the WHATWG parser, written from the URL Standard) and
  node's `url` (`parse`, `format`, `resolve`, `fileURLToPath`, `pathToFileURL`, `urlToHttpOptions`) and
  `querystring`; `TextEncoder`/`TextDecoder` (UTF-8, UTF-16, windows-1252); `DataView`.
- `crypto` (T65): random values from `wasi:random` (`randomBytes`, `randomInt`, `randomUUID`, `getRandomValues`,
  the `crypto` global), `createHash`/`createHmac` (md5, sha1, sha224/256/384/512), AES-128/192/256 in ECB, CBC,
  CTR and CCM (`createCipheriv`/`createDecipheriv`), `timingSafeEqual`; `pbkdf2`, `scrypt` and `hkdf` (T149, md5/sha1/sha2
  digests, with their `Sync` forms) are computed by a small Rust component (RustCrypto) that you compose with the
  program (`build` composes it in; `build --ext-lib crypto` forces the import). Built-ins that load with node's shape
  while their facility is missing (T65): `http` (T71: its server works), `https`, `http2`, `tls`, `dns`, `zlib` (`crc32`
  works), `perf_hooks`, `module` (`createRequire` fails with `MODULE_NOT_FOUND`), and `child_process` (every
  spawn fails with `ENOENT`, as for a missing program; T70: shell commands fail as with a `/bin/sh` that finds no
  program, exit code 127).
- `net` (T66): TCP clients and servers over `wasi:sockets` on the event loop (`net.Socket` is a `stream.Duplex`;
  `connect`/`createConnection`, `createServer`/`listen`/`close`, `setTimeout`, `setKeepAlive`, node's errors such as
  `connect ECONNREFUSED 127.0.0.1:1`), and `dns.lookup` over `wasi:sockets/ip-name-lookup`. wasmtime needs
  `-S inherit-network` (and `-S allow-ip-name-lookup` for host names).
- `http` servers (T71): node 24's own `_http_server` over `net` with a JS llhttp (`createServer`, keep-alive,
  chunked bodies and trailers, `Expect: 100-continue`, `upgrade`/`connect` events, `clientError`, timeouts), so
  `ws` WebSocket servers work; the web `Headers`/`Request`/`Response`/`ReadableStream`/`Blob` globals;
  zigbee2mqtt's frontend (srvx) serves its pages and WebSocket like node.
- `fetch` (T72) over `wasi:http` (wasmtime needs `-S http`; the host does HTTPS): methods, headers, bodies,
  redirects, `AbortSignal`, streamed responses and undici's errors; zigbee-herdsman's OTA index and firmware
  downloads give node's results. `http.request` isn't there yet.
- `dgram` (T67): UDP sockets over `wasi:sockets` (`createSocket`, `bind`, `send` with and without `connect`,
  `message` with `rinfo`, `close`, `address()`/`remoteAddress()`, node's errors). No multicast or broadcast
  (WASI 0.3 has neither).
- `Error.captureStackTrace` (T68). With it, zigbee2mqtt 2.14.1 runs against a live Z-Stack coordinator and an MQTT
  broker like node: network start, coordinator backup, MQTT, Home Assistant discovery, its stop and restart (see
  [docs/zigbee2mqtt-feasibility.md](docs/zigbee2mqtt-feasibility.md), "Live coordinator").
- `zlib` (T150): `deflate`/`inflate`/`gzip`/`gunzip`/`deflateRaw`/`inflateRaw`/`unzip` in every form (`*Sync`, callbacks, `create*`
  streams, `flush`/`params`/`reset`/`close`, dictionaries, `info`, `maxOutputLength`, `rejectGarbageAfterEnd`) with node's
  own `lib/zlib.js` over a Rust helper component (`libz-rs`, zlib in Rust) that `build` composes into any program that names
  `zlib` (`--ext-lib zlib` forces it). Errors and codes are zlib's (`Z_DATA_ERROR -3 incorrect header check`); the compressed
  bytes differ from node's (other match choices), so compare round trips, not bytes. Brotli (`brotliCompress*`, `createBrotli*`, params) is a second helper (`brotli` crate, 1 MB, linked only into programs that name
  `brotli`) and gives node's bytes at the same quality/window/mode; zstd (`zstdCompress*`, `createZstd*`, dictionaries, params, pledged size) is a third helper
  (libzstd 1.5.7 built with clang, 585 KB, linked only into programs that name `zstd`), also node's bytes. Express's
  `compression` middleware answers like node.
- Express 5.3 (T148): a notes + todo web app runs as one component, answering like node (one shim, for `depd`'s call-site
  API): [examples/express](examples/express/README.md), with the steps to install, build and run it. `morgan` and `ejs` are not there yet;
  `pbkdf2-password` (Express's `auth` example) works with the crypto helper (T149).
- Explicit resource management: `using`/`await using` declarations, `DisposableStack`, `AsyncDisposableStack`,
  `SuppressedError`.
- `globalThis` (and node's `global`): the built-ins as its properties, undeclared names through it. `eval("…")`,
  `Function("…")` and indirect eval of code known at build time (string literals) are compiled ahead of time
  (completion values, the caller's scope, SyntaxErrors at the call); code built at run time can't be (`eval(x)` is
  a build error, `Function(x)` of computed code builds and throws an `EvalError` when it runs (T148), `eval` called through a
  variable with a string throws an `EvalError`).

Which Node.js APIs work, API by API, with test262 and benchmark results: [docs/compatibility.md](docs/compatibility.md),
also at <https://harryzz.github.io/js2wasi3/> (generated by `scripts/compat.mjs`). Where the time goes compared
with node (perf profiles of the benchmarks): [docs/performance.md](docs/performance.md).

Not yet: many built-ins (`Intl` beyond `DateTimeFormat` in en-US, `WeakRef`), sloppy-mode CommonJS semantics, most of node's built-in
modules (`http.request`, `tls`, `zlib`, `worker_threads`, …), and richer WIT types at the boundary (resources; `future<T>` and `stream<T>` work in `async func`s). See [docs/keywords.md](docs/keywords.md) for keyword-by-keyword status and the Follow-ups table in
[tasks/README.md](tasks/README.md) for every known gap.

## How it works

```text
entry.js ──frontend──▶ AST + scopes ──lower──▶ IR ──backend──▶ core Wasm ──component──▶ component.wasm
          (oxc)                       (IR passes)  (wasm-encoder)           (wit-component, WASI 0.3.1 WIT)
```

| Crate | Role |
|---|---|
| `crates/frontend` | parsing and scope analysis ([oxc](https://oxc.rs)), diagnostics |
| `crates/ir` | the IR, its validator and passes (e.g. the suspension transform for generators and async functions) |
| `crates/lower` | JS AST → IR, type inference, closure analysis, "unsupported" diagnostics |
| `crates/backend` | IR → core Wasm: the GC runtime, exceptions, promises, the host event loop |
| `crates/component` | WIT world generation and component packaging |
| `crates/js2wasi3` | the command-line tool |

The design decisions (heap layout, exceptions, async, the WIT boundary) are in [docs/design.md](docs/design.md).
Work is organised as tasks in [tasks/](tasks/README.md), each with its goal, progress log and results.

## Testing

```sh
cargo test                 # unit, IR snapshot and component tests
scripts/diff.sh            # differential tests: every fixture run under node and js2wasi3 + wasmtime
scripts/check-tools.sh     # check the pinned tool versions
```

node is the reference for JavaScript behaviour: `scripts/diff.sh` compares stdout, stderr and the exit status of
each program in `tests/fixtures/` between node and the compiled component.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
