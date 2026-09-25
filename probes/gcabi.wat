(component
  (core module $m
    (type $ft (func (param i32) (result i32)))
    (func (export "f") (type $ft) local.get 0))
  (core instance $i (instantiate $m))
  (core type $ct (func (param i32) (result i32)))
  (func (export "f") (param "x" u32) (result u32)
    (canon lift (core func $i "f") gc)))
