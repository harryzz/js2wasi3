# T98: GC cost on the default heap (F371, F322)

Status: done (2026-10-09, cloud session; full test262 on a-03: 0 regressions, 6 timeouts now pass) | Depends on: T97

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

## Decision (user, 2026-10-09)
A 16 MiB reservation by default, with a build option to change or disable it.

## Notes / Result
**What changed**: `build --gc-reserve=<MiB>` (default 16, 0–1024; `crates/js2wasi3/src/main.rs`) sets
`ir::Module::gc_reserve_mib`; the start function (the last `consts$init` chunk, `crates/backend/src/lib.rs`) first
does `i32.const (MiB << 20); array.new_default $bytes; drop`: wasmtime's heap grows once and stays, so later
collections are rarer. A module without string constants has no start function and no reservation (not expected in
practice: the prelude has constants). `wasmtime -O gc-heap-initial-size` still works on top. README updated.

**Measurements** (cloud, 2 rounds each, `--gc-reserve=0` → default 16): RayTrace score 380–389 → 638–696 (+73%),
EarleyBoyer 1,197–1,255 → 1,575–1,583 (+28%), DeltaBlue 345–411 → 431–512 (+25%), Splay 760–781 → 863–891
(+13%); peak RSS small programs 20 → 51 MB, big ones unchanged (EarleyBoyer 61 → 53 MB, Splay 531 → 533 MB).
hello-world: 7 → 15 ms.

**Commands**: full `scripts/diff.sh -j 4` (builds with the default): 248 pass, 19 skip, 2 known sandbox failures;
`cargo test -p js2wasi3-backend` ok; `cargo test -p js2wasi3` 30 pass, 4 `wac` (tool missing); validate + smoke.

**Not done**: F322 (constants in one array): fewer roots, but a load on every constant use; with the reservation
collections are rarer, so its gain shrinks; left open with these numbers. The copying collector's cost per collection
(copying the live set) stays: only a bigger heap reduces it. 

**a-03**: full test262: 6 changes, 0 regressions: six timeouts now pass (Array every/filter/forEach/some, decodeURI,
decodeURIComponent stress tests: GC-bound, now in time).

**Next smallest step**: none required; a zigbee2mqtt replay isn't needed (it runs with 512 MiB already; the
reservation is then inside that heap).

## Progress
- 2026-10-09: task created (user: F371/F322 first, zigbee2mqtt is the reference). Measurements above.
- 2026-10-09: user chose 16 MiB default + flag. Implemented `--gc-reserve` (ir field, start-function allocation).
  RayTrace +73%, EarleyBoyer +28%, DeltaBlue +25%, Splay +13%; hello 7 → 15 ms, small-program RSS 20 → 51 MB. Full
  suite 248 pass; cargo test ok (wac missing). README, design.md, F371 ticked, F322 note, F385. Done.
- 2026-10-09: a-03 (user): full test262 6 changes, 0 regressions (6 timeouts → pass); HISTORY.md; F385 ticked.
