# T98: GC cost on the default heap (F371, F322)

Status: in progress (started 2026-10-09, cloud session; measurements done, design decision pending) | Depends on: T97

## Goal
Programs run without wasmtime's `-O gc-heap-initial-size` flag (the default, e.g. any command a user runs) spend a
large share of their time in the copying collector. zigbee2mqtt runs with 512 MiB already (F347).

## Measurements (cloud, 2026-10-09)
- GC share on the default heap (perf, 15 s per benchmark), roots (`trace_vmctx_roots`, `RegisteredType::root`,
  `from_wasm_type`: F322) vs copying (`forward`, `scan_field`, `collect_increment`, memmove):
  RayTrace 19.8% / 13.2%, DeltaBlue 7.9% / 11.7%, EarleyBoyer 2.9% / 29.0%, knucleotide 1.2% / 7.2%,
  binarytrees 0.3% / 46.9%, Splay 0.3% / 59.1%. Root scanning matters only where collections are frequent and
  the live set small; elsewhere copying dominates (inherent to the copying collector).
- wasmtime's other collector, `-C collector=drc` (compile and run): same outputs, 7–15× slower (binarytrees 8.4 →
  110 s, RayTrace score 389 → 45, EarleyBoyer 1,251 → 82, knucleotide 4.5 → 33 s; Splay equal). Not an option.
- `-O gc-heap-initial-size=256MiB`: Splay score 476 → 872, RayTrace 383 → 665, EarleyBoyer 1,169 → 1,870,
  DeltaBlue 379 → 497, binarytrees 8.1 → 5.5 s, knucleotide equal.
- A program can grow its own heap: a throwaway allocation at startup grows wasmtime's GC heap, which then stays
  large (fewer collections), no flag needed. Prepended `{ let pad = new Uint8Array(N); pad[0] = 1; }`:

  | reservation | RayTrace | DeltaBlue | EarleyBoyer | peak RSS |
  |---|---|---|---|---|
  | none | 390 | 409 | 1,177 | 20–61 MB |
  | 16 MB | 635 | 501 | 1,614 | ~50 MB |
  | 32 MB | 672 | 448 | 1,800 | ~80 MB |
  | 64 MB | 674 | 480 | 1,878 | ~140 MB |
  | 128 MB | 701 | 495 | 1,868 | ~265 MB |
  | 200 MB | 718 | — | — | ~400 MB (Splay 568 → 646) |

  hello-world: none 7 ms / ~3 MB, 16 MB 16 ms / ~30 MB, 32 MB 27 ms / ~44 MB (zero-filling and the second
  semispace).
- F322 (string constants in one array: one GC root instead of ~600–2,000 in benchmarks, ~60,000 in zigbee2mqtt)
  would cost an array load on every constant use (every `obj.x` key); zigbee2mqtt's root scanning is ~1% at
  512 MiB, so likely neutral or negative there; worth it only where collections stay frequent.

## Decision (user)
A default heap reservation emitted by the compiler (size? build option to change or disable it), or opt-in only.

## Progress
- 2026-10-09: task created (user: F371/F322 first, zigbee2mqtt is the reference). Measurements above.
