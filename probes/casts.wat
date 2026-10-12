;; T113 (root R1): the cost of `ref.test` against a final type and a non-final type, on a hit and on a miss,
;; 50M tests each. Expectation from wasmtime's `is_subtype` (F376): a final target is one inline type-id
;; compare (hit or miss); a non-final target is inline only when the value's type is exactly the target,
;; else a call. `run_<case>` returns the number of hits so the tests aren't dropped.
(module
  (rec
    (type $obj (sub (struct (field (mut i32)))))            ;; like `$object`: not final
    (type $plain (sub final $obj (struct (field (mut i32)))))  ;; a final plain-object type
    (type $map (sub final $obj (struct (field (mut i32)) (field i32))))  ;; another final subtype
    (type $str (sub final (struct (field i64)))))           ;; not an object at all
  (global $v (mut eqref) (ref.null eq))
  (func $loop (param $n i32) (param $t i32) (result i32)
    (local $i i32) (local $hits i32)
    (loop $l
      (local.set $hits (i32.add (local.get $hits)
        (if (result i32) (local.get $t)
          (then (ref.test (ref $plain) (global.get $v)))
          (else (ref.test (ref $obj) (global.get $v))))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $l (i32.lt_u (local.get $i) (local.get $n))))
    (local.get $hits))
  ;; t = 1: test against the final `$plain`; t = 0: against the non-final `$obj`.
  (func (export "final_hit") (result i32)
    (global.set $v (struct.new $plain (i32.const 1))) (call $loop (i32.const 50000000) (i32.const 1)))
  (func (export "final_miss_subtype") (result i32)
    (global.set $v (struct.new $map (i32.const 1) (i32.const 2))) (call $loop (i32.const 50000000) (i32.const 1)))
  (func (export "final_miss_other") (result i32)
    (global.set $v (struct.new $str (i64.const 1))) (call $loop (i32.const 50000000) (i32.const 1)))
  (func (export "final_miss_i31") (result i32)
    (global.set $v (ref.i31 (i32.const 7))) (call $loop (i32.const 50000000) (i32.const 1)))
  (func (export "nonfinal_exact") (result i32)
    (global.set $v (struct.new $obj (i32.const 1))) (call $loop (i32.const 50000000) (i32.const 0)))
  (func (export "nonfinal_subtype") (result i32)
    (global.set $v (struct.new $map (i32.const 1) (i32.const 2))) (call $loop (i32.const 50000000) (i32.const 0)))
  (func (export "nonfinal_other") (result i32)
    (global.set $v (struct.new $str (i64.const 1))) (call $loop (i32.const 50000000) (i32.const 0)))
  (func (export "nonfinal_i31") (result i32)
    (global.set $v (ref.i31 (i32.const 7))) (call $loop (i32.const 50000000) (i32.const 0)))
)
