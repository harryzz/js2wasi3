# T140: probe, what integer locals and a cheaper array access could gain on fannkuchredux (F475, F476)

Status: done (2026-10-20) | Depends on: T138 | Local (probe only; no compiler change)

## Goal
T138 found integers are `f64` everywhere: fannkuchredux's loop counters convert `f64 → i32 → f64` on every array
access and the array access is ~37 Wasm instructions. Before building integer locals (F475) and cheaper array
access (F476), bound the gain with hand-written Wasm (as T000's probes did): the same algorithm in four models of
our generated code, run on wasmtime 49.0.3 (and V8 for reference). No compiler change.

## Read first
- `tasks/T138-fannkuch-crypto-profile.md` (Notes / Result), `tasks/T000-general-speed-analysis.md` (the earlier probes).
- `probes/layout.wat`, `probes/layout.mjs`.

## Plan
- [x] `probes/fannkuch-gen.py` writes three WAT files, same algorithm (the benchmark's `fannkuch(10)`):
  M0 today's shape (`f64` counters, key converted per access, receiver tested per access, generic `i31` store, the
  write's nine tests, `box_number` call), M1 `i32` counters with overflow checks and the same generic store,
  M2 M1 + receiver cast hoisted + an `i32` element store (bounds and flags test only).
- [x] Outputs equal node's (`checksum 73196`, `flips 38`); wasmtime and V8 times, minimums of 5.
- [x] Report the gain of M1 and M2 over M0 and compare M0 with our compiler's 3.6–3.8 s.

## Notes / Result
`probes/fannkuch-gen.py` writes `probes/fannkuch-m{0,1,2,3}.wat` (the benchmark's `fannkuch(10)`; result 73196 / 38 as
node in every model); `probes/fannkuch-run.sh` times them on wasmtime 49.0.3 (precompiled, minimum of 5, laptop),
`probes/fannkuch.mjs` on node's V8 (minimum of 5). No compiler change.

| model | what it is | wasmtime ms | vs M0 | V8 ms |
|---|---|---|---|---|
| M0 | today's shape: `f64` counters, key converted per access, receiver tested+cast per access, generic `i31` store with the write's nine tests, `box_number` call per written number | 3,736 | | 2,115 |
| M1 | `i32` counters (checked arithmetic), same generic store, value written as an inline `i31` | 2,721 | −27% | 663 |
| M3 | M1 + the receiver typed once (no per-access test/cast) | 1,581 | −58% | 669 |
| M2 | M3 + an `i32` element store (read: null + bounds test; write: flags + null + bounds test) | 1,191 | **−68% (3.1×)** | 424 |

- **M0 is a fair model:** 3,736 ms against our compiler's 3.6–3.8 s for the real program (laptop, `out/bench-t138/`);
  node runs the JS in ~380 ms (V8 on M2: 424 ms).
- **Where the gain comes from (wasmtime):** `i32` counters and inline `i31` boxing −1,015 ms; the receiver typed once
  −1,140 ms (the largest single step); the `i32` store −390 ms.
- **What the probe assumes (not shown to be derivable by the compiler):** (a) loop counters and the values read from
  these arrays are provably int32 (here an overflow stands in as a trap; a real version needs a fallback path and its
  code size); (b) a `const` local holding an array literal stays an array (true in JS), while its store mode
  (generic / number / `i32`) can change, so M3's gain needs only (b) and M2's needs a mode test (the null test);
  (c) one program: crypto's `am3` is property reads (types for prototype methods), not covered.
- **Result for the plan:** the receiver typed once for `const` array locals (F476) is the best first step (−42% of
  M1's time on wasmtime, no new number representation), then `i32` locals (F475), then an `i32` element store.

## Progress
- 2026-10-20: started (user: "push and make probe"). Pushed 73b6eb2.
- 2026-10-20: generator, four models, runs (above). Done.
