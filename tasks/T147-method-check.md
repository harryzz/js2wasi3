# T147: a cheaper guard for direct method calls (F481)

Status: done (2026-10-21): typed variant measured on a-03 and dropped, no code kept | Depends on: T132, T144 | Local (+ a-03 for the gate and test262, after asking)

## Goal
T144: `method_check` (T132's guard before a direct call of the one function a method call site can reach) costs about 40 ns
per call: richards 21.3% + `method_fn` 4.9% of its samples (13.9M checks), deltablue 14.7% (14.1M checks). It is a
~60-instruction helper with a cache lookup (`G_MCACHE` array, the site's entry, the receiver's keys array, prototype,
epoch, the holder's key and function). Find why it is slow and make the guard cheaper without changing what is cached.

## Read first
- `tasks/T132-method-calls.md` (Notes / Result), `tasks/T144-richards-deltablue-profile.md`, `docs/design.md` "Speed work".
- `crates/backend/src/object.rs`: `method_check`, `method_fill`, `method_fn`; the call-site emission in `crates/backend/src/lib.rs`.

## Plan
- [x] Read the helper and its call site; count (`method.check`, `.miss`, `.filled`, `.given_up`) and the instructions per check.
- [x] Design and build; fixtures (method calls: replaced prototype methods, shadowing own properties, prototype changes,
  receivers of other shapes, getters, proxies/other kinds).
- [x] Laptop counts and size; gate on a-03 (ask); test262 (ask).

## Notes / Result
- **Dropped** (user, 2026-10-21, option 1). The typed variant (the receivers' constructor from the type analysis, one exact shape-type
  test at the site, `method_check_obj` without the chain of type tests) on a-03 vs 2c5af30: geomean **0.9974**; richards −4.0%
  (instructions −5.37%), nothing else moved; `.wasm` +0.06…+0.97%. Left dropped: 4% on one program of 18 for up to 1% more code.
- **Kept:** `tasks/T147-files/typed-method-check.patch` (IR `MethodCheck { layout }`, `Result::fields` also for `method` sites,
  `Helper::MethodCheckObj`, the counter `method.check_obj`) and `tests/fixtures/objects/method-typed-receiver.js` (passes before and after).
- **Learned:** the guard's cost is the dependent loads (~25 per hit: `G_MCACHE` → entry → keys/proto/epoch → holder → its keys and
  slot) and the order of the type tests, not the receiver's chain alone: reversing `cast_plain_or_shape`'s order made richards
  +8.7% and deltablue +23.5% slower (laptop). Shape types are sorted by field count (`shape_ns.sort_unstable()`), not by use.
- **Next smallest steps (not started):** order the shape types by how much they are used (count constructor sites/`new` in loops
  first), which helps every user of the chain (`get_named`, `method_check`); a guard with fewer loads (per-site cache globals, a
  holder check that the epoch covers).

## Progress
- 2026-10-21: started (user: "method_check"). Tools unchanged (wasmtime 50.0.0).
- 2026-10-21: read `method_check`: the hit path is ~100 instructions and ~25 dependent loads (`G_MCACHE` → entry → keys/proto/epoch →
  holder → holder keys/vals), and **two `cast_plain_or_shape` chains of up to 7 sequential `br_on_cast`** (`$plain`, then the shape
  types in ascending field-count order: `shape_ns.sort_unstable()`), one for the receiver and one for the holder.
- 2026-10-21: experiment (not kept): reversing the type order in `cast_plain_or_shape` made richards **+8.7%** and deltablue **+23.5%**
  slower (laptop, minimum of 3), so the position of the receivers' type in the chain matters a lot (every user of the chain:
  `get_named`, `method_check`, ...).
- 2026-10-21: built a typed variant. The type analysis' `Result::fields` (receivers all made by `new F`, F's node) now also covers
  `method` sites (was `get`/`set` only); the lowering passes the receivers' layout to `Intrinsic::MethodCheck { layout }`; the backend
  tests the layout's shape type (one exact `ref.test`) at the site and calls the new `method_check_obj` (the same body, the receiver
  already a `(ref $object)`, no chain), else the old `method_check`. Counters: richards 13,895,724 of 13,895,740 checks take the typed
  path, deltablue 14,142,731 of 14,144,775, crypto 56,761 of 56,882, raytrace 5 of 2,120,186.
  Laptop instructions vs the build before: richards 17.21 G → 16.29 G (**−5.4%**), deltablue 26.369 G → 26.366 G (−0.01%: its receivers'
  types are early in the chain already); `.wasm` richards +0.40%, deltablue +0.67%. Fixtures objects/classes/closures 50/50;
  new `tests/fixtures/objects/method-typed-receiver.js` (other constructors at the site, overrides, replaced/deleted prototype methods,
  changed shapes and prototypes, subclasses, null/primitive receivers) = node.
- 2026-10-21: all fixtures (`scripts/diff.sh -j 4`): 282 passed, 0 failed, 19 skipped. User: yes to the a-03 gate.
- 2026-10-21: a-03 gate of the typed variant vs 2c5af30 (`out/gate-t147-a03.log`, `out/gate-t147/gate.tsv`): geomean **0.9974**
  (minimum times); octane-richards 1,762 → 1,692 ms (**−4.0%**, instructions −5.37%); deltablue −0.8% (−0.01% instructions); the
  rest within ±0.4%, 10 programs byte-identical; outputs = node. `.wasm` +0.06…+0.97% (crypto +0.97%, deltablue +0.67%,
  richards +0.41%). Micros: no verdicts. Decision for the user: keep (small gain on one program, size up to 1%) or leave dropped.
- 2026-10-21: user: option 1 (leave dropped). Code reverted (patch kept); the new fixture stays.
