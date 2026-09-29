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
> [docs/keywords.md](docs/keywords.md)). Much of the built-in library (`Intl`, `Proxy`…; `Math`, strings, arrays, `JSON`, `Map`/`Set`, `Object` methods, `Reflect`, `RegExp`, `Date`, BigInt and number parsing/formatting are done)
> doesn't exist yet. Unsupported code is rejected at build time with a clear diagnostic rather than
> miscompiled. The known exception, built-in methods that crash when called, is listed in the docs.

## Quick start

Requirements: Rust (pinned in `rust-toolchain.toml`), and [wasmtime](https://wasmtime.dev) 49 to run the output.
The other tools and versions are in [TOOLS.md](TOOLS.md).

```sh
cargo build --release

# A command: a file without exports.
echo 'console.log("hello from js2wasi3");' > hello.js
target/release/js2wasi3 build hello.js -o hello.wasm
wasmtime run -Sp3 hello.wasm          # -Sp3: enable WASI 0.3
# hello from js2wasi3

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
  template literals.
- `let`/`const`/`var` with TDZ checks, closures, arrow functions, `this`, default/rest parameters, destructuring,
  spread.
- Control flow: `if`, loops (`for`, `for-in`, `for-of`, `while`, `do`), labels, `switch`, `break`/`continue`,
  `try`/`catch`/`finally`, `throw`.
- Objects and prototypes, getters/setters, `Object.defineProperty` (also on array elements and `length`),
  `freeze`/`seal`; arrays and the iterator protocol.
- Built-ins written in JS and compiled with the program (only what's used): `Math`, `String`/`Number` methods and
  parsing, `Array.prototype` methods (`map`, `filter`, `reduce`, `sort`, `splice`, …, generic over array-likes, with species), `Array.from`/`of`/`fromAsync`, `JSON.stringify`/`parse`, `Map`/`Set`, typed arrays and `ArrayBuffer`, `Object.assign`/`fromEntries`,
  `hasOwnProperty` and the other `Object.prototype` methods, `Symbol` (`Symbol.for`/`keyFor`, the well-known
  symbols, `Symbol` as a value).
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
- `console.log` and friends, printed like node; uncaught errors and unhandled rejections reported like node.
- Programs of several files: `import`/`export` between ES modules (relative paths), joined into one component at
  build time (namespaces, re-exports, live bindings, cycles, top-level `await` in node's order, `import.meta`, JSON
  modules); `build --command` makes an entry with exports a command, as node runs it.
- Dynamic `import()` of modules known at build time: the build finds what each `import()` can load (string
  literals, `const`s, `?:`, template and `+` patterns like ``import(`./locales/${lang}.js`)``); `--include <glob>`
  adds modules for specifiers only known at run time (an `import()` the build can't work out is an error without
  it). Code that doesn't exist at build time can't be loaded.
- Packages: bare specifiers (`import x from "pkg"`) resolved from `node_modules` at build time as node does
  (`exports` conditions and patterns, `imports`, `main`); ES module packages only.
- Explicit resource management: `using`/`await using` declarations, `DisposableStack`, `AsyncDisposableStack`,
  `SuppressedError`.
- `globalThis` (and node's `global`): the built-ins as its properties, undeclared names through it. `eval("…")`,
  `Function("…")` and indirect eval of code known at build time (string literals) are compiled ahead of time
  (completion values, the caller's scope, SyntaxErrors at the call); code built at run time can't be (`eval(x)` is
  a build error, `eval` called through a variable with a string throws an `EvalError`).

Not yet: many built-ins (`WeakMap`, `Intl`), CommonJS packages, node's built-in
modules (`node:fs`), and richer WIT types at the boundary (resources; `future<T>` and `stream<T>` work in `async func`s). See [docs/keywords.md](docs/keywords.md) for keyword-by-keyword status and the Follow-ups table in
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
