<!-- T69: DRAFT for review; not posted. Target: https://github.com/bytecodealliance/wasmtime/issues (new issue). -->

# Copying collector: grow-vs-collect threshold of `capacity / 2` lets the live set fill the whole active semi-space, so collections free almost nothing

## Summary

With the copying collector, `should_collect_first` (crates/wasmtime/src/runtime/store/gc.rs) chooses to collect
rather than grow while

```rust
predicted_usage < gc_heap_capacity / 2
```

where `predicted_usage = last_post_gc_allocated_bytes + bytes_needed`. For the copying collector
`gc_heap_capacity` is the size of both semi-spaces, so `capacity / 2` is the *entire* active semi-space. The heap
therefore only grows once the live set alone fills the active semi-space. Until then, when the live set is just
below a semi-space, every collection copies (almost) the full semi-space and frees only what was allocated since the
last one, a few KiB, so the mutator does a full copy every few KiB of allocation.

The 2× live-set bound discussed in #13470 / #12942 holds for the non-moving collectors, but for the copying
collector the same ratio gives a ~1× bound on the active semi-space, the point where throughput collapses.

This is on 49.0.1; the heuristic and `CopyingHeap::capacity()` look the same on `main` (3be9345).

## Reproducer

Core module, no host code: a live list grows to 29,000 nodes of 1 KiB (~30 MiB, just under the 32 MiB semi-space of
a 64 MiB heap), then only short-lived structs are allocated.

```wat
(module
  (type $bytes (array (mut i8)))
  (type $node (struct (field $next (ref null $node)) (field $data (ref $bytes))))
  (type $tmp (struct (field i32) (field i32)))
  (global $live (mut (ref null $node)) (ref.null $node))
  (global $sink (mut (ref null $tmp)) (ref.null $tmp))
  (func (export "run") (result i32)
    (local $i i32) (local $n i32)
    (loop $l
      (global.set $sink (struct.new $tmp (local.get $i) (local.get $i)))
      (if (i32.and (i32.eqz (i32.and (local.get $i) (i32.const 63)))
                   (i32.lt_u (local.get $n) (i32.const 29000)))
        (then
          (global.set $live (struct.new $node (global.get $live) (array.new_default $bytes (i32.const 1024))))
          (local.set $n (i32.add (local.get $n) (i32.const 1)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $l (i32.lt_u (local.get $i) (i32.const 8000000))))
    (local.get $n)))
```

```
wasm-tools parse gc-thrash.wat -o gc-thrash.wasm
wasmtime run -W gc=y,function-references=y --invoke run gc-thrash.wasm
wasmtime run -W gc=y,function-references=y -O gc-heap-initial-size=536870912 --invoke run gc-thrash.wasm
```

| wasmtime 49.0.1, x86_64 Linux, copying (default) | wall | peak RSS | collections |
|-|-|-|-|
| default | 1,837 ms | 87 MB | 249 |
| `-O gc-heap-initial-size=536870912` | 284 ms | 289 MB | 1 |

With `WASMTIME_LOG=wasmtime::runtime::vm::gc=trace`, every collection in the default run ends with
`After collection, GC heap's allocated bytes = 0x1e17230 bytes` (the 30 MiB live set) in a 64 MiB heap: the heap
never grows to 128 MiB, because `0x1e17230 + bytes_needed < 64 MiB / 2` holds for every small allocation.

## In a real program

We hit this compiling a JavaScript application (zigbee2mqtt, ~30 MiB of live data at start-up) to a WasmGC
component. During start-up, the post-GC allocated bytes double from 32 KiB up to 32 MiB and then stay at
0x1fff9f0–0x2000000 in a 64 MiB heap: 54 collections ~110 ms apart, each copying ~32 MiB and freeing a few hundred
bytes to ~1.5 KiB. GC was 51% of the run's CPU samples (perf: `CopyingHeap::forward`, `memmove`, `scan_field`,
`collect_increment`); with `-O gc-heap-initial-size=268435456` it is 19% and the start-up phase we measured goes from
4 s to 1 s.

## Possible fix

For the copying collector, compare against the active semi-space, e.g. collect first only if
`predicted_usage < semi_space_size / 2` (i.e. `capacity / 4`), so that after a collection at least half of the active
space is free. That keeps RSS O(live set) (≤ 4× live including the idle semi-space) while bounding the copy work per
allocated byte. The tunable ratio proposed in #13470 would also let embedders choose, but the default ratio for a
two-space collector arguably needs to account for the second space either way.
