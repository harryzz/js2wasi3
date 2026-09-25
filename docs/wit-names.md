# JS names → WIT names

How js2wasi3 turns the names of ES exports, their parameters and the entry file into WIT names.
Decided at the start of T07 (2026-09-25). Code: `crates/component`.

## Why a mapping is needed

WIT identifiers are much stricter than JS identifiers, and the core export must carry the WIT name
(a core export `sumOdd` next to a WIT `sum-odd` fails with "failed to find export of function `sum-odd`").
So every exported JS name needs a WIT name, and the core module is emitted with the WIT names as export names.

### WIT identifier rules

Checked on 2026-09-25 against the pinned toolchain (wasm-tools 1.259: `component wit`, then `component embed` + `component new`):

- A name is words joined by single `-`. No leading, trailing or doubled `-`.
- Each word is either all lowercase (`a-z0-9`) or all uppercase (`A-Z0-9`). Mixing cases inside a word is an error (`getHttp`).
- The first word starts with a letter. Later words may start with a digit (`x-2y` is valid, `2x` is not).
- WIT keywords (`type`, `func`, `list`, `string`, `result`, `from`, `async`, …; the list is in `wit-parser-0.259.0/src/ast/lex.rs`)
  are written with a `%` prefix in WIT text (`%type`). The name in the binary, and so the core export, is plain `type`.
- `default` is not a keyword and works as is.

| Name | Result |
|---|---|
| `x-2y`, `x2y`, `HTTP-get`, `get-HTTP`, `x-Y`, `utf8-x`, `default`, `%type` | ok |
| `getHttp` | invalid character in identifier 'H' |
| `a--b`, `x-` | identifiers must have characters between '-'s |
| `type` | found keyword `type` (needs `%type`) |
| `2x` | found an integer |

## The mechanism

### 1. Automatic conversion (the normal case)

1. **Split into words** at `_` and `$`, and at case boundaries:
   - before an uppercase letter that follows a lowercase letter or a digit (`sumOdd` → `sum|Odd`, `x2Y` → `x2|Y`);
   - before the last uppercase letter of an uppercase run when a lowercase letter follows (`HTTPServer` → `HTTP|Server`).
   - Digits stay attached to the word before them (`utf8Decode` → `utf8|Decode`, `HTML5Parser` → `HTML5|Parser`).
2. **Lowercase** every word and **drop empty words** (from leading, trailing or doubled `_`/`$`).
3. **Join** with `-`.
4. In the generated WIT text, **every identifier is written with `%`** (`%add`, `%type`). `%` is allowed before any
   identifier, so we don't need to keep a keyword list in sync with wit-parser. `wasm-tools component wit` still
   prints the plain names, escaping only real keywords.

| JS | WIT |
|---|---|
| `add` | `add` |
| `sumOdd` | `sum-odd` |
| `getHTTPServer` | `get-http-server` |
| `x2Y` | `x2-y` |
| `utf8Decode` | `utf8-decode` |
| `_private` | `private` |
| `foo_bar`, `foo$bar` | `foo-bar` |
| `type` | `type` (text: `%type`) |
| `default` (from `export default function`) | `default` |

### 2. Cases that can't be mapped → a diagnostic, never a panic

| Case | Example | Why |
|---|---|---|
| Nothing left after dropping `_`/`$` | `_`, `$`, `__` | no words |
| First word doesn't start with a letter | `_1`, `$0`, `_2fast` | WIT names start with a letter |
| Non-ASCII characters | `café`, `π` | WIT is ASCII only |
| Two exports map to the same name | `fooBar` + `foo_bar` → `foo-bar` | a world's export names must be unique. The comparison ignores case, so a string name `"FOO-BAR"` also collides |

Each diagnostic points at the export's source span, says why the name can't be mapped and suggests the
escape hatch below. For a collision it names both exports (the second as a note, see F5).

### 3. Escape hatch: string export names (plain JS, ES2022)

```js
function _x(a) { return a + 1; }
export { _x as "my-x" };
```

A string-literal export name is used **exactly as written**, with no conversion. It must already be a valid
WIT name, otherwise it's a diagnostic. This handles any name the conversion can't, and the user fixes it in
the JS source without a compiler flag.

### 4. Parameters

Parameter names don't affect the ABI, so they never cause an error. Each one goes through the same conversion.
If the result is invalid, or duplicates an earlier parameter of the same function, it becomes `p<index>` (`p0`, `p1`, …).
Parameters that aren't plain identifiers (destructuring, later) get `p<index>` too.

## Parameter and result types (T09)

Decided with the user at the start of T09. Plain JS has no types, so a generated world picks each
exported function's WIT types like this:

- **JSDoc wins.** `@param {string} name` / `@param {number} n` and `@returns {string}` / `{number}`
  on the exported function (the `/** ... */` comment right before `export function` or `function`).
  Other JSDoc types on an exported function are an error ("JSDoc type `{boolean}` ... planned: T20").
- **Otherwise usage.** A parameter is `string` if the function uses it as a string: an operand of `+`,
  `==`, `!=`, `===` or `!==` whose other operand is known to be a string, a template-literal
  interpolation (`${name}`), or a `.length` read. Everything else is `f64`.
- **Result**: `string` if every returned value is a string (known statically), otherwise `f64`
  (values that aren't numbers go through JS ToNumber at the boundary).

```js
export function greet(name) { return "hi " + name; }   // greet: func(name: string) -> string
/** @param {number} n */
export function label(n) { return `#${n}`; }           // label: func(n: f64) -> string
```

The usage rule is a heuristic: `${n}` alone makes `n` a string. JSDoc overrides it. User-written WIT
worlds (T20) replace all of this.

Strings cross the boundary as UTF-8 (the Canonical ABI, with `cabi_realloc`). JS strings are UTF-16 and
may contain lone surrogates, which WIT strings can't: **a lone surrogate in a returned string becomes
U+FFFD**.

## Package and world name

- **Default: taken from the entry file name.** The file stem goes through the same conversion, with `.` treated like `_`.
  The result is `package js2wasi3:<name>; world <name> { … }`.
  - `add.js` → `js2wasi3:add/add`
  - `sumOdd.js` → `js2wasi3:sum-odd/sum-odd`
  - `my_lib.test.js` → `js2wasi3:my-lib-test/my-lib-test`
  - If the stem can't be mapped (`_.js`, `1.js`), it falls back to `app` (`js2wasi3:app/app`) without an error.
- **Override: `--world <namespace>:<package>/<world>`**, e.g. `js2wasi3 build calc.js --world acme:math/calc`.
  Each part must be a valid WIT identifier. An invalid value is a CLI error. It's used as given, with no conversion.
  (Package versions such as `@1.0.0` aren't supported yet.)
- **The name isn't stored in the component.** Component binaries are structurally typed: they record the exports,
  not the world or package they came from. `wasm-tools component wit out.wasm` always prints
  `package root:component; world root { … }` (hardcoded in `wit-parser-0.259.0/src/decoding.rs`).
  The name is used when the metadata is embedded, and it will matter for user worlds (T20). Writing the generated WIT
  next to the output is follow-up F12.

## Where the code lives

- `crates/component/src/names.rs`: `to_wit` (the conversion) and `is_wit_name` (the validity check).
- `crates/component/src/lib.rs`: `WorldName::{from_path, parse}`, `world()` (names, diagnostics, WIT text) and `package()`
  (embed + encode). Its input is plain `ExportSig`s (name, string flag, local name, params, line/col), so it depends
  on neither the frontend nor the IR.
- `crates/js2wasi3/src/main.rs` (`plan_world`): builds the `ExportSig`s from the IR, turns naming errors into
  source diagnostics, and renames `ir.exports[*].name` to the WIT names before `backend::emit`. The backend didn't change.
- The frontend records whether an export name was a string literal (`Export.string_name`, carried to `ir::Export`).
