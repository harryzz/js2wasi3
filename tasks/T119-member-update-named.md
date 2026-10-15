# T119: `o.k++` / `o.k--` through the named-property paths (F428)

Status: done | Depends on: T74, T105, T110, T111 | Local (+ a-03 only after asking)

## Goal
F428 (found in T118's count run): `member_update` (`crates/lower`) puts the member key in a `key` local even when
it's a constant string, so `o.k++` / `--o.k` reach the backend as `GetProp(o, Local)` / `SetProp(o, Local, …)` and
take the generic `get` and `set_num` → `set`, not `get_named`/`set_named` (T74/T105), the layout slots (T110) and
the number cells (T111). `member_assign` (`o.k += v`) already keeps a constant key in place (`key_temp`, T74).
richards: 2.1M such writes (`this.v2++`). The user chose F428 on 2026-10-15.

## Read first
- `crates/lower/src/lib.rs` `member_update`, `key_temp`, `member_assign`.

## Plan
- [x] Count run on HEAD (`b7788d1`) and after: `scripts/bench.sh -c` (existing counters: `set.call`,
  `set_num.call`, `num_write.*`, `num_read.*`).
- [x] `member_update` uses `key_temp` (both the number and the BigInt forms).
- [x] Checks: `scripts/diff.sh` on fixtures with `o.k++` (`objects/`, `classes/`, `semantics.js`), a fixture case
  for each form (prefix/postfix, `++`/`--`, getter/setter, non-number values, private, `super`, BigInt).
- [x] Gate on a-03 against `b7788d1` (ask); test262 (ask).

## Progress
- 2026-10-15: task created (user: "ok go with F428"). `scripts/check-tools.sh`: all ok.
- 2026-10-15: count run on HEAD `b7788d1` (`out/count-t119-base`, laptop): 18/18 outputs = node.
- 2026-10-15: `member_update` takes the key through `key_temp` (as `member_assign`, T74): a constant key stays in
  place in both the number and the BigInt forms. New fixture `objects/member-update.js` (prefix/postfix, `++`/`--`,
  strings/null/undefined/booleans/arrays, missing keys, large/fractional/-0, layouts in a loop, getters/setters,
  prototype setter, getter-only/read-only/frozen/non-extensible TypeErrors, arrays' `length`, functions, Maps,
  strings, null, a receiver with side effects, private and static private, `super`, BigInt): = node. Fixtures
  `objects` 19, `classes` 12, `closures` 11 (1 skip), `syntax` 10 (1 skip), `semantics.js`,
  `builtins/bigint-operators.js`, `async/for-await-order.js` pass; validate OK.
- 2026-10-15: count run after (`out/count-t119`): 18/18 = node; only three programs change. richards: 2,126,260
  generic `set` calls (`elem_write.miss_generic`) → `num_read.i31` +2,126,260, `num_write.i31` +2,126,260
  (`set.call` 2,135,700 → 9,440); deltablue: 68,420 the same way; crypto: 234,808 → `get_named.guess_hit`
  +235,056, `set_named.guess_hit` +234,833 (`set.call` 471,301 → 236,493). The other 15: identical counters.
  Next: laptop gate on the three, then a-03 (ask).
- 2026-10-15: laptop gate vs `b7788d1` on the three (`out/gate-t119-laptop.log`): richards 3,053 → 2,760 ms (−9.6%,
  instructions −8.74%), deltablue +0.7% (−0.22%), crypto −0.6% (−0.38%): same. The micro modules are
  byte-identical to base (no member `++` in them), so their moves (+2…+16 ns on laptop) are noise.
  Next: gate on a-03 and test262 (ask).
- 2026-10-15: user: yes. a-03 gate vs `b7788d1` (`~/gate-t119.log`), then full test262 (`~/t262-t119.log`): started.
- 2026-10-15: a-03 gate (`out/gate-t119-a03.log`): richards −9.4% (instr −8.74%), the rest same or identical; micro
  modules identical to base. test262: no changes, 31,461 tests in 1,515 s (`out/t262-t119.log`). Task done.

## Notes / Result
- **What changed.** `member_update` (`crates/lower`) takes the key through `key_temp`, as `member_assign` does
  (T74): a constant key stays in place, so `o.k++`/`--o.k` reach the backend as `GetProp(o, "k")` /
  `SetProp(o, "k", …)` and take the named paths (`get_named`/`set_named` guesses, T110 slots, T111 number
  reads/writes). Both forms (number, BigInt). A computed key still goes into a `key` local (evaluated once).
- **Counts** (`out/count-t119-base` → `out/count-t119`): richards 2,126,260 generic `set` calls → inline number
  reads/writes (`num_read.i31`, `num_write.i31`); deltablue 68,420 the same; crypto 234,808 → `get_named`/
  `set_named` guess hits. The other 15 programs: identical counters.
- **Result (a-03, `out/gate-t119-a03.log`, vs `b7788d1`):** richards 3,010 → 2,727 ms (−9.4%, instructions
  −8.74%); crypto −1.6% (−0.39%), deltablue +0.4% (−0.22%), knucleotide/regexredux/revcomp/earley-boyer ±0.8%
  at equal instructions (their `.wasm` changed by rarely-run `o.k++` sites): same; 11 programs byte-identical.
  Micro: all four modules byte-identical to base; four 2k-op regex cases marked slower there are noise.
  Laptop: richards −9.6% (`out/gate-t119-laptop.log`).
- **test262** (a-03, `out/t262-t119.log`, 1,515 s): no changes since the baseline (28,105 pass, 97.0%).
- **Commands:** `scripts/bench.sh -c` twice; `scripts/diff.sh` on `tests/fixtures/{objects,classes,closures,syntax}`,
  `semantics.js`, `builtins/bigint-operators.js`, `async/for-await-order.js` (new `objects/member-update.js`);
  `wasm-tools validate`; `scripts/gate.sh -o out/gate-t119 b7788d1` (laptop on the three changed programs, a-03
  full); `node scripts/test262.mjs subset` (a-03).
- **Not done:** nothing new deferred.
- **Next smallest step:** choose among F429 (typed-array keys `1.5`/`-0`, small correctness fix), F420, F427, F404.

