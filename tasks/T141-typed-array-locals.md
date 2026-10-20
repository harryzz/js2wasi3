# T141: a local that always holds one array literal keeps its receiver typed (F476)

Status: done (2026-10-20) | Depends on: T138, T140 | Local (+ a-03 for the gate and test262, after asking)

## Goal
T140's probe: on fannkuchredux the receiver typed once (no `ref.test` + `ref.cast` on every element access) is the
largest single step, 2,721 → 1,581 ms on wasmtime (−42%) on top of `i32` counters. A local whose only write is one
array literal (`const perm = [n]`) is always an `$array` (or null before its initialization): declare its Wasm local
`(ref null $array)`, cast once at the write, and let element reads and writes use it without the type test and cast.
No new number representation.

## Read first
- `tasks/T140-int-locals-probe.md` (Notes / Result), `tasks/T138-fannkuch-crypto-profile.md`.
- `crates/backend/src/lib.rs`: `function`, `elem_get`, `elem_num_key`, `elem_set_split`, `Stmt::Set`, `Expr::Tee`.

## Plan
- [x] Backend: `array_locals` (a `jsval` local, not a parameter, with exactly one write, an `Expr::Array`, and no
  other write except `Hole`); its Wasm local is `(ref null $array)`; the write casts (nullable) once.
- [x] `elem_get`, `elem_num_key`, `elem_set_split`: a bare `Expr::Local` receiver of such a local skips the type
  test/cast (a null test stays, null = TDZ: the miss path); only when the key and value contain no `Seq`/`Tee`.
- [x] Fixtures (TDZ, closures capturing, reassign, loops, destructuring, spread literal); counts and size on the
  laptop; gate on a-03 (ask); test262 (ask).

## Notes / Result
- **What changed:** `array_locals` (backend): a `jsval` local that is not a parameter, whose every write is an array
  literal, HOLE or a copy of another such local (the lowering's `a[i] = v` temporaries), becomes a Wasm local of type
  `(ref null $array)`; each write that isn't a copy casts once. `elem_get`, `elem_num_key` and `elem_set_split` read
  such a receiver without the type test and cast (a null test stays: not yet initialized), when no operand writes
  that local. No new representation, no IR change.
- **Result:** a-03 gate vs f0b5416: geomean of the 18 programs **0.9881** (minimum times); fannkuchredux 3,906 →
  3,275 ms (**−16.2%**, instructions −19.35%); spectralnorm −5.4% and mandelbrot −3.3% at equal instructions
  (placement); pidigits +3.2% at equal instructions (placement); the other 13 within ±1.4%; `.wasm` +0.01…+0.03%.
  test262 on a-03 (`out/t262-t141.log`, 1,586 s): **no changes**, 28,123 of 28,984 run pass (97.0%).
- **Against the probe (T140):** typing the receiver alone gave −19% instructions on the real program; the probe's
  −42% was after `i32` counters. The rest of the probe's gain needs `i32` locals (F475) and an `i32` element store.
- **Commands:** `build --core` + `wasm-tools print` (the type tests in `fannkuch`: 9 → 0), `scripts/diff.sh -j 4`,
  `scripts/gate.sh -o out/gate-t141 f0b5416` (a-03), `node scripts/test262.mjs subset` (a-03).
- **Not done (follow-ups):** receivers that are not array-literal locals but known arrays (crypto's
  `var this_array = this.array`: a property read, F478); `i32` element store and `i32` locals (F475).

## Progress
- 2026-10-20: started (user: "yes" after T140; pushed f0b5416). Tools unchanged.
- 2026-10-20: built. `array_locals` (backend): a `jsval` local, not a parameter, every write an `Expr::Array`, HOLE or a
  read of another such local (the lowering's `a[i] = v` temporaries copy the receiver into a temp: without this the
  typed path hit 4 of 9 accesses in fannkuchredux); fixpoint over copies. Typed Wasm local `(ref null $array)`, a
  nullable cast at each write that isn't a copy. `elem_get`, `elem_num_key`, `elem_set_split` use it when the receiver
  is a bare `Local` and no operand writes it (`Tee`/`Seq` setting that local; the temporaries' `Tee`s don't count).
  fannkuchredux listing: `ref.test`/`ref.cast` of the array type 9 → 0 and `br_on_cast_fail` 8 → 0 in `fannkuch`.
- 2026-10-20: laptop (instructions, same work): fannkuchredux 38.48 G → 31.04 G (**−19.3%**), `.wasm` 197,993 →
  198,044 bytes (+0.03%); crypto has no array-literal locals on its hot path (`this.array` is a property read), `.wasm`
  +73 bytes. Laptop times are unreliable today (load): node 0.31 → 0.50 s between runs.
- 2026-10-20: new fixture `tests/fixtures/arrays/array-locals.js` (reassign inside the key, aliases, mixed
  array/string/object values, frozen/sealed arrays, captured, generators/async, holes, order of evaluation) = node.
- 2026-10-20: all fixtures (`scripts/diff.sh -j 4`, laptop load average ~10): 272 passed, 6 failed (timers-more, ws, fetch,
  http, http-server, stream-async, stream-flow: timeouts/ordering under load), 19 skipped; the failed ones pass alone.
  Next: the a-03 gate (ask).
- 2026-10-20: user: yes to the a-03 gate. Synced; `scripts/gate.sh -o out/gate-t141 f0b5416` started on a-03 (`~/gate-t141.log`).
- 2026-10-20: a-03 gate vs f0b5416 (`out/gate-t141-a03.log`, `out/gate-t141/gate.tsv`): geomean **0.9881** (minimum times);
  fannkuchredux 3,906 → 3,275 ms (**−16.2%**, instructions −19.35%); spectralnorm −5.4% and mandelbrot −3.3% at 0.00%
  instructions (placement); pidigits **+3.2% slower at −0.00% instructions** (placement); the other 13 within ±1.4%;
  outputs = node. `.wasm` +0.01…+0.03% (fannkuchredux +51 bytes; earley-boyer −4). Micros: 3 of ~190 "slower":
  `v == null` [null] and [undefined] +2 ns (29 → 31, 5/0 and 6/0 pairs) and the lazy `<.*?>` regex replace +2 µs (5/0);
  none of those paths is touched by this change. Next: full test262 on a-03 (ask).
- 2026-10-20: user: yes to test262. `node scripts/test262.mjs subset` started on a-03 (`~/t262-t141.log`).
- 2026-10-20: test262 on a-03 (`out/t262-t141.log`, 1,586 s): no changes (28,123). Task done.
