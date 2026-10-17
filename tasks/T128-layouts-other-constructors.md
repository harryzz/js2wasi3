# T128: layouts for objects whose fields another function sets (F407)

Status: done (2026-10-17) | Depends on: T109, T110, T111, T113, T117, T124 | Local (+ a-03 only after asking)

## Goal
F407 (found in T109): an object gets a layout (T109: pre-sized, keys added by slot, a shared index; T110 inline slot
reads/writes; T111 number cells; T117 shared keys/attributes) only when `new F(…)` names a `function`/base-class
constructor whose own body writes `this.k = …`. Objects whose fields are written by **another function** get no
layout and take the dictionary path:
- raytrace (Octane, Prototype.js style): `var Class = { create: function() { return function() {
  this.initialize.apply(this, arguments); } } }` (`tests/bench/octane/raytrace.js:33`); the fields are set in each
  class's `initialize: function(…) { this.x = …; … }` (e.g. line 607), called through `apply` from a constructor
  that has no `this.k =` of its own.
- deltablue: `X.superConstructor.call(this, …)` (`tests/bench/octane/deltablue.js:216, 297, 315, 345, 462`): a
  derived constructor runs its base's field writes through `call`.
- also no layout for derived-class constructors and arrow functions (F407's text).

This is a **speed** task (no output changes): raytrace is 31.8× node on a-03 (`docs/compatibility.md`), the worst
Octane ratio; deltablue 22.5×.

## Evidence so far (T124, laptop, `out/count-t124`, `perf record -e instructions:u`)
- `grow` calls per run: raytrace **11,131,976**, earley-boyer 1,750,300, regexp 908,290, deltablue 67,901 (counters
  `grow.call`, `define.layout_grow`, `define.layout_own` exist in `build --count`, T124).
- raytrace profile: `get_named` 14.8%, `find_own_named` 12.5%, `find_own` 11.5% (lookups on layout-less objects,
  ~39%), `grow` 8.1%, `set_named` 3.8%, `define` 3.8%, `get` 3.5%, `append_named` 2.3%, `alloc_object` 1.8%
  (building them, ~16%).
- splay is NOT a target (T124: ~92% wasmtime's copying collector; F427 dropped).

## Read first
- `tasks/T109-site-layouts.md` (how layouts are found: `Lowerer::this_layout`, `crates/lower/src/lib.rs:3176`;
  `Intrinsic::LayoutSet`/`LayoutDefine`, `lib.rs:7071`, `:7194`; `module.layouts`), `tasks/T117-shared-layout-shape.md`
  (shared keys/attrs, `object.rs` `alloc_layout` `:2740`, `own_shape`, `define`'s layout branch), `tasks/T110`/`T111`
  (inline slots: `crates/backend/src/lib.rs` `LAYOUT_GUESSES` `:1044`, `num_slots`, the `GetProp`/`SetProp` constant-key
  paths ~`:2525`, `:2674`), `tasks/T124-layout-slack.md` (the measurements above).
- `docs/design.md` "Speed work" (the rules below), `docs/root-causes.md` R3.
- User rule "consult proven implementations first": find what V8 (in-object slack tracking: a map's instance size is
  learned from the first allocations, then shrunk; transitions give objects built the same way the same map
  whatever function wrote the fields) and JSC (structure transitions, inferred inline capacity) ship for this case, let
  that decide the design, report it and **ask before building**.

## Plan (suggested; ask at each step marked "ask")
- [x] Count run (`scripts/bench.sh -c`): in raytrace/deltablue, how many objects are built per constructor and which
  keys each ends with (a counter or a one-off trace in `--count` builds only); confirm the `grow`/lookup numbers.
- [x] Research V8/JSC for fields written by another function (`apply`/`call` of an initializer, derived constructors);
  write the findings in Progress; propose a design (ask). Candidates to evaluate, not to assume: (a) compile-time:
  follow `this.initialize.apply(this, arguments)` / `Base.call(this, …)` in a constructor to the called function's
  `this.k =` writes (static, like T109); (b) run-time: per-allocation-site learned capacity/keys (slack tracking).
- [x] Build (after the user's yes); fixtures `objects/`, `classes/`, `builtins/`, `scripts/diff.sh`; `cargo test`.
- [x] Gate (`scripts/gate.sh <base>`, laptop, then a-03: ask); full test262 on a-03 (ask).

## Rules that apply (from `CLAUDE.md`, `docs/design.md` "Speed work" and the user's standing instructions)
- Write a dated Progress line after every step. Set the status in `tasks/README.md` to `in progress` when starting.
- Root cause first; count before designing; a miss may not cost more than the old path; a miss doesn't repeat the
  inline test; equal instructions = placement; nothing slower (laptop gate, then a-03 after asking).
- Only the standard tests and benchmarks (fixtures, test262, the 18 programs, micro-benchmarks). No zigbee2mqtt.
- Report results, don't fix after results without asking; never tune to a benchmark.
- a-03 is test-only: copy every log back to `out/` at once; docs cite the laptop copies. Ask before a-03 runs, long
  runs and installing tools. Never propose wasmtime upstream issues.
- Never commit/push without asking each time; no attribution lines (no Co-Authored-By, no session links).
- At the end: Notes/Result, `tasks/README.md` status and follow-ups (next free F is F439), tick F407, `CLAUDE.md`,
  `tests/test262/HISTORY.md` (after a full run), `docs/design.md`, `docs/root-causes.md`.

## State at handoff (2026-10-16)
- `main` = `ebe6055` (T127), pushed; working tree clean except this file and the README row.
- wasmtime 49.0.3 pinned (laptop `~/.wasmtime/bin/wasmtime`, a-03 `~/.local/bin/wasmtime`; 50.0.0-rc.1 also on a-03 as
  `wasmtime-50.0.0-rc.1`, F436 pins 50.0.0 when final). test262 baseline: 28,123 pass.
- Gate base for this task: `ebe6055`.

## Progress
- 2026-10-16: handoff written (user: "Handoff f407 for new session").
- 2026-10-16: started; status `in progress`. Count step, part 1: a one-off node trace (scratchpad `t128/trace.mjs`,
  patches the concatenated `out/count-t124/octane-{raytrace,deltablue}.js` to record own keys at the end of each
  construction; same driver and sizes as `bench.sh -c`). raytrace (all through the ONE anonymous `Class.create`
  closure body, a different closure per class): Vector {x,y,z} 1,130,814; Color {red,green,blue} 1,076,806;
  IntersectionInfo {color} 512,216 (then isHit/distance/… added later: its prototype holds 7 defaults);
  Ray {position,direction} 143,104; the rest ≤ 86 each. deltablue: OrderedCollection 1,924,038, Variable 699,021,
  Plan 11,535, Planner 4,614 (these have T109 layouts already: `layout_set.add` 717,639); WITHOUT a layout:
  StayConstraint {strength,myOutput,satisfied} 233,007, EqualityConstraint {strength,v1,v2,direction} 230,700,
  ScaleConstraint {direction,scale,offset,strength,v1,v2} 230,700, EditConstraint 11,535 (bases `Constraint`,
  `UnaryConstraint`, `BinaryConstraint` are never `new`ed, so their stores aren't counted). deltablue grows are
  few (67,901): its cost is lookups, not building.
- 2026-10-16: raytrace's 11.1M `grow`s (T124) are about half layouts, half `arguments`: `make_arguments`
  (`object.rs:5775`) allocates an empty dictionary and `define`s "0".."n-1", `length`, `callee` one by one (number
  → string each), ~2 grows per call × 2.9M `this.initialize.apply(this, arguments)` calls; objects ~1 grow each
  (0 → 4), IntersectionInfo 2. The `arguments` cost is R4 (calls through values and argument arrays), not this task.
- 2026-10-16: count step, part 2 (from T124's `out/count-t124/octane-raytrace.counts`, no new run): raytrace's adds
  are slow for a second reason: `set_named.other` 11,696,202 vs `set_named.add_cached` 653. Its prototypes hold
  defaults for the fields (`Vector.prototype = { x: 0.0, y: 0.0, z: 0.0, … }`, IntersectionInfo's 7), so every
  `this.x = …` add finds `x` on the prototype and takes `ordinary_set_named`'s chain search (T90's add cache only
  covers "absent on the chain") — the `find_own_named` 12.5%. Reads: `get_named.guess_hit` 41.0M (own, a call),
  `get_named.proto_or_absent` 11.3M (methods and prototype defaults: not layout work), inline_read hit 4.2M /
  miss 1.8M (raytrace's keys are in no static layout, so few sites try slots).
- 2026-10-16: research (V8 and JSC sources as known; what they ship for fields written by another function):
  - V8: the layout belongs to the **function object** (`JSFunction::EnsureHasInitialMap`, the initial map in the
    `prototype_or_initial_map` slot), not to the code: every `Class.create()` closure gets its own initial map,
    created at the first `new`. Its size starts from the parser's `this.x =` count (`expected_nof_properties`) plus
    slack; **in-object slack tracking** (`Map::kSlackTrackingCounterStart` = 7 constructions,
    `Map::CompleteInobjectSlackTracking`) then shrinks it to what the objects built so far actually used — fields
    added after the constructor (IntersectionInfo) included. Who writes the fields doesn't matter: each add is a
    map transition, so objects built the same way share a map. A store that adds a key found on the prototype as a
    writable data property is an ordinary transitioning store, validated by the prototype chain's validity cell.
  - JSC: the same per function object (`FunctionRareData`'s `ObjectAllocationProfile` for `this`), inline capacity
    from the bytecode's static count (`StaticPropertyAnalyzer`) and, for Prototype.js-style code,
    `ObjectAllocationProfile::possibleDefaultPropertyCount`: the prototype's non-function properties count as
    expected fields (the "defaults on the prototype" pattern raytrace uses); structure transitions do the rest.
  - Both: run-time, per function object, learned from the objects actually built; nothing follows `apply`/`call`
    statically. Candidate (a) (follow `this.initialize.apply` / `X.superConstructor.call` at build time) is
    pattern matching on two benchmarks' idioms and can't work for raytrace anyway: one constructor body (the
    `Class.create` closure) builds every class, so a per-scope layout is wrong for all but one.
- 2026-10-16: proposed design (b), asked the user before building:
  1. A learned layout per function object: `$function` gets a field (null / learning / a run-time layout: index,
     keys, attrs, as T109/T117's). `new F` for a function without a static layout: with a learned layout, allocate
     with it (count 0, its size); while learning, keep the first object and at the 8th construction take the
     layout from that object's keys then (V8's 7-construction window; sees fields added after the constructor);
     1–8 plain named writable data keys, else "none" for good.
  2. Generic adds keep the layout: `define`'s shared-index path stores at `count` when `keys[count]` is `k`
     without `own_shape`'s copy (as `layout_set`'s add branch), so learned objects keep T117's sharing.
  3. T90's add cache also caches "k is on the chain only as a writable data property" (V8's transitioning store
     with a validity cell; our `G_EPOCH`): raytrace's adds over prototype defaults stop searching the chain.
  4. Not now (count first, separate step): read slots for learned layouts (T110's `layout_slots` only knows static
     layouts).
  Expected: raytrace ~3–4M fewer grows, ~9M adds without the chain search; deltablue's constraints get layouts
  (their reads inline only after 4). `arguments` for `apply` (the other half of the grows) is R4, a follow-up.
- 2026-10-16: user: build 1+2+3. Order: 3 first (smallest, independent), gate; then 1+2, gate.
- 2026-10-16: step 3 built: `object::add_cached` (one check for `set_named` and `layout_set`): the T90 tag is the epoch (absent) or `[epoch, holder, entry]` (T128: `k` first on the chain as a writable data property of a `$plain` holder; the holder entry is re-checked: key, `count`, not accessor/read-only), filled by `ordinary_set_named`; `set_ic_store` takes the tag as a `jsval`. Counters `*.add_cached_proto`, `layout_set.add_cached`. Fixture `objects/proto-default-adds.js` (read-only/accessor/delete/freeze/moved entries/other prototypes) = node; its count build hits `set_named.add_cached_proto` 17×.
- 2026-10-16: count run after step 3 (`out/count-t128-s3`, outputs same as node): raytrace `set_named.other` 11,696,202 → 151, `set_named.add_cached_proto` 11,696,051; deltablue unchanged (no prototype defaults). Micro cases added to `property-paths.js`: add absent key, add over a plain prototype default (hit), over a function default (miss, not cached). Laptop gate vs ebe6055 started (`out/gate-t128-s3`).
- 2026-10-16: steps 1+2 written (not built yet, the gate runs on `target/release`): `$function` field `learn` (`func::F_LEARN`, null / `$vals [first, i31 n]` / `$vals [index, keys, attrs]` / `i31` none), `func::learn_layout` (8th construction: the first object, plain, 1–8 live keys, attrs 0, no own index → index built as `layout_index`), `construct_ordinary` allocates through `create_this_layout` with it; `define` stores the layout's next key at `count` keeping the shared keys/attrs (`define.layout_next`). Counters `construct.learned`, `learn.layout`, `learn.none`. Only ordinary functions without a build-time layout (`construct_ordinary`); adapters (`f$construct`) and classes not yet.
- 2026-10-17: step 3 laptop gate vs ebe6055 (`out/gate-t128-s3`, log `out/gate-t128-s3.log`): raytrace **faster**
  (instructions −3.45%); revcomp faster; pidigits and deltablue "slower" at equal instructions (+0.00%, +0.01%:
  placement); the rest same, all outputs = node. Micros: `add over proto default (plain)` 148.5 → 136.0 ns (hit),
  the function miss 173 → 171.5 (not worse), absent add same; many unrelated cases (`typeof`, `==`) +3…+5 ns. A
  rerun of `primitives` (3 alternating runs of the gate's own .cwasm) shows base = new in the first pair and both
  drifting later (laptop noise), and `perf stat` instructions equal (107,751.96M vs 107,751.95M): noise, not work.
- 2026-10-17: steps 1+2 built. Fixture `objects/learned-layouts.js` (Class.create, later fields, odd orders, deletes, accessors, freeze, returned objects, Reflect.construct, 12 keys, prototype keys added later) = node; count build: `learn.layout` 3, `learn.none` 1, `construct.learned` 49, `define.layout_next` 121. `scripts/diff.sh` objects 21/21, classes 13/13, builtins 76/76 (1 skip).
- 2026-10-17: count after steps 1+2 (`out/count-t128-s12`, outputs = node): raytrace `grow.call` 11,131,976 → 7,658,038, `construct.learned` 3,995,536, `define.layout_next` 10,557,740, `learn.layout` 13, `define.layout_own` 381,115 (objects leaving the learned order, IntersectionInfo); deltablue grows 67,901 → 22,605, `construct.learned` 45,296 (3 layouts; the count build runs fewer iterations than the node trace: Variable adds show the same ~1/7). Laptop gate (1+2+3) vs ebe6055 started: `out/gate-t128-s123`.
- 2026-10-17: laptop gate 1+2+3 vs ebe6055 (`out/gate-t128-s123`): no program slower in time; raytrace faster
  (7156 → 6968 ms min), regexredux faster; binarytrees instructions **+1.81%** (same time), micros again uniformly
  "slower" at equal instructions (property-paths 147.52G → 147.33G, primitives/object-kinds equal: the second run
  of each pair is biased, F439). raytrace's `instr` is NOT repeatable: 5 runs of the same `.cwasm` give 47.7–56.8G
  (±10%, the collector), so the gate's single `instr` sample (−2.08% vs −3.45% for step 3) says nothing for it
  (F440); binarytrees' is repeatable to 6 digits. Cause of binarytrees' +1.81%: `TreeNode` returns a literal, its
  `this` is thrown away (learned: none), but every `new` still paid `construct_ordinary`'s learning tests.
- 2026-10-17: fix, V8's way (a function's construct stub is swapped when slack tracking ends): `$function.construct`
  is mutable; ordinary functions start with `construct_learning` (create_this, call, `learn_layout`), and
  `learn_layout` switches them to `construct_ordinary` (restored to its pre-T128 body; `learn` cleared) or to
  `construct_learned` (alloc with `[index, keys, attrs]`). Shared body `construct_with`. binarytrees instructions
  58,769.98M base → 58,768.52M (equal). Fixtures objects 21/21, classes 13/13, builtins 76/76; counts unchanged.
- 2026-10-17: final laptop gate vs ebe6055 (`out/gate-t128-final`, log `out/gate-t128-final.log`), all outputs =
  node: **raytrace −3.7%** (7176 → 6911 ms min), **deltablue −2.2%**, **richards −2.4%** (faster); nothing slower;
  the rest within ±1.4%. Micros: `add over proto default (plain)` 146.0 → 134.0 ns, its miss 167.5 → 166.0, `write
  hit` −2.5; 41 cases "slower" by 2–11 ns in code T128 doesn't touch (`typeof`, `==`, `k in o`), at equal
  instructions (primitives 107,751.95M vs 107,752.11M, object-kinds equal, property-paths −0.13%).
- 2026-10-17: two gate artefacts found (not T128's work): (F439) the micro loop always runs base then new, and the
  second of each pair is slower: `primitives` with the order reversed (5 runs) gives new "faster" in 19 of 30
  cases. (F440) the `instr` column of GC-heavy programs depends on incidental heap layout: the same `.cwasm` bytes
  give binarytrees 58,768.5M from `scratchpad/btg.cwasm` and 59,218.0M from `binarytrees.newx.cwasm` (argv holds
  the path; base doesn't move), so the gate's +0.76% is collector timing; with `-O gc-heap-initial-size=256M` base
  29,349.93M = new 29,349.77M at both path lengths. raytrace's `instr` varies ±10% between identical runs.
- 2026-10-17: user: a-03 gate and full test262. Synced (rsync), built there; `scripts/gate.sh -o out/gate-t128 ebe6055` then `node scripts/test262.mjs` started in one nohup chain (logs to be copied to `out/gate-t128-a03.log`, `out/t262-t128.log`).
- 2026-10-17: a-03 gate vs ebe6055 (`out/gate-t128-a03.log`, `out/gate-t128-a03.tsv`), outputs = node: **raytrace
  −4.4%** (7003 → 6698 ms, instructions −2.66%), pidigits −2.4% (equal instructions), deltablue −1.0% (−1.03%
  instructions), nothing slower. Micros: `add over proto default (plain)` 133.5 → 125.0 ns; 4 cases +2…+4 ns in
  paths T128 doesn't change (`o.hasOwnProperty(k) [plain]`, `read miss proto`, `o[k] = v [Map]`, `write Map own x`),
  at equal instructions on a-03 (object-kinds 179,503.31M → 179,503.09M, property-paths 147,439.16M → 147,241.67M).
- 2026-10-17: the chained test262 command lacked `subset` (printed usage, nothing ran); rerun as `node scripts/test262.mjs subset` on a-03.
- 2026-10-17: test262 on a-03 (`out/t262-t128.log`): no changes, 28,123 pass. Task done.

## Notes / Result
**Done 2026-10-17.** Root R3; closes F407. Design from V8/JSC (Progress, 2026-10-16 "research"): run-time, per
function object; nothing follows `apply`/`call` at build time.
- What changed (`crates/backend`):
  - Adds over prototype defaults (step 3): `object::add_cached` (shared by `set_named` and `layout_set`) accepts
    T90's tag as the epoch (no `k` on the chain) or `[epoch, holder, entry]` (`k` first on the chain as a writable
    data property of a `$plain` holder; the entry is re-checked on every hit); `props::ordinary_set_named` fills it;
    `set_ic_store` takes the tag as a `jsval`.
  - Learned layouts (step 1): `$function` field `learn` (`func::F_LEARN`), `construct` mutable; ordinary functions
    start with `construct_learning`; `func::learn_layout` keeps the first object and at the 8th construction
    (`LEARN_AFTER`) builds `[index, keys, attrs]` from its keys (plain, 1–8 live keys, default attributes, no own
    index) and switches `construct` to `construct_learned`, else back to `construct_ordinary` (pre-T128 body). Shared
    body `construct_with`.
  - Generic adds keep a layout (step 2): `define` stores the layout's next key at `count` without unsharing.
  - Counters: `set_named.add_cached_proto`, `layout_set.add_cached{,_proto}`, `define.layout_next`,
    `construct.learned`, `learn.layout`, `learn.none`.
- Tests: fixtures `objects/proto-default-adds.js`, `objects/learned-layouts.js` (= node); micro cases in
  `tests/bench/micro/property-paths.js` (add absent key, add over a plain/function prototype default);
  `scripts/diff.sh` objects/classes/builtins pass; `cargo test --release` passes.
- Counts (`out/count-t128-s3`, `out/count-t128-s12`): raytrace `set_named.other` 11,696,202 → 151, `grow.call`
  11,131,976 → 7,658,038, 3,995,536 constructions with a learned layout; deltablue grows 67,901 → 22,605.
- Results: laptop gate vs ebe6055 (`out/gate-t128-final.log`): raytrace −3.7%, deltablue −2.2%, richards −2.4%,
  nothing slower; a-03 (`out/gate-t128-a03.log`): raytrace −4.4% (instructions −2.66%), pidigits −2.4%, deltablue
  −1.0%, nothing slower; micro `add over proto default (plain)` −8.5…−12 ns, its miss not worse; micro "slower" rows
  at equal instructions (F439 order bias). A first version tested the learning state on every `new`: binarytrees
  +1.81% instructions (its `TreeNode` returns a literal), fixed by swapping `construct` (V8's stub swap). test262
  (a-03, `out/t262-t128.log`): no changes, 28,123.
- Commands: `scripts/bench.sh -c -o out/count-t128-{s3,s12} octane-raytrace octane-deltablue`; `scripts/gate.sh -o
  out/gate-t128-{s3,s123,final} ebe6055` (laptop), `PERF=… scripts/gate.sh -o out/gate-t128 ebe6055` (a-03);
  `node scripts/test262.mjs subset` (a-03); node trace `scratchpad t128/trace.mjs` (one-off).
- Not done / left: F441 (reads of learned layouts not inline), F442 (`arguments` for `apply`: about half of
  raytrace's remaining grows, R4), F443 (build-time-layout functions whose objects outgrow it, adapters, classes
  don't learn), F444 (one key order per function), F439/F440 (gate artefacts).
- Next smallest step: F442 (raytrace's `make_arguments` + `list_from_array_like` per `initialize.apply`), or F441
  after counting.
