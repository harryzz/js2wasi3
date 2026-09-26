# T21: Self-hosted built-ins

Status: todo | Depends on: T16 (can start after T13)

## Goal
A pipeline that compiles built-ins written in JS/TS (with `__intrinsic_*` calls into the backend runtime) together with
user code, keeping only what's used. First batch: `Math`, `String.prototype` basics, `Array.prototype` basics, `JSON.stringify` for primitives, arrays and objects.

## Acceptance
- The built-ins compile with the same frontend/IR/backend (no separate compiler), and fixtures using them match node.
- A build of `hello.js` pulls in no unused built-in (check the size in the Result).

## Notes / Result

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
