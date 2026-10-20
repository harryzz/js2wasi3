# T139: a split number slot read as a JS value boxes inline (F474)

Status: done (2026-10-20): measured on the laptop and dropped, no speed code kept | Depends on: T134, T138 | Local (+ a-03 for the gate and test262, after asking)

## Goal
`let t = perm[i]; … perm[k-i] = t` (fannkuchredux): `t` is a split number slot; used as a JS value (stored into a
generic array) it is read with `SplitGet`, which calls `box_number` while the object half is null. `box_number` is
5.1% of fannkuchredux (T138), crypto 2.0%. Its body is `fits_i31` → `i31` or `struct.new`: put it inline at the
`SplitGet` site (`runtime::box_inline`) instead of the call. Expected: the call overhead only, so a small gain; the
gate decides, against the `.wasm` growth.

## Read first
- `tasks/T138-fannkuch-crypto-profile.md` (Notes / Result), `tasks/T134-typed-function-versions.md`.
- `crates/backend/src/lib.rs` (`Intrinsic::SplitGet` in the expression emitter), `runtime.rs` (`box_inline`, `fits_i31`).

## Plan
- [x] Count: `box_number` calls per program (`build --count`) and which sites; static call sites per program.
- [x] Build (inline in `SplitGet`); fixtures; laptop instructions and `.wasm` size.
- [x] Gate on a-03 (ask); test262 (ask): not run, dropped before them.

## Notes / Result
- **Dropped** (user, 2026-10-20, option 1). Laptop instructions vs the build before: fannkuchredux −3.0%, crypto
  +1.0%; `.wasm` crypto +0.9%. At most ~0.17% on the 18-program geomean (one program of 18 at −3%), so the a-03 gate
  would only confirm a gain too small to matter. No gate, no test262.
- **Kept:** the counters `box_number.i31` / `box_number.new` in the helper (`build --count` only) and the code as
  `tasks/T139-files/split-get-inline-box.patch` (`SplitGet` boxing with `runtime::box_inline`). `box_number` calls
  per run (`out/count-t139/`): fannkuchredux 64.2M (all `i31`), knucleotide 50.4M, revcomp 20.3M, crypto 14.2M,
  richards 13.7M, raytrace 8.5M new `$number`s + 1.0M `i31`, earley-boyer 5.1M, regexp 5.3M.
- **Learned:** a call that the compiler could inline costs a few percent at most; the integer round trips around it
  (`i31 → f64 → i31`) are the cost to remove (F475). Count the callers by site before inlining any of them.

## Progress
- 2026-10-20: started (user: "yes" after T138). Tools unchanged.
- 2026-10-20: counts (`out/count-t139/`, two new counters `box_number.i31`/`.new` in the helper): `box_number` calls
  per run: fannkuchredux 64,232,062 (all `i31`), knucleotide 50,359,532, revcomp 20,280,271, crypto 14,198,473,
  richards 13,697,632, raytrace 8,541,492 new `$number`s + 992,750 `i31`, navier-stokes 3,529,991 new + 2,961,657
  `i31`, earley-boyer 5,100,467, regexp 5,286,969, the rest < 7M. Only the calls from `SplitGet` are this task.
- 2026-10-20: `SplitGet` boxes inline (`runtime::box_inline`). Calls after (`out/count-t139b/`): fannkuchredux
  64,232,062 → 302, crypto 14,198,473 → 8,620,912; richards, knucleotide, revcomp unchanged (other sites).
  Laptop instructions vs the build before: fannkuchredux **−3.0%** (38.48 G → 37.35 G), crypto **+1.0%**
  (54.69 G → 55.26 G); `.wasm` crypto 580 → 585 KB, fannkuchredux 193 KB same; fixtures (arrays, closures, syntax)
  33 passed. Next: the a-03 gate (ask).
- 2026-10-20: user: drop (option 1). Code reverted to the counters only, patch kept; task done.
