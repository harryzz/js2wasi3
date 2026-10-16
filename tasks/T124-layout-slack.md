# T124: keys added right after construction (F427)

Status: done (measured; F427 dropped) | Depends on: T109, T117 | Local (+ a-03 only after asking)

## Goal
F427 (found in T117): keys an object gets right after construction, outside its constructor (splay's
`node.left`/`node.right`, set by `insert` on every new node; the prototype gives them defaults), aren't in its
layout, so each such object grows (new keys/vals/attrs arrays) at its first extra key. splay is 19.4× node on a-03
and GC-bound. User chose F427 on 2026-10-16, starting with a count run.

## Read first
- `tasks/T109-site-layouts.md`, `tasks/T117-shared-layout-shape.md`; `crates/backend/src/object.rs` `define`,
  `grow`, `alloc_layout`, `own_shape`.
- V8's slack tracking (in-object slack, shrunk after the first allocations of a map) for the design.

## Plan
- [x] Count run: `define.layout_grow` (a full layout object adds a key: `grow`), `define.layout_own` (a layout object
  with room adds a key: own copies), `grow.call`; `scripts/bench.sh -c` on the 18 programs; report and decide.

## Progress
- 2026-10-16: task created (user: yes, start F427 with the count run).
- 2026-10-16: counters `define.layout_grow`, `define.layout_own`, `grow.call` (`--count` only). Count run
  (`out/count-t124`, laptop, 18/18 = node): a full layout object growing: splay 57,594 (one per tree node), deltablue
  21,999, crypto 5,103, else 0; all `grow` calls: raytrace 11,131,976, earley-boyer 1,750,300, regexp 908,290,
  deltablue 67,901, splay 57,889 (the rest ≤ 1.5k). Profiles (`perf record -e instructions:u`, laptop):
  splay ~92% in wasmtime's copying collector (`forward` 33%, `scan_field` 28%, `collect_increment` 16%, memmove 8%),
  `grow` not in its top 14: F427 can't pay on splay (its cost is the collector, F304; wasmtime 50 helps, T123).
  raytrace: `grow` 8.1%, `define` 3.8%, `append_named` 2.3%, `alloc_object` 1.8% (objects built without a layout
  ~16%) plus lookups on them `get_named` 14.8%, `find_own_named` 12.5%, `find_own` 11.5%: F407 (classes from
  `Class.create`, fields set in `initialize` via `this.initialize.apply(this, arguments)`: no T109 layout).
  Reported; waiting for the user's decision.
- 2026-10-16: user: close as measured (F427 doesn't pay; the target is F407); keep the counters. Also retested F134
  (wasmtime 49.0.3 / 50.0.0-rc.1, `tasks/README.md`): fixed in 50.0.0-rc.1.

## Notes / Result
- **Result:** F427 doesn't pay. splay grows one object per tree node (57,594 `grow`s per run) and spends ~92% of its
  instructions in wasmtime's copying collector; `grow` isn't in its top 14 functions. The object-growth cost is in
  objects **without** a layout: raytrace 11.1M `grow`s (`grow` 8.1% of its instructions, ~16% building objects, 39%
  lookups on them), earley-boyer 1.75M, regexp 0.9M: F407 (fields set by `initialize.apply` /
  `superConstructor.call`, no T109 layout). F427 closed as measured; F407 updated with these numbers.
- **Kept:** counters `define.layout_grow`, `define.layout_own`, `grow.call` (`build --count` only; no change
  otherwise).
- **Commands:** `scripts/bench.sh -c -o out/count-t124`; `perf record -e instructions:u` on raytrace and splay
  (laptop).
- **Next smallest step:** choose: F407 (raytrace, speed), F85 (a trap, small), F106/F108 (`extends Error`/`Array`).

