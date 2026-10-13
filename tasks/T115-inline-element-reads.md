# T115: array element reads inline on receivers of unknown type

Status: done | Depends on: T107, T113 | Local (+ a-03 only after asking)

## Goal
nbody's profile on a-03 (2026-10-13, `fd1b5e7`, recorded in T111's Progress): `get` is 22.9% self:
`bodies[i]` / `bodies[j]` in `advance`, where `bodies` is a parameter of unknown type, so every element read
calls the generic `get` helper (14 per pair of bodies). `get`'s first fast path (an `$array`, a small integer
key, inside the element store, a data value) is cheap; the call and its key boxing are not. The user chose this
before T111 (2026-10-13: "first an inline fast path for array element reads on values of unknown type").

## Design
- In the backend, `GetProp(obj, key)` with a key that isn't a constant string (`lib.rs` `elem_get`): the same
  checks as `get`'s first fast path, inline: `ref.test $array` (final since T113: one compare), the key an i31
  (or, for an `f64` key such as a typed loop index, an integer-valued `f64`: no boxing), inside the element
  store, not null (hole / past `length`), not an accessor; else the `get` call (an `f64` key boxed only then).
- Number-mode arrays (T107) have an empty element store: they take the call, as before.
- Three more scratch locals per function (`i32`, `(ref null $vals)`, `jsval`).

## Plan
- [x] Implement (`elem_get`).
- [x] Checks: `scripts/diff.sh` on `arrays/`, `objects/`, `classes/`, `builtins/`; `wasm-tools validate`.
- [x] Gate on a-03 against `fd1b5e7` (ask): no program or micro case slower (the misses: `o[k]` on plain
  objects, functions, Maps in `object-kinds.js`); test262 (ask).
- [x] (Added after the first gate.) A miss on a receiver that isn't an `$array` calls `get_other` (`get` without
  its array fast path), so it doesn't pay the `$array` test twice.

## Acceptance
- `scripts/gate.sh`: nbody faster, nothing slower; fixtures = node; test262 no changes.

## Notes / Result
- **What changed.** `GetProp(obj, key)` with a non-constant key (`lib.rs` `elem_get`) reads an `$array`'s element
  inline: `ref.test $array` (final: one compare), an i31 key or an integer-valued `f64` key (unboxed, `-0` is 0),
  inside the element store, not a hole, not an accessor. Misses: an `$array` (other key, hole, number mode, past
  the store, accessor) calls `get`; any other receiver calls the new `object::Helper::GetOther` (`get_other`:
  `get` built with `arrays: false`, no array fast path); an `f64` key is boxed only for the call. Three scratch
  locals per function (`i32`, `(ref null $vals)`, `jsval`).
- **First gate (a-03) failed:** revcomp +4.2% (instructions +1.9%: its Buffer reads `line[i]` tested `$array`
  inline and again in `get`), `o[k]` misses +3…+4 ns. `get_other` fixed both.
- **Result (a-03, `out/gate-t115b-a03.log`, vs `fd1b5e7`):** nbody 3,209 → 2,800 ms (−12.7%, instructions
  −16.2%), fannkuchredux 7,217 → 6,505 ms (−9.9%, −17.6%), pidigits −3.5% (equal instructions: placement),
  revcomp −0.4% (−0.3%), the other 14 the same; outputs = node. Micro: `o[k]` misses +0…+2 ns (same). Left
  marked slower: `typeof o` function/Map +2 ns (no element read in that loop; `typeof v` −2…−3 ns in the same
  run) and regex "literal prefix, match at the end" +3.0% (threshold 3%; three regex cases faster): accepted
  as noise/placement by the user. Laptop: nbody −21% (first version), −15% (`out/gate-t115-laptop.log`).
- **test262** (a-03, `out/t262-t115.log`, 1,450 s): no changes since the baseline (28,105 pass, 97.0%).
- **Commands:** `scripts/diff.sh` on `tests/fixtures/{arrays,objects,classes,builtins}`; `wasm-tools validate`;
  `scripts/gate.sh -r 5 -o out/gate-t115 fd1b5e7` (a-03, twice; the old `out/gate` worktree there, at `db256bc`
  with uncommitted edits from an earlier session, refused the checkout and was left untouched);
  `node scripts/test262.mjs subset` (a-03).
- **Not done (follow-ups):** typed-array/Buffer element reads still take the call (F420); number-mode arrays
  at an untyped read take the call (F421); element writes on untyped receivers still call the helper (F422).
- **Next smallest step:** T111 (number fields unboxed), from the "Handoff" section of `tasks/T111-number-fields.md`.

## Progress
- 2026-10-13: implemented. Laptop: nbody 4,078 → 3,208 ms (−21%), instructions 35.96G → 30.19G (−16%), output =
  node; `arrays/` fixtures pass (8); the module validates.
- 2026-10-13: fixtures: `arrays/` 8, `objects/` 16, `classes/` 12, `builtins/` 73 (1 skipped as before) pass. Next:
  the gate on a-03 (to ask).
- 2026-10-13: gate on a-03 against `fd1b5e7` and full test262 (user: yes, gate first): started.
- 2026-10-13: a-03's old `out/gate/base-src` (at `db256bc`, with stale worktree records copied over by the rsync, not real edits) refused to check out `fd1b5e7`; left untouched, the gate runs with `-o out/gate-t115` (`out/gate-t115-a03.log`).
- 2026-10-13: gate on a-03 vs `fd1b5e7` (`out/gate-t115-a03.log`): **fails**. Faster: nbody −12.2% (instr −16.1%),
  fannkuchredux −8.7% (instr −17.6%), pidigits −4.0%, spectralnorm −2.7%, mandelbrot −2.3%, octane-crypto −2.3%.
  Slower: revcomp +4.2% (instr +1.9%: added work, not placement); micro misses `o[k]` plain/array/function/Map
  +3…+4 ns, `o[k] = v` plain +2, `k in o` array +3, `hasOwnProperty` plain/Map +4/+5, `getPrototypeOf` function +2.
  test262 not started (user: only after a passing gate). Reported; waiting for the user's decision.
- 2026-10-13: user chose to investigate the misses and revcomp, then gate again.
- 2026-10-13: cause of the misses and revcomp (+1.9% instr): a non-`$array` receiver (revcomp's Buffer `line[i]`, `o[k]` on plain/function/Map) tests `$array` inline, then `get` tests it again. Fix: those misses call a new `get_other` (`get` without its array fast path); array misses still call `get`. Built.
- 2026-10-13: fixtures `arrays/` 8, `objects/` 16, `classes/` 12, `builtins/` 73 (1 skipped) pass; object-kinds.wasm validates. Laptop gate (revcomp, nbody, fannkuchredux + micro, `-r 3`) running.
- 2026-10-13: laptop gate (`out/gate-t115-laptop.log`): revcomp instr −0.30% (was +1.9%), nbody −16.2%, fannkuchredux −17.6% instr; the laptop's micro times drifted (+3…+7 ns on cases the change can't reach, `typeof v` on primitives): no verdict there. Re-gate on a-03 (`out/gate-t115b-a03.log`).
- 2026-10-13: re-gate on a-03 vs `fd1b5e7` with `get_other` (`out/gate-t115b-a03.log`): programs: nbody −12.7%
  (instr −16.2%), fannkuchredux −9.9% (−17.6%), pidigits −3.5% (instr equal), revcomp −0.4% (instr −0.3%, was
  +4.2%/+1.9%), the rest same. `o[k]` misses plain/array/function/Map +1/+2/0/+1 ns = same (were +3…+4). Left
  "slower": `typeof o` function/Map +2 ns (no element read in that loop; `typeof v` on primitives −2…−3 ns in the
  same run: placement), regex "literal prefix, match at the end" +249 ns of 8,234 (3.0%, threshold 247; three other
  regex cases faster). Reported to the user; test262 waits for their decision.
- 2026-10-13: user: run full test262 on a-03 (`out/t262-t115.log`).
- 2026-10-13: full test262 on a-03 (`out/t262-t115.log`, 1,450 s): no changes since the baseline. Done; docs updated.
