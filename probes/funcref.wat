;; T89 (F350): the cost of calling through a funcref stored in a GC struct (`struct.get` + `call_ref`)
;; versus an i32 index into a funcref table (`call_indirect`), 50M calls each. `run_struct` and
;; `run_table` return the same sum; perf shows whether wasmtime's `get_interned_func_ref` libcall runs.
(module
  (type $code (func (param i32) (result i32)))
  (type $fn (struct (field $code (ref $code)) (field $idx i32)))
  (table $t 1 funcref)
  (elem (table $t) (i32.const 0) func $inc)
  (elem declare func $inc)
  (func $inc (type $code) (i32.add (local.get 0) (i32.const 1)))
  (func $make (result (ref $fn)) (struct.new $fn (ref.func $inc) (i32.const 0)))
  (func (export "run_struct") (result i32)
    (local $f (ref null $fn)) (local $i i32) (local $acc i32)
    (local.set $f (call $make))
    (loop $l
      (local.set $acc (call_ref $code (local.get $acc) (struct.get $fn $code (local.get $f))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $l (i32.lt_u (local.get $i) (i32.const 50000000))))
    (local.get $acc))
  (func (export "run_table") (result i32)
    (local $f (ref null $fn)) (local $i i32) (local $acc i32)
    (local.set $f (call $make))
    (loop $l
      (local.set $acc (call_indirect $t (type $code) (local.get $acc) (struct.get $fn $idx (local.get $f))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $l (i32.lt_u (local.get $i) (i32.const 50000000))))
    (local.get $acc)))
