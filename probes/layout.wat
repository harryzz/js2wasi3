;; T000 probe: probes/layout.js compiled by hand in the proposed design, to check it before building it.
;; run_typed: an object is a WasmGC struct per shape with f64 fields (numbers unboxed); a method call is guarded by
;;   "prototype unchanged" (one global) + "receiver has this shape" (br_on_cast_fail), and the argument by one cast;
;;   a failed guard takes the generic path ($generic*, present but never taken here).
;; run_boxed: the same struct per shape, but fields and results are boxed numbers (as our values are today), to
;;   separate the object layout from the number representation.
;; Values outside a known type are eqref (a JS value); no inlining (calls stay calls).
(module
  (type $obj (sub (struct)))
  (type $Vec (sub final $obj (struct (field $x f64) (field $y f64) (field $z f64))))
  (type $box (struct (field f64)))
  (type $VecB (sub final $obj (struct (field $x eqref) (field $y eqref) (field $z eqref))))

  ;; cleared when Vec.prototype.add/dot is replaced (watchpoint)
  (global $proto_ok (mut i32) (i32.const 1))
  (global $sink (mut i32) (i32.const 0))

  ;; stand-ins for the generic paths (property lookup + call); never taken in this probe
  (func $generic (param eqref eqref) (result eqref)
    (global.set $sink (i32.add (global.get $sink) (i32.const 1)))
    (ref.null eq))
  (func $generic_num (param eqref eqref) (result f64)
    (global.set $sink (i32.add (global.get $sink) (i32.const 1)))
    (f64.const 0))

  ;; ---- typed ----
  (func $Vec_new (param f64 f64 f64) (result (ref $Vec))
    (struct.new $Vec (local.get 0) (local.get 1) (local.get 2)))

  ;; Vec.prototype.add specialized for `this` a Vec; `o` checked once
  (func $Vec_add (param $this (ref $Vec)) (param $o eqref) (result eqref)
    (local $p (ref $Vec))
    (call $generic (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $Vec)
        local.set $p
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
        (return
          (f64.add
            (f64.add
              (f64.mul (struct.get $Vec $x (local.get $this)) (struct.get $Vec $x (local.get $p)))
              (f64.mul (struct.get $Vec $y (local.get $this)) (struct.get $Vec $y (local.get $p))))
            (f64.mul (struct.get $Vec $z (local.get $this)) (struct.get $Vec $z (local.get $p))))))))

  (func (export "run_typed") (param $n f64) (result f64)
    (local $a eqref) (local $b eqref) (local $s f64) (local $i f64)
    (local.set $a (call $Vec_new (f64.const 0) (f64.const 0) (f64.const 0)))
    (block $exit
      (loop $top
        (br_if $exit (i32.eqz (f64.lt (local.get $i) (local.get $n))))
        (local.set $b (call $Vec_new (local.get $i) (f64.const 0.5) (f64.const 1)))
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
                local.get $b
                call $Vec_dot
                br $done)
              (local.get $b)))))
        (local.set $i (f64.add (local.get $i) (f64.const 1)))
        (br $top)))
    (local.get $s))

  ;; ---- boxed numbers ----
  (func $num (param eqref) (result f64)
    (struct.get $box 0 (ref.cast (ref $box) (local.get 0))))
  (func $VecB_new (param eqref eqref eqref) (result (ref $VecB))
    (struct.new $VecB (local.get 0) (local.get 1) (local.get 2)))

  (func $VecB_add (param $this (ref $VecB)) (param $o eqref) (result eqref)
    (local $p (ref $VecB))
    (call $generic (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $VecB)
        local.set $p
        (return (call $VecB_new
          (struct.new $box (f64.add (call $num (struct.get $VecB $x (local.get $this))) (call $num (struct.get $VecB $x (local.get $p)))))
          (struct.new $box (f64.add (call $num (struct.get $VecB $y (local.get $this))) (call $num (struct.get $VecB $y (local.get $p)))))
          (struct.new $box (f64.add (call $num (struct.get $VecB $z (local.get $this))) (call $num (struct.get $VecB $z (local.get $p))))))))))

  (func $VecB_dot (param $this (ref $VecB)) (param $o eqref) (result eqref)
    (local $p (ref $VecB))
    (call $generic (local.get $this)
      (block $slow (result eqref)
        local.get $o
        br_on_cast_fail $slow eqref (ref $VecB)
        local.set $p
        (return (struct.new $box
          (f64.add
            (f64.add
              (f64.mul (call $num (struct.get $VecB $x (local.get $this))) (call $num (struct.get $VecB $x (local.get $p))))
              (f64.mul (call $num (struct.get $VecB $y (local.get $this))) (call $num (struct.get $VecB $y (local.get $p)))))
            (f64.mul (call $num (struct.get $VecB $z (local.get $this))) (call $num (struct.get $VecB $z (local.get $p))))))))))

  (func (export "run_boxed") (param $n f64) (result f64)
    (local $a eqref) (local $b eqref) (local $s f64) (local $i f64)
    (local.set $a (call $VecB_new (struct.new $box (f64.const 0)) (struct.new $box (f64.const 0)) (struct.new $box (f64.const 0))))
    (block $exit
      (loop $top
        (br_if $exit (i32.eqz (f64.lt (local.get $i) (local.get $n))))
        (local.set $b (call $VecB_new (struct.new $box (local.get $i)) (struct.new $box (f64.const 0.5)) (struct.new $box (f64.const 1))))
        (local.set $a
          (block $done (result eqref)
            (call $generic
              (block $slow (result eqref)
                local.get $a
                global.get $proto_ok
                i32.eqz
                br_if $slow
                br_on_cast_fail $slow eqref (ref $VecB)
                local.get $b
                call $VecB_add
                br $done)
              (local.get $b))))
        (local.set $s (f64.add (local.get $s)
          (call $num
            (block $done (result eqref)
              (call $generic
                (block $slow (result eqref)
                  local.get $a
                  global.get $proto_ok
                  i32.eqz
                  br_if $slow
                  br_on_cast_fail $slow eqref (ref $VecB)
                  local.get $b
                  call $VecB_dot
                  br $done)
                (local.get $b))))))
        (local.set $i (f64.add (local.get $i) (f64.const 1)))
        (br $top)))
    (local.get $s))
)
