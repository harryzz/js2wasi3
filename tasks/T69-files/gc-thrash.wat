;; T69: wasmtime's copying collector near a full semi-space. A live list grows to 29,000 nodes of 1 KiB
;; (one per 64 short-lived allocations), then only garbage is allocated (8M small structs in all). Run:
;;   wasm-tools parse gc-thrash.wat -o gc-thrash.wasm
;;   wasmtime run -W gc=y,function-references=y --invoke run gc-thrash.wasm               (default)
;;   wasmtime run -W gc=y,function-references=y -O gc-heap-initial-size=536870912 --invoke run gc-thrash.wasm
(module
  (type $bytes (array (mut i8)))
  (type $node (struct (field $next (ref null $node)) (field $data (ref $bytes))))
  (type $tmp (struct (field i32) (field i32)))
  (global $live (mut (ref null $node)) (ref.null $node))
  (global $sink (mut (ref null $tmp)) (ref.null $tmp))
  (func (export "run") (result i32)
    (local $i i32) (local $n i32)
    (loop $l
      ;; Garbage: a small struct, kept only until the next one.
      (global.set $sink (struct.new $tmp (local.get $i) (local.get $i)))
      ;; Every 64th iteration, one more live node of 1 KiB, until 29,000 of them (~30 MiB: just under the 32 MiB semi-space of a 64 MiB heap).
      (if (i32.and (i32.eqz (i32.and (local.get $i) (i32.const 63)))
                   (i32.lt_u (local.get $n) (i32.const 29000)))
        (then
          (global.set $live (struct.new $node (global.get $live) (array.new_default $bytes (i32.const 1024))))
          (local.set $n (i32.add (local.get $n) (i32.const 1)))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $l (i32.lt_u (local.get $i) (i32.const 8000000))))
    (local.get $n)))
