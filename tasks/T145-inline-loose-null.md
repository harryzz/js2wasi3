# T145: `x == null` and `x != null` inline (F480)

Status: done (2026-10-21) | Depends on: T144 | Local (+ a-03 for the gate and test262, after asking)

## Goal
T144: every `x == null` / `x != null` (and `== undefined`) calls `loose_eq`, which calls `is_nullish`: richards spends
23% of its samples there (`this.currentTcb != null`, `tcb == null`, `this.queue == null`), deltablue 3.7%. Loose equality
with `null` or `undefined` is "x is `null` or `undefined`": two pointer compares against the oddball singletons.

## Read first
- `tasks/T144-richards-deltablue-profile.md` (Notes / Result), `crates/backend/src/lib.rs` (`Expr::Binary` with
  `eq_const`, `strict_eq_const`), `crates/backend/src/runtime.rs` (`is_nullish`, `loose_eq`).

## Plan
- [x] Backend: a `LooseEq`/`LooseNe` on `jsval` with a `null`/`undefined` constant operand → `ref.eq` against both
  oddball globals (no call).
- [x] Fixtures (every value kind against `== null`/`!= null`/`== undefined`, both operand orders, `??`-like patterns);
  laptop instructions and size; gate on a-03 (ask); test262 (ask).

## Notes / Result
- **What changed:** `x == null`, `x != null`, `x == undefined`, `x != undefined` (either operand order, `BinOp::LooseEq`/`LooseNe`
  on `jsval` with a `null`/`undefined` constant) compile to `ref.eq` against the two oddball singletons instead of calling
  `loose_eq` (which called `is_nullish`). `is_nullish_const` in the backend; no IR or lowering change.
- **Result:** a-03 gate vs 3a99fbc: geomean of the 18 programs **0.9864** (minimum times); octane-richards 2,139 → 1,771 ms
  (**−17.2%**, instructions −18.97%); crypto −3.1% at −0.20% instructions (placement); deltablue −0.6%; the other programs
  within ±1.6% or byte-identical; micros: every `v == null` case 28 → 5 ns; `.wasm` +0.00…+0.06%. test262 on a-03
  (49.0.3, `out/t262-t145.log`, 1,535 s): **no changes**, 28,123 of 28,984 run pass (97.0%).
- **Commands:** `scripts/bench.sh` + `perf stat -e instructions:u` (laptop), `scripts/diff.sh -j 4`,
  `scripts/gate.sh -o out/gate-t145 3a99fbc` (a-03), `node scripts/test262.mjs subset` (a-03).
- **Not done (follow-ups):** richards' other costs: `method_check` (F481), `box_number` calls (13.7M), `to_number`;
  deltablue's prototype reads (F482).

## Progress
- 2026-10-21: started (user: "yes" after T144; pushed 3a99fbc). Tools unchanged.
- 2026-10-21: built: `Expr::Binary` `LooseEq`/`LooseNe` on `jsval` with a `null`/`undefined` constant operand (either side) →
  `ref.eq` against `G_NULL` and `G_UNDEFINED` (`is_nullish_const`; no `loose_eq`/`is_nullish` call). Laptop (`out/bench-t145/`):
  richards instructions 21.25 G → 17.22 G (**−19.0%**), deltablue 26.44 G → 26.38 G (−0.2%); `.wasm` richards +114 bytes
  (+0.03%), deltablue +84; outputs = node. Fixture `tests/fixtures/syntax/loose-null.js` (27 value kinds × both orders,
  conditions, loops, fields, defaults, evaluation order) = node.
- 2026-10-21: all fixtures (`scripts/diff.sh -j 4`): 279 passed, 2 failed (commonjs/ws, node/fetch: timeouts under load, both pass alone), 19 skipped. Next: the a-03 gate (ask).
- 2026-10-21: user: yes. Synced; `scripts/gate.sh -o out/gate-t145 3a99fbc` started on a-03 (`~/gate-t145.log`).
- 2026-10-21: a-03 gate vs 3a99fbc (`out/gate-t145-a03.log`, `out/gate-t145/gate.tsv`): geomean **0.9864** (minimum times);
  octane-richards 2,139 → 1,771 ms (**−17.2%**, instructions −18.97%); crypto −3.1% at −0.20% instructions (placement);
  deltablue −0.6% (−0.22% instructions); the other 14 within ±1.6% or byte-identical (8); outputs = node. Micros: all `v == null`
  cases **28 → 5 ns** (every value kind, 6/6 pairs). `.wasm` +0.00…+0.06% (richards +114 bytes). Next: full test262 (ask).
- 2026-10-21: user: yes to test262. `node scripts/test262.mjs subset` started on a-03 (`~/t262-t145.log`).
- 2026-10-21: test262 on a-03 (wasmtime 49.0.3, `out/t262-t145.log`, 1,535 s): no changes (28,123). Task done.
