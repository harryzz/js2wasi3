# T148: gaps found running Express (the `__proto__` getter, a function as a property key, `Function` of computed code)

Status: done (2026-10-21) | Depends on: T143 | Local (+ a-03 for test262, after asking)

## Goal
Running Express 5.3.0 on js2wasi3 (`temp/express-demo/`, 2026-10-21) needed four shims; three are js2wasi3 gaps:
1. Reading `x.__proto__` traps (`builtin_not_implemented`: `Object.prototype.__proto__`'s getter is a sentinel); reached by
   `qs` through dunder-proto/get-proto/get-intrinsic.
2. `obj[someFunction]` (a function, or an object with a user `toString`, as a property key) traps
   (`user_to_primitive_not_implemented`); reached by `mime-types`.
3. `Function(…)` / `new Function(…)` of code known only at run time is a **build error** even when the call is never
   executed (`function-bind`); `eval`/`Function` as values already throw `EvalError` at run time.
Also seen: a strict-mode `arguments.callee` doesn't throw a TypeError. `depd` (V8's CallSite API, `new Function` with
computed arguments at load time) stays a stub.

## Read first
- `docs/keywords.md` (`eval`, `Function`), `crates/backend/src/object.rs` (`property_value`, `BuiltinTrap`, `to_property_key`),
  `crates/backend/src/runtime.rs` (`to_primitive`), `temp/express-demo/README.md`.

## Plan
- [x] `__proto__` getter (read) and setter-by-descriptor behave as the spec says (`Object.getOwnPropertyDescriptor(Object.prototype, "__proto__")`).
- [x] ToPropertyKey of a function / an object with a user `toString` or `Symbol.toPrimitive`.
- [x] `Function(computed)` → a run-time `EvalError` (a build warning if the compiler has them).
- [x] Fixtures; Express without the three shims; counts/size; test262 on a-03 (ask).

## Notes / Result
Express 5.3.0 now runs as a js2wasi3 component with one shim (`depd`). The demo app is `examples/express/` (notes + todo, README with the steps).
- **What changed (js2wasi3):**
  1. `Object.prototype.__proto__` has a getter (`builtins/object.js`, getter flag in `object.rs`): `x.__proto__` as a read used to trap.
  2. A function as a property key / `` `${f}` `` / `f + ""` in a program that never names `toString` (no source text kept, T25) gives
     `function name() { [native code] }` instead of trapping (`to_primitive`, `object.rs`).
  3. `Function(x)` / `new Function(x)` of computed code builds and throws an `EvalError` when it runs (`Arg::Throw`, `frontend/src/eval.rs`);
     `eval(x)` of computed code is still a build error.
  4. `net.Server.listen(port)` without a host listens on `::` and on `0.0.0.0` (a WASI IPv6 socket is IPv6-only; node's is dual-stack): a browser
     or `curl 127.0.0.1` could not reach `app.listen(3000)` before (`frontend/node/net.js`, `_bindExtra`).
  5. `fs.Stats` times: `mtimeMs` keeps the fraction, the Dates round to the nearest ms like node (`frontend/node/fs.js`); Express's `send` built a
     different `ETag` for `res.download`/static files.
- **Tests:** fixtures `objects/proto-getter.js`, `objects/function-as-key.js`, `node/net-listen-dual-stack.js`; build tests
  `function_of_computed_code_builds_and_throws_at_run_time`, `function_without_source_text_converts_to_a_native_code_string`
  (`errors_of_every_module_are_reported` uses two `eval` modules now). All fixtures 285/285 in the last full run (284 before the net fixture); build
  tests 36/36; full test262 on a-03: see `tests/test262/HISTORY.md`.
- **Express's 20 example apps vs node** (same requests, `NODE_ENV=production`): identical for hello-world, web-service, params, resource, route-map,
  route-middleware, cookie-sessions, markdown (8). The others fail for reasons outside this task: `morgan` and `ejs` build code with `new Function` at
  run time (cookies, error, error-pages, static-files, route-separation, ejs, view-locals, mvc), `crypto.pbkdf2` is missing (auth),
  content-negotiation needs `--include` (a computed `require`).
- **The shim:** `depd` reads V8's call-site API and builds functions from computed strings at load time: replaced by `examples/express/shims/depd.js`.
- **Commands:** `js2wasi3 build`, `wasmtime run -Sp3 -S inherit-network --dir "$PWD" server.wasm 3300`, `examples/express/smoke.sh`,
  `scripts/diff.sh`, `cargo test -p js2wasi3 --test build`, `node scripts/test262.mjs subset` (a-03).
- **Not done (follow-ups):** F484 (`__proto__` descriptor `set`), F485 (strict `arguments.callee`), F486 (call-site API), F487 (source text for
  implicit conversions), F488 (`eval(x)` as a run-time error), F489 (`crypto.pbkdf2` and the other missing `crypto` functions), F490 (`morgan`/`ejs`: run-time
  code generation), F491 (`fs.statSync(path, { bigint: true })` gives plain numbers, no `mtimeNs`).

## Progress
- 2026-10-21: started (user: "yes" to fixing the compiler gaps found with Express). Tools unchanged (wasmtime 50.0.0).
- 2026-10-21: (1) `Object.prototype.__proto__`'s getter: `$$Object_prototype___proto__` in `builtins/object.js` and the getter flag for
  `Builtin::Object` + `__proto__` in `object.rs` (the setter stays the special case of the property setters); `[].__proto__`,
  `Object.create(null).__proto__`, class/function/primitive receivers and `d.get.call(null)` = node (`objects/proto-getter.js`).
- 2026-10-21: (2) ToPropertyKey/ToPrimitive of a function when the code never names `toString` (no source text kept, T25): `to_primitive`'s
  built-in branch now gives `function name() { [native code] }` instead of the `user_to_primitive_not_implemented` trap
  (`objects/function-as-key.js`; the old "user" case of `unimplemented_object_features_trap` became
  `function_without_source_text_converts_to_a_native_code_string`). The text is not the source: a program that observes it must name `toString`.
- 2026-10-21: (3) `Function(…)` / `new Function(…)` of computed code (no spread): `Arg::Throw` in `crates/frontend/src/eval.rs`: the arguments
  are evaluated, then an `EvalError` is thrown at run time; `eval(x)` of computed code stays a build error. Test
  `function_of_computed_code_builds_and_throws_at_run_time`; `errors_of_every_module_are_reported` now uses two `eval` modules
  (`unsupported/report-all/dyn.cjs` is an indirect eval); `docs/keywords.md` updated.
- 2026-10-21: Express 5.3.0 (`temp/express-demo/`) with only the `depd` stub (function-bind, dunder-proto and mime-types unmodified): builds in
  23 s, output identical to node. `cargo test -p js2wasi3 --test build` 36/36, frontend tests pass.
- 2026-10-21: all fixtures (`scripts/diff.sh -j 4`): 284 passed, 0 failed, 19 skipped. Next: full test262 on a-03 (ask).
- 2026-10-21: found by running Express's example apps (`temp/express-demo`, scratch `ex/`): (4) an unqualified `listen(port)` was IPv6-only (a WASI
  IPv6 socket is IPv6-only; node's `::` is dual-stack): `crates/frontend/node/net.js` now binds `::` and a second `0.0.0.0` socket on the same port
  (`_bindExtra`, closed with the server); fixture `node/net-listen-dual-stack.js`; `node/net.js`, `http.js`, `commonjs/ws` pass, `http-server.js` passes alone.
  (5) `fs.Stats` times: `mtimeMs` keeps the fraction and the Dates round to the nearest ms (`crates/frontend/node/fs.js` `ms()`/`Stats`), as node does: Express's `send`
  built a different `ETag` for `res.download`.
- 2026-10-21: Express's 20 example apps vs node (same requests, `NODE_ENV=production`): same responses for hello-world, web-service, params, resource,
  route-map, route-middleware, cookie-sessions, markdown. Not yet: `morgan` (cookies, error, error-pages, static-files) and `ejs` (ejs, route-separation,
  view-locals, mvc) build code with `new Function` at run time (EvalError), `auth` needs `crypto.pbkdf2`, `content-negotiation` needs `--include` (computed `require`).
- 2026-10-21: user: update docs/task files, commit, push. Full test262 on a-03 started (`~/t262-t148.log`) before committing.
- 2026-10-21: full test262 on a-03 (`out/t262-t148.log`, 1,767 s): 28,123 pass, 0 regressions; 8 tests moved from `unsupported` to `fail` (`Function(computed)` builds now). Task done.
