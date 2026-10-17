;; T000 probe 2: probes/layout2.js compiled by hand in the proposed design, with the costs the language adds:
;; - every object has a header: $flags (non-zero = a struct field was deleted, got a non-number, or the prototype
;;   changed: fast paths must not trust the struct fields) and $extra (an overflow dictionary for added keys);
;; - a method guard is: prototype unchanged (global) + receiver has the shape (cast) + its $flags == 0;
;; - `b.tag = i` and `Object.keys(b)` go through generic code that understands struct objects;
;; - `new Vec(i, half, 1)` with `half` of unknown type: the constructor checks it's a number (else a generic object).
;; Keys are small integers here (x=1 y=2 z=3 half=4 tag=5); real keys would be interned strings.
(module
  (type $ia (array (mut i32)))
  (type $va (array (mut eqref)))
  (type $dict (struct (field $n (mut i32)) (field $k (mut (ref $ia))) (field $v (mut (ref $va)))))
  (type $obj (sub (struct (field $flags (mut i32)) (field $extra (mut (ref null $dict))))))
  (type $Vec (sub final $obj (struct (field $flags (mut i32)) (field $extra (mut (ref null $dict)))
                                     (field $x (mut f64)) (field $y (mut f64)) (field $z (mut f64)))))
  (type $Plain (sub final $obj (struct (field $flags (mut i32)) (field $extra (mut (ref null $dict))))))
  (type $box (struct (field f64)))

  (global $proto_ok (mut i32) (i32.const 1))
  (global $sink (mut i32) (i32.const 0))
  (global $cfg (mut (ref null $Plain)) (ref.null $Plain))

  ;; ---- generic paths ----
  (func $generic (param eqref eqref) (result eqref)
    (global.set $sink (i32.add (global.get $sink) (i32.const 1)))
    (ref.null eq))
  (func $generic_num (param eqref eqref) (result f64)
    (global.set $sink (i32.add (global.get $sink) (i32.const 1)))
    (f64.const 0))

  ;; dictionary: find key, -1 if absent
  (func $dict_find (param $d (ref $dict)) (param $key i32) (result i32)
    (local $i i32)
    (block $no
      (loop $l
        (br_if $no (i32.ge_u (local.get $i) (struct.get $dict $n (local.get $d))))
        (if (i32.eq (array.get $ia (struct.get $dict $k (local.get $d)) (local.get $i)) (local.get $key))
          (then (return (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $l)))
    (i32.const -1))

  ;; dictionary: set key (append, growing the arrays)
  (func $dict_set (param $d (ref $dict)) (param $key i32) (param $val eqref)
    (local $i i32) (local $n i32) (local $k2 (ref $ia)) (local $v2 (ref $va))
    (local.set $i (call $dict_find (local.get $d) (local.get $key)))
    (if (i32.ge_s (local.get $i) (i32.const 0))
      (then (array.set $va (struct.get $dict $v (local.get $d)) (local.get $i) (local.get $val)) (return)))
    (local.set $n (struct.get $dict $n (local.get $d)))
    (if (i32.eq (local.get $n) (array.len (struct.get $dict $k (local.get $d))))
      (then
        (local.set $k2 (array.new_default $ia (i32.mul (local.get $n) (i32.const 2))))
        (local.set $v2 (array.new_default $va (i32.mul (local.get $n) (i32.const 2))))
        (array.copy $ia $ia (local.get $k2) (i32.const 0) (struct.get $dict $k (local.get $d)) (i32.const 0) (local.get $n))
        (array.copy $va $va (local.get $v2) (i32.const 0) (struct.get $dict $v (local.get $d)) (i32.const 0) (local.get $n))
        (struct.set $dict $k (local.get $d) (local.get $k2))
        (struct.set $dict $v (local.get $d) (local.get $v2))))
    (array.set $ia (struct.get $dict $k (local.get $d)) (local.get $n) (local.get $key))
    (array.set $va (struct.get $dict $v (local.get $d)) (local.get $n) (local.get $val))
    (struct.set $dict $n (local.get $d) (i32.add (local.get $n) (i32.const 1))))

  (func $new_dict (result (ref $dict))
    (struct.new $dict (i32.const 0) (array.new_default $ia (i32.const 4)) (array.new_default $va (i32.const 4))))

  ;; a number from a JS value, or NaN-flag via the second result (0 = not a number)
  (func $to_f64 (param $v eqref) (result f64 i32)
    (drop (block $no (result eqref)
      (return (struct.get $box 0 (br_on_cast_fail $no eqref (ref $box) (local.get $v))) (i32.const 1))))
    (f64.const 0) (i32.const 0))

  ;; generic [[Set]] of key on any object (struct fields of a Vec, else the overflow dictionary)
  (func $set_generic (param $o eqref) (param $key i32) (param $val eqref)
    (local $ob (ref $obj)) (local $vec (ref $Vec)) (local $f f64) (local $ok i32) (local $d (ref null $dict))
    (local.set $ob (ref.cast (ref $obj) (local.get $o)))
    (drop (block $not_vec (result (ref $obj))
      (local.set $vec (br_on_cast_fail $not_vec (ref $obj) (ref $Vec) (local.get $ob)))
      (if (i32.and (i32.ge_u (local.get $key) (i32.const 1)) (i32.le_u (local.get $key) (i32.const 3)))
        (then
          (call $to_f64 (local.get $val)) (local.set $ok) (local.set $f)
          (if (i32.eqz (local.get $ok))
            ;; non-number into an f64 field: flag the object, keep the value in the dictionary
            (then (struct.set $obj $flags (local.get $ob) (i32.const 1)))
            (else
              (if (i32.eq (local.get $key) (i32.const 1)) (then (struct.set $Vec $x (local.get $vec) (local.get $f)) (return)))
              (if (i32.eq (local.get $key) (i32.const 2)) (then (struct.set $Vec $y (local.get $vec) (local.get $f)) (return)))
              (struct.set $Vec $z (local.get $vec) (local.get $f)) (return)))))
      (local.get $ob)))
    (local.set $d (struct.get $obj $extra (local.get $ob)))
    (if (ref.is_null (local.get $d))
      (then (local.set $d (call $new_dict)) (struct.set $obj $extra (local.get $ob) (local.get $d))))
    (call $dict_set (ref.as_non_null (local.get $d)) (local.get $key) (local.get $val)))

  ;; generic [[Get]] (only what this probe needs: Plain objects)
  (func $get_generic (param $o eqref) (param $key i32) (result eqref)
    (local $d (ref null $dict)) (local $i i32)
    (local.set $d (struct.get $obj $extra (ref.cast (ref $obj) (local.get $o))))
    (if (ref.is_null (local.get $d)) (then (return (ref.null eq))))
    (local.set $i (call $dict_find (ref.as_non_null (local.get $d)) (local.get $key)))
    (if (i32.lt_s (local.get $i) (i32.const 0)) (then (return (ref.null eq))))
    (array.get $va (struct.get $dict $v (ref.as_non_null (local.get $d))) (local.get $i)))

  ;; Object.keys(o).length for any object (struct fields first, then added keys)
  (func $keys_len (param $o eqref) (result i32)
    (local $ob (ref $obj)) (local $n i32) (local $d (ref null $dict))
    (local.set $ob (ref.cast (ref $obj) (local.get $o)))
    (if (ref.test (ref $Vec) (local.get $ob)) (then (local.set $n (i32.const 3))))
    (local.set $d (struct.get $obj $extra (local.get $ob)))
    (if (i32.eqz (ref.is_null (local.get $d)))
      (then (local.set $n (i32.add (local.get $n) (struct.get $dict $n (ref.as_non_null (local.get $d)))))))
    (local.get $n))

  ;; ---- Vec ----
  (func $Vec_new (param f64 f64 f64) (result (ref $Vec))
    (struct.new $Vec (i32.const 0) (ref.null $dict) (local.get 0) (local.get 1) (local.get 2)))

  ;; new Vec(x, y, z) with y of unknown type: a Vec if it's a number, else a generic object
  (func $Vec_new_y (param $x f64) (param $y eqref) (param $z f64) (result eqref)
    (local $f f64) (local $ok i32) (local $p (ref $Plain))
    (call $to_f64 (local.get $y)) (local.set $ok) (local.set $f)
    (if (local.get $ok) (then (return (call $Vec_new (local.get $x) (local.get $f) (local.get $z)))))
    (local.set $p (struct.new $Plain (i32.const 0) (ref.null $dict)))
    (call $set_generic (local.get $p) (i32.const 1) (struct.new $box (local.get $x)))
    (call $set_generic (local.get $p) (i32.const 2) (local.get $y))
    (call $set_generic (local.get $p) (i32.const 3) (struct.new $box (local.get $z)))
    (local.get $p))

  (func $Vec_add (param $this (ref $Vec)) (param $o eqref) (result eqref)
    (local $p (ref $Vec))
    (call $generic (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $Vec)
        local.set $p
        (drop (br_if $slow (local.get $o) (struct.get $Vec $flags (local.get $p))))
        (return (call $Vec_new
          (f64.add (struct.get $Vec $x (local.get $this)) (struct.get $Vec $x (local.get $p)))
          (f64.add (struct.get $Vec $y (local.get $this)) (struct.get $Vec $y (local.get $p)))
          (f64.add (struct.get $Vec $z (local.get $this)) (struct.get $Vec $z (local.get $p))))))))

  (func $Vec_dot (param $this (ref $Vec)) (param $o eqref) (result f64)
    (local $p (ref $Vec))
    (call $generic_num (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $Vec)
        local.set $p
        (drop (br_if $slow (local.get $o) (struct.get $Vec $flags (local.get $p))))
        (return
          (f64.add
            (f64.add
              (f64.mul (struct.get $Vec $x (local.get $this)) (struct.get $Vec $x (local.get $p)))
              (f64.mul (struct.get $Vec $y (local.get $this)) (struct.get $Vec $y (local.get $p))))
            (f64.mul (struct.get $Vec $z (local.get $this)) (struct.get $Vec $z (local.get $p))))))))

  (func (export "run_typed") (param $n f64) (result f64)
    (local $a eqref) (local $b eqref) (local $s f64) (local $i i32) (local $half eqref) (local $r (ref $Vec))
    ;; const cfg = { half: 0.5 }  (module start in a real program)
    (global.set $cfg (struct.new $Plain (i32.const 0) (ref.null $dict)))
    (call $set_generic (global.get $cfg) (i32.const 4) (struct.new $box (f64.const 0.5)))
    (local.set $half (call $get_generic (global.get $cfg) (i32.const 4)))
    (local.set $a (call $Vec_new (f64.const 0) (f64.const 0) (f64.const 0)))
    (block $exit
      (loop $top
        (br_if $exit (i32.eqz (f64.lt (f64.convert_i32_s (local.get $i)) (local.get $n))))
        (local.set $b (call $Vec_new_y (f64.convert_i32_s (local.get $i)) (local.get $half) (f64.const 1)))
        ;; if (i % 1000 === 0) { b.tag = i; s += Object.keys(b).length; }
        (if (i32.eqz (i32.rem_s (local.get $i) (i32.const 1000)))
          (then
            (call $set_generic (local.get $b) (i32.const 5) (struct.new $box (f64.convert_i32_s (local.get $i))))
            (local.set $s (f64.add (local.get $s) (f64.convert_i32_s (call $keys_len (local.get $b)))))))
        ;; a = a.add(b)
        (local.set $a
          (block $done (result eqref)
            (call $generic
              (block $slow (result eqref)
                local.get $a
                global.get $proto_ok
                i32.eqz
                br_if $slow
                br_on_cast_fail $slow eqref (ref $Vec)
                local.set $r
                (drop (br_if $slow (local.get $a) (struct.get $Vec $flags (local.get $r))))
                local.get $r
                local.get $b
                call $Vec_add
                br $done)
              (local.get $b))))
        ;; s += a.dot(b)
        (local.set $s (f64.add (local.get $s)
          (block $done (result f64)
            (call $generic_num
              (block $slow (result eqref)
                local.get $a
                global.get $proto_ok
                i32.eqz
                br_if $slow
                br_on_cast_fail $slow eqref (ref $Vec)
                local.set $r
                (drop (br_if $slow (local.get $a) (struct.get $Vec $flags (local.get $r))))
                local.get $r
                local.get $b
                call $Vec_dot
                br $done)
              (local.get $b)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $top)))
    (local.get $s))
)
