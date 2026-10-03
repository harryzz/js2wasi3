# Implementing a WIT world in JavaScript

By default js2wasi3 generates the WIT world from the module's exports. With `--wit` you give it your own world,
and the module implements it:

```sh
js2wasi3 build demo.js --wit world.wit -o demo.wasm        # the only world in world.wit
js2wasi3 build demo.js --wit wit/ --world demo -o demo.wasm # a world by name (or ns:pkg/world) from a directory
wasmtime run --invoke 'parse("12")' demo.wasm               # ok(12)
```

The vendored WASI 0.3.1 packages can be imported by the world (`import wasi:clocks/monotonic-clock@0.3.1;`).

## Matching exports

Each function the world exports needs a JS export whose name is its camelCase form (`make-point` →
`export function makePoint`), or a string export name (`export { f as "make-point" }`). The compiler reports a
world export without a JS function, a JS export the world doesn't have, a parameter-count mismatch, and an
`async func` that isn't an `async function` (or the other way round).

## Exporting an interface

A world that exports an interface (`export host;`, `export acme:demo/host;`) is implemented, as in jco, by an
exported object named after the interface in camelCase, whose keys are its functions in camelCase:

```js
function add(a, b) { return a + b; }
export const host = {
  add,                                   // a top-level function
  greet(name) { return `hello ${name}`; }, // a method
  parse: function (s) { return Number(s); },
};
```

Every function of the interface needs a key, and every key must be a function of the interface. The object
itself isn't a JS value (other code can't use `host`), and its methods can't use `this` or `arguments` (F130).

## Importing an interface

A world that imports an interface makes its functions importable by their camelCase names, from the interface's
name with or without its version:

```wit
package acme:demo@0.1.0;

interface host {
  greet: func(name: string) -> string;
  parse: func(s: string) -> result<u32, string>;
}

world app {
  import host;
  export run: func() -> string;
}
```

```js
import { greet, parse } from "acme:demo/host";   // or "acme:demo/host@0.1.0"

export function run() {
  try {
    return greet("js") + parse("x");
  } catch (e) {
    return e.message;                            // "not a number: x", e.payload the same
  }
}
```

Arguments convert like an export's results (missing ones are `undefined`), and results arrive in the JS shapes
of the table below. An import whose result is a `result<T, E>` returns its `ok` value and throws its `err` as
jco's `ComponentError` does: an `Error` whose `payload` is the value and whose message is the value if it's a
string, else `String(value) + " (see error.payload)"`. Only named imports of an interface's functions are
supported (no `import * as`, no world-level `import f: func()`: F131).

An `async func` import returns a promise, settled when the call finishes (a `result`'s `err` rejects it with
the `ComponentError`). Several can run at once, and they run while the module's own timers do. Something must
wait for them, so the world needs an `async func` export: the export's call keeps going until its promise
settles, handling import completions and timers meanwhile.

```js
import { fetch } from "acme:demo/slow";          // fetch: async func(n: u32) -> list<u8>

export async function go(n) {                    // go: async func(n: u32) -> string
  const [a, b] = await Promise.all([fetch(n), fetch(n + 1)]);
  return `${a.length} ${b.length}`;
}
```

Compose the result with a component that exports the interface, e.g. one built from JS as above:

```sh
js2wasi3 build provider.js --wit world.wit --world provider -o provider.wasm
js2wasi3 build app.js --wit world.wit --world app -o app.wasm
wac plug app.wasm --plug provider.wasm -o composed.wasm
wasmtime run --invoke 'run()' composed.wasm
```

## Types

The JS shapes are jco's, so a module written for jco / componentize-js works the same.

| WIT | JS parameter you receive | JS value you return |
|---|---|---|
| `bool` | `true` / `false` | anything (ToBoolean) |
| `s8` … `u32`, `f32`, `f64` | a number | a number (integers wrap like ToInt32, then to their width) |
| `s64`, `u64` | a number (not a BigInt yet) | a number (saturated) |
| `char` | a one-character string | a string (its first code point) |
| `string` | a string | anything (ToString; lone surrogates become U+FFFD) |
| `option<T>` | the value, or `undefined` for `none` | the value; `undefined` / `null` is `none` |
| `tuple<A, B>` | an array `[a, b]` | an array |
| `record { max-size: u32 }` | an object `{ maxSize }` (camelCase keys) | an object |
| `flags { read, exec-now }` | an object `{ read: true, execNow: false }` | an object (each key ToBoolean) |
| `enum { dark-blue, … }` | the case name as a string: `"dark-blue"` | the case name |
| `variant { circle(f64), empty }` | `{ tag: "circle", val: 1.5 }`, `{ tag: "empty" }` | the same shape |
| `result<T, E>` inside another type | `{ tag: "ok" \| "err", val }` | the same shape |
| `list<u8>` (also `s8`, `s16`, `u16`, `s32`, `u32`, `f32`, `f64`) | a `Uint8Array` (`Int8Array`, …, `Float64Array`) | that typed array, or any array-like |
| `list<T>`, other `T` | an array | an array (or any array-like) |
| `future<T>` (in an `async func`) | a `Promise` of the value | a promise, or any value (see [Futures](#futures)) |
| `stream<T>` (in an `async func`) | an async iterable of chunks shaped like `list<T>` (`stream<u8>`: `Uint8Array`s) | any (async) iterable of chunks (see [Streams](#streams)) |

A returned value that doesn't fit its type (an enum string that isn't a case, a `tag` that isn't a case) makes
the call trap (jco throws a TypeError instead: F128).

## `result` results: return or throw

A function whose WIT result is `result<T, E>` **returns** its `ok` value and **throws** its `err` value. The
thrown value becomes `E` by jco's rule:

1. if it has an own `payload` property, that property is the `err` value;
2. otherwise, if it's an `Error`, the call traps (an Error isn't a WIT value);
3. otherwise the thrown value itself is the `err` value.

For a `result` without `E` (`result` or `result<T>`), any throw is `err`.

```wit
package acme:demo;

world demo {
  enum parse-error { empty, not-a-number }
  record problem { code: u32, detail: string }

  export parse: func(s: string) -> result<u32, parse-error>;
  export load: func(path: string) -> result<string, problem>;
  export check: func(ok: bool) -> result;
  export fetch: async func(n: u32) -> result<list<u8>, string>;
}
```

```js
export function parse(s) {
  if (s === "") throw "empty";             // a plain value: it is the `err` payload
  const n = Number(s);
  if (Number.isNaN(n)) {
    const e = new Error(`bad number: ${s}`);
    e.payload = "not-a-number";            // an Error carries its payload in `payload`
    throw e;
  }
  return n;                                // ok(n)
}
export function load(path) {
  if (!path.startsWith("/")) throw { code: 2, detail: `relative path: ${path}` };
  return `contents of ${path}`;
}
export function check(ok) {
  if (!ok) throw new Error("no");          // `result` without `E`: any throw is `err`
}
export async function fetch(n) {
  await null;
  if (n > 3) throw "too many";             // a rejection is `err` too
  return new Uint8Array(n).fill(1);
}
```

| Call | Result |
|---|---|
| `parse("12")` | `ok(12)` |
| `parse("")` | `err(empty)` |
| `parse("x")` | `err(not-a-number)` |
| `load("a")` | `err({code: 2, detail: "relative path: a"})` |
| `check(false)` | `err` |
| `fetch(5)` | `err("too many")` |

Without a `result` in the WIT signature, an exception (or an unhandled rejection) traps: the host sees a failed
call, not a value.

## Async exports

An `async func` is implemented by an `async function` (or one returning a promise). The call runs the promise's
jobs and timers until it settles; a fulfilled value is the result, a rejection follows the `result` rules above.
A module with timers needs `import wasi:clocks/monotonic-clock@0.3.1;` in its world.

## Futures

A `future<T>` is a `Promise` on the JS side, in both directions, anywhere in an `async func`'s signature (a
parameter, the result, inside a record, …):

- A future you **receive** (a parameter, or in what an import returns) is a promise of its value, read as soon as
  it arrives. If the writer drops the future without a value, the promise rejects.
- A future you **give** (a result, or an argument to an import) is made from any value: a promise is awaited and
  its value written when it settles; any other value is written at once. A rejection drops the future (the reader
  sees it dropped) and fails the call. The call goes on after it returned (or the import it was passed to
  started) until every future it gave has been written and read.

An async function's own promise adopts a promise it returns, so an `async func` returning `future<T>` directly
can only write a value it already has; to hand out a value later, put the future in a record or tuple:

```wit
interface later {
  record pair { now: u32, later: future<u32> }
  double-later: async func(n: u32) -> pair;
  shout: async func(s: future<string>) -> string;
}
```

```js
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
export const later = {
  async doubleLater(n) {
    return { now: n, later: sleep(20).then(() => 2 * n) }; // written after 20 ms
  },
  async shout(s) {
    return (await s).toUpperCase(); // s: a Promise<string>
  },
};
```

A `func` (not `async`) with a future in its signature is rejected. `tests/fixtures/library/futures` has a
provider and an app that use futures both ways, composed with `wac plug`.

## Streams

A `stream<T>` is an async iterable of **chunks** on the JS side, each chunk shaped like a `list<T>`: a
`Uint8Array` for `stream<u8>`, a typed array for the other numeric types, an array otherwise.

- A stream you **receive** (a parameter, or what an import returns) is an async generator: each `next()` reads
  once (up to 64 KiB) and gives what that read got. It ends when the writer drops its end. Leaving a
  `for await` early (`break`, `return`, a throw) drops the readable end, so the writer sees the stream end.
- A stream you **give** (a result, or an argument to an import) is made from any async or sync iterable of
  chunks (an async generator, an array of `Uint8Array`s, …), or a stream you received. Each chunk is
  converted like a `list<T>` and written. The stream ends when the iterable is done, or when the reader drops
  its end (then the iterable is closed with `return()`). If the iterable throws, the stream ends and the call
  fails (like an unhandled rejection). The call goes on after it returned until every stream it gave has been
  written.
- A stream doesn't keep write boundaries: the chunks the reader gets needn't be the ones written.

```wit
interface bytes {
  echo: async func(input: stream<u8>) -> stream<u8>;
  upper: async func(input: stream<u8>) -> stream<u8>;
}
```

```js
async function* upperChunks(input) {
  for await (const chunk of input) {
    yield chunk.map((b) => (b >= 97 && b <= 122 ? b - 32 : b));
  }
}
export const bytes = {
  async echo(input) {
    return input; // the received stream, pumped into the result
  },
  async upper(input) {
    return upperChunks(input);
  },
};
```

A `stream` without an element type, and streams of futures or streams, are rejected. `tests/fixtures/library/streams`
has a provider and an app that use streams both ways, composed with `wac plug`.

## Limits

- `option<option<T>>`: `some(none)` and `none` are both `undefined` (F127).
- 64-bit integers are numbers, not BigInts; integer results wrap instead of being range-checked (F124).
- Resources (T59): an imported resource's handle is a number; its functions are imported by their WIT names
  (`import { "[method]blob.read" as read } from "acme:demo/store"`, `self` first) and `[resource-drop]blob` drops
  one. Exported resources and jco's resource classes: not yet.
- Composed components with async calls between them (timers, async imports, streams) can fail when wasmtime
  49 (49.0.1, 49.0.2) collects garbage (a wasmtime internal error or crash with the default collector); running with
  `-O gc-heap-initial-size=64MiB` avoids it for short calls (F134).
- A received stream that is never read to its end or dropped stays open, so its writer waits (F137). Memory
  used while streaming `string`s isn't reclaimed until the next call (F138).
- While an async call waits for its future writes after returning, no other call of the component starts (F135).
- An `async func` import still pending when the export's promise settles finishes during a later async call
  (F133).
- Rest parameters in exported functions are rejected (F55).
