# T99: a prototype-hit cache for constant-key reads (F359)

Status: done, no code kept (2026-10-10: slower on the zigbee2mqtt replay; fixture kept) | Depends on: T98 | Cloud (+ a-03: test262, replay)

## Goal
A constant-key read that finds `k` on a prototype (a method call `o.m()`, `counts.get`) misses `get_named`'s
own-entry cache (T74): after the own lookup it walks the chain with `lookup_named`, probing each level. zigbee2mqtt
(T93 call graph): `find_own_named` 4.6%, of which 2.3% from `lookup_named` (the prototype levels), plus
`lookup_named` 0.5%: at most ~2.5–3% to gain there; knucleotide's `counts.get`/`counts.set` and method-heavy Octane
more.

## Plan
1. `G_PIC` (`(ref null $vals)`, 4 slots per `get_named` site, grown on demand from 16 sites): the receiver's
   prototype `P`, the holder `H`, the `G_EPOCH` tag, the entry `e`.
2. `lookup_named_cached(p, k, hash, site)` replaces `lookup_named` in `get_named`'s own-miss path: a hit (`p == P`,
   tag == `G_EPOCH`, `H.keys[e] == k`) returns `H.vals[e]`; a miss walks as `lookup_named`, and on a hit fills the
   site after `mark_chain(p)` (T90: a key added to a flagged object, or a new prototype of one, bumps `G_EPOCH`).
   Deleting `k` from `H` fails the key check; the receiver's own `k` is still looked up first.
3. Measure: `T92-files/meth.js` (methods), knucleotide, `T92-files/props.js` (own hits must not change), Octane
   Richards/DeltaBlue/RayTrace/EarleyBoyer. Tests: prototype/class fixtures, a new fixture for invalidation (method
   replaced on the prototype, shadowed on an intermediate prototype, deleted, `setPrototypeOf`), full `diff.sh`,
   `cargo test`, validate, smoke; test262 + one replay on a-03.

## Acceptance
- [x] Before/after in Notes; results = node; test262 0 regressions.
- [x] Docs, follow-ups.

## Notes / Result
- Built `G_PIC` + `lookup_named_cached` (commit 0f86f8f) and **reverted the code** (2026-10-10): the cache does
  not pay on the reference.
- Benchmarks (cloud, 5 rounds avg, ±5% noise): Richards 351 → 362, DeltaBlue 454 → 461, RayTrace 641 → 630,
  EarleyBoyer 1,455 → 1,503, knucleotide 4,132 → 4,254 ms: neutral.
- a-03, T99 build: test262 full run **no changes since the baseline**; zigbee2mqtt replay **20.64 s** vs T97's
  20.28 s (+0.36 s, +1.8%, above the ~1% replay noise; 37.66 vs 37.01 ms CPU/event; output identical: 13,478
  publishes, 9 errors, 139 exact, 1 unmatched). The rule set before the run (≤ 20.0 keep, 20.1–20.5 revert,
  slower revert) → revert.
- Why it loses (likely, not profiled): the prototype hit was at most ~2.5–3% of the replay (T93 call graph), and
  the cache adds work on every own-miss: the site lookup, epoch and key checks, and on fills `mark_chain`, which
  flags more objects `IS_PROTO`, so more key adds bump `G_EPOCH` and invalidate T90's add caches and this one.
- Kept: `tests/fixtures/objects/proto-cache.js` (prototype replace/shadow/delete/`setPrototypeOf`/getter cases)
  as a regression fixture; it passes with the reverted code.
- Next smallest step: none on this path. A prototype hit would need to be cheaper than one chain probe without
  flagging new prototypes, i.e. shapes (F359 stays parked, branch `shapes`). Open: F322 (GC heap).

## Progress
- 2026-10-09: task created (user: docs, then F359). Re-estimated the gain from the T93 call graph: ~2.5–3% on
  zigbee2mqtt at most (the receiver's own lookup stays).
- 2026-10-09: implemented (G_PIC, lookup_named_cached; the string path keeps lookup_named). Validates. Fixture
  objects/proto-cache.js (replace, shadow on intermediate/instance, delete, Object.prototype patch, setPrototypeOf of
  receiver and of an intermediate, getter↔data, mixed receivers) = node. Measured vs HEAD (4 rounds avg): DeltaBlue
  430 → 485, EarleyBoyer 1,332 → 1,461, knucleotide 4.48 → 4.15 s, but Richards 358 → 344, RayTrace 638 → 607;
  meth.js (method one level up, 2-key prototype) 455 → 464 ms. Suspect polymorphic sites refilling on every
  mismatch (walk + mark_chain + stores). Next: refill only an empty or stale (old epoch) entry.
- 2026-10-09: refill only an empty or stale entry. 5 rounds avg vs HEAD: Richards 351 → 362, DeltaBlue 454 → 461,
  RayTrace 641 → 630, EarleyBoyer 1,455 → 1,503, knucleotide 4,132 → 4,254 ms: all within this machine's ±5% noise,
  as the small share predicts. Full suite 249 pass (2 known sandbox failures); cargo test ok (wac missing).
  Committed as in progress: the zigbee2mqtt replay (the reference, ~1% noise) decides keep or revert; test262 in
  the same a-03 run.
- 2026-10-10: a-03 results: test262 no changes; replay 20.64 s (T97 20.28 s, +1.8%). Reverted object.rs and
  runtime.rs to faa2cf6; release build ok; objects/proto-cache.js passes. Task closed, no code kept.
