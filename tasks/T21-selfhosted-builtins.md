# T21: Self-hosted built-ins

Status: in progress | Depends on: T16 (can start after T13)

## Goal
A pipeline that compiles built-ins written in JS/TS (with `__intrinsic_*` calls into the backend runtime) together with
user code, keeping only what's used. First batch: `Math`, `String.prototype` basics, `Array.prototype` basics, `JSON.stringify` for primitives, arrays and objects.

## Acceptance
- The built-ins compile with the same frontend/IR/backend (no separate compiler), and fixtures using them match node.
- A build of `hello.js` pulls in no unused built-in (check the size in the Result).

## Notes / Result

### Slice 1 (2026-09-26): the pipeline + `Math` — done
- **Pipeline** (design in D5): `crates/lower/builtins/math.js` is the prelude; `lower::parse` parses the user's
  source alone (its diagnostics are unchanged), then user source + `\n` + prelude as one module
  (`frontend::parse_with_prelude`, `Program::user_end`/`in_prelude`). `lower/src/builtins.rs::Prelude` finds the
  prelude declarations reachable from the user's code (`Math.floor` → `$$Math_floor`, then references inside
  them); `Lowerer::run` declares only those, `declare_nested` and closure-analysis diagnostics skip the rest.
  `Math.x(…)` is a direct typed call (`Lowerer::builtin_static`), `Math.x` a function object named `x`,
  `Math.<constant>` an `f64`. `__intrinsic_*` calls (`Lowerer::intrinsic_call`, prelude only): new IR ops
  `UnOp::{Sqrt,Floor,Ceil,Trunc,Fround,Clz}`, `BinOp::{Min,Max,CopySign}`,
  `Intrinsic::{F64Hi,F64Lo,F64FromWords,RandomSeed}`.
- **`Math`**: all functions and constants. fdlibm ports (the source of V8's `ieee754.cc`, incl. V8's `exp(1)` special
  case and the classic `e_log10.c`); `hypot` is V8's algorithm; `pow` is `**` (`backend/src/pow.rs`); `random` is
  xorshift128+ seeded from `wasi:random/insecure` (imported only when used; commands only).
- **Tests**: `tests/fixtures/builtins/math-{basic,fdlibm,random}.js` (diff.sh), `unsupported.rs::builtins_prelude`.
  900 random inputs per function: identical to node 20 except `pow` (F77).
- **Acceptance**: `hello.js` builds byte-identical to before (52174 bytes): no unused built-in is pulled in.
- **Commands run**: `cargo test` (64 ok), `scripts/diff.sh` (68 passed, 0 failed, 9 skipped), `wasm-tools validate`.
- **Unsupported**: `Math` as a value (F81); `Math.random` in libraries (F83); prelude speed (F82); built-ins used as
  values make their parameters `Any` for every call (F84).
- **Next smallest step**: slice 2 (strings). String methods live on `String.prototype`, so the reachability scan
  needs a second rule: a property name (`.slice`) read anywhere in user code (or any computed key) keeps
  `$$String_prototype_slice`, and `String.prototype`'s sentinels get replaced by the prelude's function objects.

### Slice 2 (2026-09-26): strings, string → number, number formatting — done
- **Prototype methods from the prelude**: `crates/lower/builtins/string.js`. `$$String_prototype_m` /
  `$$Number_prototype_m` are kept when user code names a property `m` (`x.m`, `x["m"]`; `builtins::Prelude`),
  listed in `ir::Module::builtin_methods` (adapter + `length`), and installed by the backend's lazy
  `object::proto` over the sentinels (`Cx::builtin_methods`). Calls go through the prototype (it's mutable),
  so their parameters are `Any`. Global functions: `$$parseInt` is `parseInt` (direct call; as a value a
  function object); `Number.parseInt/parseFloat` are the same objects; `Number.<constant>` folds.
- **String → number (F20)**: `$$StringToNumber` (hex/octal/binary, `Infinity`, decimal literals); the runtime's
  `$to_number` calls it (`ir::Module::string_to_number`, `Cx::string_to_number`). The lowering keeps it when
  the IR converts a JS value to a number, uses `==` on JS values or `JsLess` (`Lowerer::may_convert_string`
  → `Facts::string_to_number` → one more round with it as a root, kinds reset because function ids shift).
  Decimal → double is correctly rounded: Clinger's fast path, else a guess refined by big-integer comparisons
  (base-2^24 limbs in JS arrays). `<`/`>`/`<=`/`>=` on two JS values: `Intrinsic::JsLess` (strings by code units).
- **Methods**: `String(x)`, `String.fromCharCode/fromCodePoint`; `at charAt charCodeAt codePointAt concat
  endsWith includes indexOf lastIndexOf padEnd padStart repeat replace replaceAll slice split startsWith
  substring substr toLowerCase toUpperCase toLocale*Case trim trimStart trimEnd trimLeft trimRight isWellFormed
  toWellFormed toString valueOf`; `Number(x)`, `parseInt`, `parseFloat`, `isNaN`, `isFinite`,
  `Number.isNaN/isFinite/isInteger/isSafeInteger`, `Number.prototype.toString(radix)` (V8's
  DoubleToRadixCString), `toFixed` (exact, big integers), `valueOf`. Errors use node's messages.
- **New IR**: `Intrinsic::{StrUnit, StrSub, StrNew, StrSet, JsLess, Trap}`; prelude intrinsics `unit`, `sub`,
  `str_new`, `str_set`, `str_len`, `to_string`, `number_to_string`, `trap` (`Call::Intrinsic(kind)`).
  `ir::visit_exprs` moved from the backend to `ir`.
- **Tests**: `tests/fixtures/builtins/{string-methods,string-number}.js` (diff.sh), `builtins/lib.js` (library,
  `build.rs::builtins_in_a_library`), `unsupported.rs::builtins_prelude`; the F20 rejection test and
  `tests/fixtures/unsupported/string-to-number.js` are gone. Ad-hoc: 1500 random decimal strings (`Number(s)`) and
  800 random doubles (`toString(2/3/16/36)`, `toFixed(3/17)`): identical to node. `emit.rs::invoke` now uses a
  unique temp path per call (three tests wrote the same `…-check/m.wasm` in parallel; it showed up now).
- **Acceptance**: `hello.js` pulls in no prelude code: 52177 bytes (+3 vs slice 1: `$object_to_number` now hands
  a remaining string to `$to_number` instead of trapping). A loop like `for (i = 0; i < a.length; i++)` does
  pull in StringToNumber (`i < a.length` converts a JS value): +10 KB (F86).
- **Commands run**: `cargo test` (all ok, 64), `scripts/diff.sh` (70 passed, 0 failed, 10 skipped),
  `wasm-tools validate`, `cargo clippy --all-targets`, `cargo fmt`.
- **Unsupported / follow-ups**: F85 (runtime-only string → number without the hook traps), F86 (size), F87
  (missing string methods, case mapping coverage), F88 (`String`/`Number` as values, wrappers, `toPrecision`…,
  computed method keys), F89 (non-decimal parsing past 2^53), F90 (typed direct calls of string methods).
- **Next smallest step**: slice 3 (arrays): `$$Array_prototype_map` etc. in a new `builtins/array.js`, same
  mechanism (the Rust-side `array::proto_methods` stay for `push/pop/join/toString`); needs a callback call
  (`f(x, i, a)` on a value works already) and `Array(n)`/`Array.from`/`of` (`$$Array_from`, a global `Array`
  call: `builtin_global` skips names that are `Builtin`s, so `Array(...)` needs its own case).

## Handoff (written 2026-09-26 at the end of the T19/T24 session)

**State:** `main` at `b55c8cf`, clean, pushed to `github.com/harryzz/js2wasi3` (private). T01–T19 and T24 done.
The user chose T21 before T20 because built-ins are the biggest practical gap (see `docs/keywords.md`).
Commit/push only when the user asks; no attribution lines (user's global rules).

**Run first (baseline):**
```sh
scripts/check-tools.sh
cargo test -q --no-fail-fast 2>&1 | grep 'test result'   # 63 tests, all ok
scripts/diff.sh                                          # 65 passed, 0 failed, 9 skipped
```

**Split T21 into slices** (one session each; this file covers all of them, log each slice in Progress):
1. **The pipeline + `Math`.** Decide how built-in source is written and compiled (below), then `Math` (constants,
   `abs/floor/ceil/round/trunc/sign/min/max/sqrt/pow/hypot/random`, then `exp/log/sin/cos/…`). `Math.pow` shares
   `backend/src/pow.rs` (`js_pow`, fdlibm; the node last-bit difference is F77: record, don't investigate).
2. **Strings:** `String(x)`, `String.prototype` basics (`slice`, `substring`, `indexOf`, `includes`, `startsWith`,
   `endsWith`, `split`, `trim*`, `toUpperCase/LowerCase` (ASCII first), `charAt`, `charCodeAt`, `at`, `repeat`,
   `padStart/End`), number `toString(radix)`/`toFixed`, `Number(x)`, `parseInt`/`parseFloat`, string → number (F20).
3. **Arrays:** `Array.prototype` basics (`map`, `forEach`, `filter`, `reduce`, `indexOf`, `includes`, `slice`,
   `splice`, `concat`, `reverse`, `sort`, `find*`, `some`/`every`, `fill`), `Array.from`/`of`, `new Array(n)` (F49).
4. **`JSON.stringify`** (primitives, arrays, plain objects), `Object.assign`/`fromEntries` (F39), `hasOwnProperty`,
   `isPrototypeOf`, built-in `toString`/`valueOf` (F37), `isNaN`/`isFinite`.

**How built-ins work today (read these first):**
- `crates/backend/src/object.rs`: `methods(b)` lists every built-in prototype's method names; a method not
  implemented holds the `G_BUILTIN` sentinel and **traps when read** (`builtin_not_implemented`). Real native
  methods are installed from tables (e.g. ~line 1064: `array::proto_methods()` → `push`/`pop`/`join`/`toString`,
  hand-written Wasm helpers with the `$code` signature `(self, this, args) -> jsval`). Each slice replaces
  sentinels with real methods; the sentinel mechanism stays for the rest.
- `crates/lower/src/lib.rs`: unknown globals are rejected at ~4702 (`global \`X\` (planned: T21)`);
  `builtin_ident` (~3848) and `object_static` (~3872) map `Object.keys` etc. to IR intrinsics; `global_function`
  maps `Promise`, `queueMicrotask`, timers to intrinsics that make a function object on first use (pattern:
  `promise::Helper::QueueMicrotask`, `timers::Helper::Fn`).
- String → number is rejected at ~634 (F20); `console.log` spread and `console.log` as a value are rejected too.

**The first design decision (slice 1): how to write the built-ins.** D5 says "built-ins written in JS/TS and
compiled by js2wasi3 itself, using intrinsics". Options to weigh, then record the choice in `docs/design.md` D5:
- (a) **Self-hosted JS prelude**: a `builtins/*.js` source (embedded with `include_str!`) lowered into the same
  IR module as the user program, calling `__intrinsic_*` for what JS can't express (bit casts, string units,
  object internals). Needs: a second parse/lower input without multi-file modules (F7), names that can't clash with
  user code, only-used functions kept (the acceptance: `hello.js` pulls in nothing), and built-in functions' type
  inference not polluted by user call sites.
- (b) **Hand-written Wasm helpers** like today's (`array.rs`, `pow.rs`): fastest to add, no pipeline, but it's the
  opposite of D5 and each method is long Rust.
- A mix is reasonable: numeric kernels as Wasm helpers (`Math.*`, number formatting already in `numstr.rs`),
  higher-level methods (`map`, `JSON.stringify`) self-hosted. Whatever is chosen, keep the backend free of new
  dependencies (the user rejected a WAT-text dependency: the compiler may target more than Wasm later).

**Constraints and user preferences:**
- node is the oracle; add fixtures under `tests/fixtures/builtins/` and run `scripts/diff.sh` on them.
- Don't sink time into bit-exact numeric investigations: use the standard algorithm (fdlibm/musl), note any
  difference vs node as a follow-up, and move on. Fixtures test exact/special values where results must match.
- Unsupported things must be rejected at build time or trap loudly, never give a wrong value silently.
- Follow-ups aimed at T21 in `tasks/README.md`: F20, F23, F24, F37, F39, F42 (`arguments`), F43, F47, F49, F51,
  F53, F59, F62, F64, F67, F70, F74, F77. Tick the ones each slice resolves.

## Progress
- 2026-09-26: slice 1 started. Baseline: 63 cargo tests ok, diff.sh 65 passed / 0 failed / 9 skipped.
- 2026-09-26: design (option a, self-hosted prelude): `crates/lower/builtins/*.js` is appended to the user
  source (after the user source alone parsed cleanly, so user diagnostics are unchanged) and parsed as one module.
  Prelude top-level names start with `$$` (`$$Math_floor` is `Math.floor`); unresolved `__intrinsic_*(…)` calls
  in the prelude map to IR ops / intrinsics. Only prelude functions reachable from the user code (AST scan:
  `Math.floor` → `$$Math_floor`, then references inside needed prelude functions) are declared and lowered,
  so programs without built-ins produce the same IR as before. Numeric kernels (sqrt/floor/ceil/trunc/min/max/
  copysign/f64 bit words) become IR unary/binary ops; fdlibm routines are ported to JS in the prelude.
- 2026-09-26: pipeline works. `frontend::parse_with_prelude` (+ `Program::user_end`), `lower::parse` (all callers
  but the `parse` subcommand use it), `lower/src/builtins.rs` (`Prelude`: names, reachability, `is_dead`),
  `Lowerer::{builtin_static, intrinsic_call}`, `Call::Number`. New IR ops: `UnOp::{Sqrt,Floor,Ceil,Trunc,Fround,Clz}`,
  `BinOp::{Min,Max,CopySign}`, `Intrinsic::{F64Hi,F64Lo,F64FromWords}`. `builtins/math.js`: abs/floor/ceil/trunc/
  round/sign/sqrt/fround/clz32/imul/pow/max/min/hypot; `Math.<const>` folded in the lowering; `Math.f` as a value
  is a function object named `f`. `tests/fixtures/builtins/math-basic.js` passes diff.sh. `hello.js` is
  byte-identical to the baseline build (52174 bytes).
- 2026-09-26: fdlibm ported to `builtins/math.js` (sin/cos/tan with `rem_pio2` + `kernel_rem_pio2`, asin/acos/
  atan/atan2, exp/expm1, log/log1p/log2/log10, cbrt, sinh/cosh/tanh, asinh/acosh/atanh). `math-fdlibm.js`
  passes; log10 needed fdlibm's classic `e_log10.c` (FreeBSD's differed from node in the last bit on 2 values).
  Ad-hoc check: 900 random inputs per function, all identical to node except `pow` (22 last-bit, F77).
- 2026-09-26: `Math.random`: xorshift128+ in the prelude, state in prelude `let`s (prelude variables with a
  literal initializer become globals, also in commands; reachability covers them), seeded by
  `Intrinsic::RandomSeed` = `wasi:random/insecure.get-insecure-random-u64`, imported after the fixed imports
  only when used (`host::import_list`); the command world lists the interface (unused → not in the component).
  Libraries: rejected where the user names `Math.random` (T20). `math-random.js` passes.
- 2026-09-26: rejections: `Math.nope` (T21), user references to `$$…` prelude names / `__intrinsic_*`
  ("global"), `Math` as a value ("global `Math`"); a user `$$` name clashing with the prelude gets a help line.
  `Math.max/min/hypot.length` = 2 (`builtins::length`). Tests: `unsupported.rs::builtins_prelude`. Full runs:
  cargo test all ok (64), diff.sh 68 passed / 0 failed / 9 skipped.
- 2026-09-26: slice 2 (strings) started. Baseline: 64 cargo tests ok, diff.sh 68 passed / 0 failed / 9 skipped.
  Plan: prototype methods `$$String_prototype_<m>` in `builtins/string.js`, kept when user code names `.m` (or a
  literal key `"m"`); the lowering lists them in `ir::Module::builtin_methods` and the backend's `String.prototype`
  installs them over the sentinels (calls go through the prototype: it's mutable). String intrinsics
  (`unit`/`sub`/`str_new`/`str_set`/`to_string`). String → number (F20) is `$$StringToNumber` in the prelude, a
  hook the runtime's `$to_number` calls; kept when the IR has a JsVal → f64 conversion or `==` on JS values.
- 2026-09-26: pipeline for prototype methods works: `builtins/string.js`; `Prelude::new(program, extra)` keeps
  `$$B_prototype_m` when a property `m` is named, `$$f` for a global `f`; `ir::Module::{builtin_methods,
  string_to_number}`; `Cx::{builtin_methods, string_to_number}` in the backend (`object::proto` installs them,
  `$to_number` calls the hook). New IR: `Intrinsic::{StrUnit, StrSub, StrNew, StrSet, JsLess, Trap}`
  (`JsLess` = `<`/`>`/`<=`/`>=` on two JS values, strings by code units). `ir::visit_exprs` moved to `ir`.
  Pitfalls found: prelude methods' params must be observed `Any` (only called through the prototype), and a
  `for (;;)` with only inner returns needs a trailing `return` (else the result joins `undefined`: `Any`).
- 2026-09-26: `builtins/string.js` written (methods, StringToNumber, parseInt/parseFloat, Number statics,
  toString(radix), toFixed). Bug found: facts keyed by `FuncId` went stale when StringToNumber was added as a
  root (ids shift): that round now starts the kinds over. Fixtures string-methods/string-number pass; stress
  check vs node identical. Library test, unsupported tests updated; docs (D5, keywords.md, README) updated.
- 2026-09-26: slice 2 done (see Notes / Result). Full runs: cargo test all ok, diff.sh 70/0/10.
