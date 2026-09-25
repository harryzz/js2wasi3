# T01: Pin the toolchain and prove the features

Status: **done** (2026-09-25) | Depends on: -

## Goal
Pin every tool and prove with small hand-written probes that GC, EH and P3 async (including in components) work on the pinned host.

## Result
- Tools updated and pinned in `TOOLS.md`. `scripts/check-tools.sh` checks them.
- `probes/run.sh`: (1) GC + EH + async export; (2) P3 command writing stdout through a stream and awaiting a timer via the callback loop;
  (3) the GC ABI option handles scalars only; (4) stack switching with EH+GC; (5) a continuation parked across WAIT;
  (6) CM threads; (7) switch cost: CM threads ~0.75 µs, stack switching ~0.03 µs.
- These results led to decisions D1–D3 in `docs/design.md`.
