(component
  (core module $m
    (type $s (array (mut i8)))
    (func (export "f") (result (ref $s)) (array.new_fixed $s 2 (i32.const 104) (i32.const 105))))
  (core instance $i (instantiate $m))
  (func (export "f") (result string) (canon lift (core func $i "f") gc)))
