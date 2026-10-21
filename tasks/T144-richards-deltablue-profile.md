# T144: profile richards and deltablue (the largest gap left, 18× node on wasmtime)

Status: done (2026-10-21) | Depends on: T132, T133, T141 | Local (no code)

## Goal
octane-richards and octane-deltablue run 18.3× and 18.2× node on wasmtime (11.0× / 10.9× on V8; `out/v8bench-t137-a03.md`),
the two largest gaps that are our code. Find what costs the time (`build --count`, perf by helper, the hot functions'
Wasm) and report the biggest items with a proposed first task for each. No code changes.

## Read first
- `tasks/T138-fannkuch-crypto-profile.md` (the method), `tasks/T132-method-calls.md`, `tasks/T133-shaped-objects-layout.md`,
  `docs/root-causes.md`.

## Plan
- [x] Counts (`scripts/bench.sh -c`): which helpers run how often in the two programs.
- [x] perf by helper (`scripts/profile.sh`, `scripts/profile-compare.py`) and the hottest compiled functions.
- [x] Report the biggest costs, each with a smallest next task (no code).

## Notes / Result
Laptop, `out/bench-t144/` (normal build), `out/prof-t144/` (perf by function), `out/count-t144/` (`--count`). No code.
- **richards** (1.98 s, 19.6× node): `method_check` **21.3%** + `method_fn` 4.9% (T132's guard for a direct method call:
  13,895,740 checks), `loose_eq` **16.5%** + `is_nullish` 6.5% (every `x == null` / `x != null`: `this.currentTcb != null`,
  `tcb == null`, `this.queue == null`, ...), `get_named` 4.4% + `find_own_named` 3.7% (2,694,969 prototype-or-absent reads),
  `to_number` 3.0% + `box_number` 2.6% (13,697,632 `box_number` calls). The compiled functions themselves: ~25%.
- **deltablue** (3.32 s, 17.5× node): `get_named` **24.0%** + `find_own_named` **17.7%** + `lookup_named` 2.6% + `property_value`
  1.4% (22,522,658 guessed hits that still call `get_named`, 10,652,910 prototype-or-absent reads that walk the chain with
  hash lookups), `method_check` **14.7%** (14,144,775 checks), `loose_eq` 3.7%, `call_value` 3.4%; by category: property
  access 48%, JS code 37%.
- **Findings (smallest next task for each):**
  1. `x == null` / `x != null` is a call to `loose_eq` (and `is_nullish`): about 23% of richards' samples, 3.7% of
     deltablue's. Inline test of the two oddballs (`undefined`, `null`; no `document.all`): a pointer compare each.
  2. `method_check` costs about 40 ns per call (richards 13.9M checks = 21.3% of 2.73 s; deltablue 14.1M = 14.7%): ~60
     instructions with a cache lookup (`G_MCACHE`, the entry array, the receiver's keys array, prototype, epoch, the
     holder's key/function). A cheaper guard (the cached entry kept on the call site's own global, one compare on the
     receiver's keys array and one on the epoch) or an inline fast path.
  3. deltablue's property reads that miss the receiver's own keys (prototype methods and fields on `$plain` objects) go to
     `get_named` + `find_own_named`: 33M calls, ~40% of its time. T99 (a prototype-hit cache) was measured on the
     zigbee replay and dropped before the type analysis, shapes and layouts existed.
  4. `box_number` 13.7M calls in richards (T139's inline boxing: −2.7% on fannkuchredux; richards unmeasured).

## Progress
- 2026-10-21: started (user: "yes" after T143; pushed 91cf20e). Tools unchanged.
- 2026-10-21: counts, perf by function and the findings (above). Done.
