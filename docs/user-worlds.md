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
`async func` that isn't an `async function` (or the other way round). Interface exports and imports aren't
supported yet.

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

## Limits

- `option<option<T>>`: `some(none)` and `none` are both `undefined` (F127).
- 64-bit integers are numbers, not BigInts; integer results wrap instead of being range-checked (F124).
- Resources, `future`, `stream`, interface exports and imports: not yet (T20 slices 5–6).
- Rest parameters in exported functions are rejected (F55).
