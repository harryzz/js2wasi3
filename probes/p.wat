(component
  (core module $m
    (type $box (struct (field f64)))
    (tag $js (param anyref))
    (import "" "task.return" (func $task_return (param f64)))
    (func (export "run") (result i32)
      block $h (result anyref)
        try_table (catch $js $h)
          (throw $js (struct.new $box (f64.const 42)))
        end
        unreachable
      end
      ref.cast (ref $box)
      struct.get $box 0
      call $task_return
      i32.const 0)
    (func (export "cb") (param i32 i32 i32) (result i32) unreachable))
  (core func $tr (canon task.return (result f64)))
  (core instance $i (instantiate $m (with "" (instance (export "task.return" (func $tr))))))
  (func (export "run") async (result f64)
    (canon lift (core func $i "run") async (callback (core func $i "cb")))))
