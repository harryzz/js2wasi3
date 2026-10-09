# T95: an inline cache for computed-key reads (F363, remainder)

Status: done, **no code kept** (2026-10-09: measured three designs, all net losses; reverted) | Depends on: T94 | Cloud

## Goal
After T94 the computed-key path (`o[k]` → `get` → `lookup` → `own_get` → `find_own`, `to_property_key`,
`string_hash`) is still ~14.7% of the zigbee2mqtt replay, mostly `find_own` (5.3%) and `get` (4.9%) self time.
Constant-key reads skip all that through `get_named`'s per-site cache (T74). Give computed-key reads the same.

## Plan
1. `object::Helper::GetKeyed(v, k, site)`: sites numbered with `get_named`'s (`ic_sites`, `G_IC`: no new global).
   Number keys go straight to `get` (array elements). An object receiver with a string key: the site's cached
   entry `e` is a hit when `keys[e]` is `k` itself (`ref.eq`) → `vals[e]` (accessors and built-in sentinels
   through `property_value`, as `get_named`). Miss: a key that isn't an array index → `find_own`, remembered when
   own; anything else → `get`. A hit is always right: a dictionary entry with that exact key is the object's own
   property (exotic properties, array elements, `length` of arrays/strings, are never in the dictionary).
2. Lowering: `GetProp` with a non-constant key (not a number literal) → `GetKeyed`.
3. Measure: `T94-files/keys.js`, knucleotide, `T92-files/props.js` (plain objects), an array-index loop (must not
   lose: `a[i]` takes one more call before `get`).
4. Tests (user: regression tests, test262 and a simple benchmark; no zigbee2mqtt replay): fixtures, full
   `diff.sh` once, `cargo test`, validate, smoke; full test262 on a-03.

## Acceptance
- [x] keys.js / plain objects / array loops / programs before/after in Notes; results = node (all variants).
- [ ] test262: not needed (no code kept).
- [x] Docs (design.md note), follow-ups (F363 closed with the findings).

## Notes / Result
**Outcome: no code kept.** Three designs of a per-site inline cache for computed-key reads, each measured against
the T94 build (cloud, alternating runs; outputs always the same as node):

| design | stable key (`T94-files/keys.js stable`) | varying keys (own/proto/miss/rw) | array code | programs |
|---|---|---|---|---|
| 1. `get_keyed(v, k, site)` helper, per-site state (0 / −1 megamorphic / entry + 1), megamorphic checked first | 78–93 → 34–38 ms (node 37) | +5–10% | `a[i]` +12% (fannkuch, nested reads): one more call | DeltaBlue +10%/−8% (noise), Crypto −6% |
| 2. as 1, number keys (`Convert` from F64/I32, number constants) lowered to plain `get` | same | same | visibly-numeric keys fixed; untyped keys (`perm[k - i]`, sloppy fannkuch) still +10% | — |
| 3. cache inside `get` (`G_SITE` set by the site, read and reset at `get`'s entry) | 74 → 65 ms | +6–14% | fannkuch +7% | Crypto −6%, DeltaBlue −7%, RayTrace = |

Why: most computed-key sites see varying keys (loops over keys, generic helpers), where a one-entry cache keyed on
the key string never hits and any per-site check is overhead; on this path a few extra instructions or one extra
call cost 5–10%. Stable-key sites gain a lot (design 1: 2.3×) but are the minority in the benchmarks, and there is
no replay evidence that zigbee2mqtt differs. A cache that helps varying keys would need the object's layout as
the cache key (shapes/hidden classes: reverted in T83–T86) rather than the key string.

Kept: `T94-files/keys.js` (+ `stable` case) and `T94-files/arrays.js` (array reads with computed indexes) as
measurement tools; this note; F363 closed with the findings.

## Progress
- 2026-10-09: task created (user: "finish F363 … only regression tests, t262 and some simple benchmark, no 1 hour
  zigbee2mqtt test"). Design above.
- 2026-10-09: first version (fill the site's entry on every own-hit miss): validates, outputs same; array indexing
  unchanged (index 667–723 vs 694–883 ms). But keys.js (32 keys rotating per site) got slower: proto 168–254 →
  287–334 ms, miss 135–145 → 212–314, own no better: a one-entry cache keyed on the key string misses every time,
  and the miss path did index check + `find_own` + the generic `get` (which repeats the lookup). Next: fill a
  site only on its first access (`e + 1` stored, 0 = empty), so a rotating site's miss is the check + `get`; add a
  stable-key case (a `get(o, k)` helper called with the same key) to keys.js.
- 2026-10-09: fill-once version + `stable` case: stable 78–95 → 38–47 ms (node 37), but rotating sites still +10–25%
  (own 117–130 → 144–167): the prologue (i31/object/string tests, cache checks) and the extra call cost ~15 ns per
  miss. Next: per-site state checked first (0 empty, −1 megamorphic, e + 1 entry); a mismatch, or a first access
  that isn't an own hit, makes the site megamorphic: then only the state check and a tail call before `get`.
- 2026-10-09: adaptive per-site state (−1 megamorphic, checked first): stable 78–93 → 34–38 ms (node 37);
  megamorphic sites ~5–10% slower on keys.js. `return_call` vs `call` + `return`: same (kept `call`). Whole
  programs (3 rounds, T94 → T95): Octane DeltaBlue score 401 → 441, RayTrace 394 → 412, Splay 849 → 875, Richards/
  EarleyBoyer/Navier-Stokes/RegExp equal, Crypto 1,092 → 1,030; fannkuch(9) 575 → 605 ms, spectralnorm(300) 1,149 →
  1,207 ms: `a[i]` with a number key now goes through `get_keyed` then `get` (one more call). Next: `get`'s
  array-element fast path copied to the top of `get_keyed`.
- 2026-10-09: design 2 (number keys to plain `get`): nested reads fixed, fannkuch still +10% (untyped keys).
  Design 3 (cache inside `get` via `G_SITE`): stable −12% only, varying keys +6–14%, fannkuch +7%, Crypto −6%,
  DeltaBlue −7%. Conclusion: no design pays; reverted all code (git checkout crates/). Done, no code kept.
