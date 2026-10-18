;; T132 probe: probes/layout.js by hand in two more layouts, to separate the object layout from the call path.
;; run_dict (P3): today's object layout ($object: class, proto, keys, vals, attrs, count, index; layout keys shared,
;;   T117) with T110-style inline reads at a known slot (count > slot && keys[slot] == key), boxed numbers, and the
;;   new design's direct guarded method calls (receiver is a $plain whose prototype is Vec.prototype, unchanged).
;; run_hdr (P4): option (a): a struct per shape that is a subtype of today's $object (so it carries the whole header)
;;   with f64 fields; guard = cast + class flags == 0.
;; run_small (P5): the same with a 2-field header (flags, overflow dictionary) instead of today's 7 fields.
(module
  (type $keys (array (mut eqref)))
  (type $vals (array (mut eqref)))
  (type $bytes (array (mut i8)))
  (type $i32s (array (mut i32)))
  (type $object (sub (struct (field $class (mut i32)) (field $proto (mut (ref null $object))) (field $keys (mut (ref $keys)))
    (field $vals (mut (ref $vals))) (field $attrs (mut (ref $bytes))) (field $count (mut i32)) (field $index (mut (ref null $i32s))))))
  (type $plain (sub final $object (struct (field $class (mut i32)) (field $proto (mut (ref null $object))) (field $keys (mut (ref $keys)))
    (field $vals (mut (ref $vals))) (field $attrs (mut (ref $bytes))) (field $count (mut i32)) (field $index (mut (ref null $i32s))))))
  (type $VecH (sub final $object (struct (field $class (mut i32)) (field $proto (mut (ref null $object))) (field $keys (mut (ref $keys)))
    (field $vals (mut (ref $vals))) (field $attrs (mut (ref $bytes))) (field $count (mut i32)) (field $index (mut (ref null $i32s)))
    (field $x (mut f64)) (field $y (mut f64)) (field $z (mut f64)))))
  (type $VecS (sub final (struct (field $class (mut i32)) (field $extra (mut (ref null $vals)))
    (field $x (mut f64)) (field $y (mut f64)) (field $z (mut f64)))))
  (type $box (struct (field f64)))

  (global $proto_ok (mut i32) (i32.const 1))
  (global $sink (mut i32) (i32.const 0))
  (global $vkeys (mut (ref null $keys)) (ref.null $keys))
  (global $vattrs (mut (ref null $bytes)) (ref.null $bytes))
  (global $novals (mut (ref null $vals)) (ref.null $vals))
  (global $vproto (mut (ref null $object)) (ref.null $object))

  (func $init
    (global.set $vkeys (array.new_fixed $keys 3 (ref.i31 (i32.const 1)) (ref.i31 (i32.const 2)) (ref.i31 (i32.const 3))))
    (global.set $vattrs (array.new_default $bytes (i32.const 3)))
    (global.set $novals (array.new_default $vals (i32.const 0)))
    (global.set $vproto (struct.new $plain (i32.const 0) (ref.null $object) (array.new_default $keys (i32.const 0))
      (ref.as_non_null (global.get $novals)) (array.new_default $bytes (i32.const 0)) (i32.const 0) (ref.null $i32s))))

  ;; stand-ins for the generic paths; never taken here
  (func $generic (param eqref eqref) (result eqref)
    (global.set $sink (i32.add (global.get $sink) (i32.const 1))) (ref.null eq))
  (func $generic_num2 (param eqref eqref) (result f64)
    (global.set $sink (i32.add (global.get $sink) (i32.const 1))) (f64.const 0))
  (func $generic_num (param eqref eqref) (result f64)
    (global.set $sink (i32.add (global.get $sink) (i32.const 1))) (f64.const 0))

  ;; new Vec(x, y, z), today's layout: the shared layout keys/attrs, a new vals array (2 allocations, T117)
  (func $Dict_new (param eqref eqref eqref) (result (ref $plain))
    (struct.new $plain (i32.const 0) (global.get $vproto) (ref.as_non_null (global.get $vkeys))
      (array.new_fixed $vals 3 (local.get 0) (local.get 1) (local.get 2))
      (ref.as_non_null (global.get $vattrs)) (i32.const 3) (ref.null $i32s)))
  ;; option (a): one struct with the whole header
  (func $Hdr_new (param f64 f64 f64) (result (ref $VecH))
    (struct.new $VecH (i32.const 0) (global.get $vproto) (ref.as_non_null (global.get $vkeys))
      (ref.as_non_null (global.get $novals)) (ref.as_non_null (global.get $vattrs)) (i32.const 0) (ref.null $i32s)
      (local.get 0) (local.get 1) (local.get 2)))

  (func $dict_add (param $this (ref $plain)) (param $o eqref) (result eqref)
    (local $p (ref $plain))
    (call $generic (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $plain)
        local.set $p
        (drop (br_if $slow (local.get $o) (i32.or (i32.eqz (global.get $proto_ok)) (i32.eqz (ref.eq (struct.get $plain $proto (local.get $p)) (global.get $vproto))))))
        (return (call $Dict_new
          (struct.new $box (f64.add (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $this)) (i32.const 0))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $this)) (i32.const 0)) (ref.i31 (i32.const 1))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $this)) (i32.const 0)))))
          (else (call $generic_num (local.get $this) (ref.i31 (i32.const 1))))) (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $p)) (i32.const 0))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $p)) (i32.const 0)) (ref.i31 (i32.const 1))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $p)) (i32.const 0)))))
          (else (call $generic_num (local.get $p) (ref.i31 (i32.const 1)))))))
          (struct.new $box (f64.add (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $this)) (i32.const 1))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $this)) (i32.const 1)) (ref.i31 (i32.const 2))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $this)) (i32.const 1)))))
          (else (call $generic_num (local.get $this) (ref.i31 (i32.const 2))))) (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $p)) (i32.const 1))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $p)) (i32.const 1)) (ref.i31 (i32.const 2))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $p)) (i32.const 1)))))
          (else (call $generic_num (local.get $p) (ref.i31 (i32.const 2)))))))
          (struct.new $box (f64.add (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $this)) (i32.const 2))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $this)) (i32.const 2)) (ref.i31 (i32.const 3))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $this)) (i32.const 2)))))
          (else (call $generic_num (local.get $this) (ref.i31 (i32.const 3))))) (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $p)) (i32.const 2))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $p)) (i32.const 2)) (ref.i31 (i32.const 3))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $p)) (i32.const 2)))))
          (else (call $generic_num (local.get $p) (ref.i31 (i32.const 3))))))))))))
  (func $dict_dot (param $this (ref $plain)) (param $o eqref) (result f64)
    (local $p (ref $plain))
    (call $generic_num2 (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $plain)
        local.set $p
        (drop (br_if $slow (local.get $o) (i32.or (i32.eqz (global.get $proto_ok)) (i32.eqz (ref.eq (struct.get $plain $proto (local.get $p)) (global.get $vproto))))))
        (return (f64.add (f64.add (f64.mul (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $this)) (i32.const 0))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $this)) (i32.const 0)) (ref.i31 (i32.const 1))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $this)) (i32.const 0)))))
          (else (call $generic_num (local.get $this) (ref.i31 (i32.const 1))))) (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $p)) (i32.const 0))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $p)) (i32.const 0)) (ref.i31 (i32.const 1))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $p)) (i32.const 0)))))
          (else (call $generic_num (local.get $p) (ref.i31 (i32.const 1)))))) (f64.mul (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $this)) (i32.const 1))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $this)) (i32.const 1)) (ref.i31 (i32.const 2))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $this)) (i32.const 1)))))
          (else (call $generic_num (local.get $this) (ref.i31 (i32.const 2))))) (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $p)) (i32.const 1))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $p)) (i32.const 1)) (ref.i31 (i32.const 2))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $p)) (i32.const 1)))))
          (else (call $generic_num (local.get $p) (ref.i31 (i32.const 2))))))) (f64.mul (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $this)) (i32.const 2))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $this)) (i32.const 2)) (ref.i31 (i32.const 3))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $this)) (i32.const 2)))))
          (else (call $generic_num (local.get $this) (ref.i31 (i32.const 3))))) (if (result f64) (i32.and (i32.gt_s (struct.get $plain $count (local.get $p)) (i32.const 2))
            (ref.eq (array.get $keys (struct.get $plain $keys (local.get $p)) (i32.const 2)) (ref.i31 (i32.const 3))))
          (then (struct.get $box 0 (ref.cast (ref $box) (array.get $vals (struct.get $plain $vals (local.get $p)) (i32.const 2)))))
          (else (call $generic_num (local.get $p) (ref.i31 (i32.const 3)))))))))))
  (func (export "run_dict") (param $n f64) (result f64)
    (local $a eqref) (local $b eqref) (local $s f64) (local $i f64) (local $r (ref $plain))
    (call $init)
    (local.set $a (call $Dict_new (struct.new $box (f64.const 0)) (struct.new $box (f64.const 0)) (struct.new $box (f64.const 0))))
    (block $exit
      (loop $top
        (br_if $exit (i32.eqz (f64.lt (local.get $i) (local.get $n))))
        (local.set $b (call $Dict_new (struct.new $box (local.get $i)) (struct.new $box (f64.const 0.5)) (struct.new $box (f64.const 1))))
        (local.set $a
          (block $done (result eqref)
            (call $generic
              (block $slow (result eqref)
                local.get $a
                br_on_cast_fail $slow eqref (ref $plain)
                local.set $r
                (drop (br_if $slow (local.get $a) (i32.or (i32.eqz (global.get $proto_ok)) (i32.eqz (ref.eq (struct.get $plain $proto (local.get $r)) (global.get $vproto))))))
                local.get $r
                local.get $b
                call $dict_add
                br $done)
              (local.get $b))))
        (local.set $s (f64.add (local.get $s)
          (block $done (result f64)
            (call $generic_num2
              (block $slow (result eqref)
                local.get $a
                br_on_cast_fail $slow eqref (ref $plain)
                local.set $r
                (drop (br_if $slow (local.get $a) (i32.or (i32.eqz (global.get $proto_ok)) (i32.eqz (ref.eq (struct.get $plain $proto (local.get $r)) (global.get $vproto))))))
                local.get $r
                local.get $b
                call $dict_dot
                br $done)
              (local.get $b)))))
        (local.set $i (f64.add (local.get $i) (f64.const 1)))
        (br $top)))
    (local.get $s))

  (func $hdr_add (param $this (ref $VecH)) (param $o eqref) (result eqref)
    (local $p (ref $VecH))
    (call $generic (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $VecH)
        local.set $p
        (drop (br_if $slow (local.get $o) (struct.get $VecH $class (local.get $p))))
        (return (call $Hdr_new
          (f64.add (struct.get $VecH $x (local.get $this)) (struct.get $VecH $x (local.get $p)))
          (f64.add (struct.get $VecH $y (local.get $this)) (struct.get $VecH $y (local.get $p)))
          (f64.add (struct.get $VecH $z (local.get $this)) (struct.get $VecH $z (local.get $p))))))))
  (func $hdr_dot (param $this (ref $VecH)) (param $o eqref) (result f64)
    (local $p (ref $VecH))
    (call $generic_num2 (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $VecH)
        local.set $p
        (drop (br_if $slow (local.get $o) (struct.get $VecH $class (local.get $p))))
        (return (f64.add (f64.add (f64.mul (struct.get $VecH $x (local.get $this)) (struct.get $VecH $x (local.get $p))) (f64.mul (struct.get $VecH $y (local.get $this)) (struct.get $VecH $y (local.get $p)))) (f64.mul (struct.get $VecH $z (local.get $this)) (struct.get $VecH $z (local.get $p))))))))
  (func (export "run_hdr") (param $n f64) (result f64)
    (local $a eqref) (local $b eqref) (local $s f64) (local $i f64) (local $r (ref $VecH))
    (call $init)
    (local.set $a (call $Hdr_new (f64.const 0) (f64.const 0) (f64.const 0)))
    (block $exit
      (loop $top
        (br_if $exit (i32.eqz (f64.lt (local.get $i) (local.get $n))))
        (local.set $b (call $Hdr_new (local.get $i) (f64.const 0.5) (f64.const 1)))
        (local.set $a
          (block $done (result eqref)
            (call $generic
              (block $slow (result eqref)
                local.get $a
                br_on_cast_fail $slow eqref (ref $VecH)
                local.set $r
                (drop (br_if $slow (local.get $a) (struct.get $VecH $class (local.get $r))))
                local.get $r
                local.get $b
                call $hdr_add
                br $done)
              (local.get $b))))
        (local.set $s (f64.add (local.get $s)
          (block $done (result f64)
            (call $generic_num2
              (block $slow (result eqref)
                local.get $a
                br_on_cast_fail $slow eqref (ref $VecH)
                local.set $r
                (drop (br_if $slow (local.get $a) (struct.get $VecH $class (local.get $r))))
                local.get $r
                local.get $b
                call $hdr_dot
                br $done)
              (local.get $b)))))
        (local.set $i (f64.add (local.get $i) (f64.const 1)))
        (br $top)))
    (local.get $s))
  ;; P5: a struct per shape with probe 2's small header (flags, overflow dictionary), not an $object subtype
  (func $Small_new (param f64 f64 f64) (result (ref $VecS))
    (struct.new $VecS (i32.const 0) (ref.null $vals) (local.get 0) (local.get 1) (local.get 2)))
  (func $small_add (param $this (ref $VecS)) (param $o eqref) (result eqref)
    (local $p (ref $VecS))
    (call $generic (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $VecS)
        local.set $p
        (drop (br_if $slow (local.get $o) (struct.get $VecS $class (local.get $p))))
        (return (call $Small_new
          (f64.add (struct.get $VecS $x (local.get $this)) (struct.get $VecS $x (local.get $p)))
          (f64.add (struct.get $VecS $y (local.get $this)) (struct.get $VecS $y (local.get $p)))
          (f64.add (struct.get $VecS $z (local.get $this)) (struct.get $VecS $z (local.get $p))))))))
  (func $small_dot (param $this (ref $VecS)) (param $o eqref) (result f64)
    (local $p (ref $VecS))
    (call $generic_num2 (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $VecS)
        local.set $p
        (drop (br_if $slow (local.get $o) (struct.get $VecS $class (local.get $p))))
        (return (f64.add (f64.add (f64.mul (struct.get $VecS $x (local.get $this)) (struct.get $VecS $x (local.get $p))) (f64.mul (struct.get $VecS $y (local.get $this)) (struct.get $VecS $y (local.get $p)))) (f64.mul (struct.get $VecS $z (local.get $this)) (struct.get $VecS $z (local.get $p))))))))
  (func (export "run_small") (param $n f64) (result f64)
    (local $a eqref) (local $b eqref) (local $s f64) (local $i f64) (local $r (ref $VecS))
    (call $init)
    (local.set $a (call $Small_new (f64.const 0) (f64.const 0) (f64.const 0)))
    (block $exit
      (loop $top
        (br_if $exit (i32.eqz (f64.lt (local.get $i) (local.get $n))))
        (local.set $b (call $Small_new (local.get $i) (f64.const 0.5) (f64.const 1)))
        (local.set $a
          (block $done (result eqref)
            (call $generic
              (block $slow (result eqref)
                local.get $a
                br_on_cast_fail $slow eqref (ref $VecS)
                local.set $r
                (drop (br_if $slow (local.get $a) (struct.get $VecS $class (local.get $r))))
                local.get $r
                local.get $b
                call $small_add
                br $done)
              (local.get $b))))
        (local.set $s (f64.add (local.get $s)
          (block $done (result f64)
            (call $generic_num2
              (block $slow (result eqref)
                local.get $a
                br_on_cast_fail $slow eqref (ref $VecS)
                local.set $r
                (drop (br_if $slow (local.get $a) (struct.get $VecS $class (local.get $r))))
                local.get $r
                local.get $b
                call $small_dot
                br $done)
              (local.get $b)))))
        (local.set $i (f64.add (local.get $i) (f64.const 1)))
        (br $top)))
    (local.get $s))
)
