# T89: function calls without wasmtime libcalls (F350) and `F.prototype` directly (F369)

Status: done (2026-10-06) | Depends on: T88 (profile) | Local

## Goal
From T88's EarleyBoyer profile: `instanceof`/`new` read `F.prototype` through the generic `get` (F369), and
wasmtime's libcalls (`get_interned_func_ref`, `is_subtype` and their trap-handler wrappers) are ~12% (F350, also
~7% of zigbee2mqtt in T79). Remove them from the hot paths; same results as node.

## Notes / Result
- `fn_prototype` for `new`/`instanceof` (F369), `test_object` instead of `ref.test $object` there; `$function`
  code/construct as code-table slots with `call_indirect` (F350's `get_interned_func_ref`; the probe:
  `probes/funcref.wat`, 5.4×). Generators' resume funcrefs and `is_subtype` elsewhere remain (F350 stays open).
- EarleyBoyer (laptop, `fixed 0.05`): 15.4–17.5 s → 10.4–12.2 s. zigbee2mqtt replay (a-03): 22.59 → 22.07 s.
- Full test262 on a-03: 28,099 pass, 0 regressions. Fixtures: 243 pass (see Progress). Found F370.
- Next smallest step: F368 (property adds search the prototype chain each time: ~16% of EarleyBoyer).

## Progress
- 2026-10-06: `fn_prototype` (func.rs): an ordinary function's `prototype` is its entry 2 (after `length`, `name`,
  non-configurable): read directly when the key there is `prototype`, else the generic `get`; `create_this` and
  `instance_of_value` use it and test the result with `test_object` (no `is_subtype` libcall for the non-final
  `$object`). Also `x instanceof <primitive>` says "is not an object" like V8 (was "not callable"). Repro (replaced
  prototypes, primitive prototypes, classes, `Symbol.hasInstance`, arrows, bound) = node; found F370 (pre-existing).
- 2026-10-06: probe `probes/funcref.wat`: 50M calls through a funcref read from a GC struct (`struct.get` +
  `call_ref`) 1,011 ms vs through an i32 index into a funcref table (`call_indirect`) 187 ms; perf: ~73% of the
  former in `get_interned_func_ref`, its trap-handler wrapper, `engine_type_index`, `FuncRefTable::get_typed`
  (wasmtime interns funcrefs stored in the GC heap).
- 2026-10-06: `$function`'s `code`/`construct` are now i32 slots in a code table (table 0, funcref, an active element
  segment; `Cx::code_slot`), -1 for "not a constructor"; calls go through `call_indirect`. All creation sites
  (closures in lib.rs, func.rs, promise.rs `native` (now takes `cx`), object.rs `Function.prototype`, built-ins)
  pass slots. EarleyBoyer (`fixed 0.05`, precompiled, 3 runs each, laptop): 15.4–17.5 s → 10.4–12.2 s.
  (Generators' resume functions are still funcrefs in their structs: not on this profile's hot path.)
- 2026-10-06: tests: `scripts/diff.sh` on every fixture directory: 243 pass; 4 `node/` fixtures timed out under the
  full parallel load (their builds take ~55 s) and pass alone; `unsupported/bad-names.js` is an expected build
  error (checked by the cargo tests, not diff.sh). `cargo test -p js2wasi3-backend -p js2wasi3 -p js2wasi3-lower`
  ok. Probe added as `probes/run.sh` step 10.
- 2026-10-06: full test262 on a-03 (`~/t262-t89.log`): 28,099 pass, 0 regressions; replay `t79/out-t89`: 22.07 s. Done.
