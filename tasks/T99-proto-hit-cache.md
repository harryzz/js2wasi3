# T99: a prototype-hit cache for constant-key reads (F359)

Status: in progress (started 2026-10-09, cloud session) | Depends on: T98 | Cloud (+ a-03: test262, replay)

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
- [ ] Before/after in Notes; results = node; test262 0 regressions.
- [ ] Docs, follow-ups.

## Notes / Result
(to be written)

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
