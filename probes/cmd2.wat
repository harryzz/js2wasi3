(module
  (type $ft (func))
  (type $ct (cont $ft))
  (type $bt (func (result i32 (ref $ct))))
  (tag $await (param i32))                 ;; payload: waitable handle to wait on
  (import "wasi:clocks/monotonic-clock@0.3.1" "[async-lower]wait-for" (func $wait_for (param i64) (result i32)))
  (import "wasi:cli/stdout@0.3.1" "write-via-stream" (func $write_via_stream (param i32) (result i32)))
  (import "wasi:cli/stdout@0.3.1" "[stream-new-0]write-via-stream" (func $stream_new (result i64)))
  (import "wasi:cli/stdout@0.3.1" "[async-lower][stream-write-0]write-via-stream" (func $stream_write (param i32 i32 i32) (result i32)))
  (import "wasi:cli/stdout@0.3.1" "[stream-drop-writable-0]write-via-stream" (func $stream_drop_w (param i32)))
  (import "$root" "[waitable-set-new]" (func $ws_new (result i32)))
  (import "$root" "[waitable-join]" (func $join (param i32 i32)))
  (import "$root" "[subtask-drop]" (func $subtask_drop (param i32)))
  (import "[export]wasi:cli/run@0.3.1" "[task-return]run" (func $task_return (param i32)))
  (memory (export "memory") 1)
  (data (i32.const 16) "tick 1 (before await)\n")   ;; 22
  (data (i32.const 48) "tick 2 (after await 1)\n")  ;; 23
  (data (i32.const 80) "tick 3 (after await 2)\n")  ;; 23
  (global $ws (mut i32) (i32.const 0))
  (global $w (mut i32) (i32.const 0))
  (global $k (mut (ref null $ct)) (ref.null $ct))

  ;; "await" helper usable from straight-line code: suspends the whole JS task
  (func $await_handle (param $h i32) (suspend $await (local.get $h)))

  (func $write (param $p i32) (param $n i32)
    (local $rc i32)
    (local.set $rc (call $stream_write (global.get $w) (local.get $p) (local.get $n)))
    (if (i32.eq (local.get $rc) (i32.const -1)) (then (call $await_handle (global.get $w)))))

  (func $sleep (param $ns i64)
    (local $st i32)
    (local.set $st (call $wait_for (local.get $ns)))
    (if (i32.ne (i32.and (local.get $st) (i32.const 15)) (i32.const 2))
      (then
        (call $await_handle (i32.shr_u (local.get $st) (i32.const 4)))
        (call $subtask_drop (i32.shr_u (local.get $st) (i32.const 4))))))

  ;; the "JS async main", written as ordinary direct-style code
  (func $js_main
    (call $write (i32.const 16) (i32.const 22))
    (call $sleep (i64.const 30000000))
    (call $write (i32.const 48) (i32.const 23))
    (call $sleep (i64.const 30000000))
    (call $write (i32.const 80) (i32.const 23)))
  (elem declare func $js_main)

  ;; event-loop step: resume the task; on await -> join handle, return WAIT; on finish -> EXIT
  (func $step (result i32)
    (local $h i32)
    (block $on_await (type $bt)
      (resume $ct (on $await $on_await) (ref.as_non_null (global.get $k)))
      (call $stream_drop_w (global.get $w))
      (call $task_return (i32.const 0))
      (return (i32.const 0)))
    (global.set $k)
    (local.set $h)
    (call $join (local.get $h) (global.get $ws))
    (i32.or (i32.const 2) (i32.shl (global.get $ws) (i32.const 4))))

  (func (export "[async-lift]wasi:cli/run@0.3.1#run") (result i32)
    (local $s i64)
    (global.set $ws (call $ws_new))
    (local.set $s (call $stream_new))
    (global.set $w (i32.wrap_i64 (i64.shr_u (local.get $s) (i64.const 32))))
    (drop (call $write_via_stream (i32.wrap_i64 (local.get $s))))
    (global.set $k (cont.new $ct (ref.func $js_main)))
    (call $step))

  (func (export "[callback][async-lift]wasi:cli/run@0.3.1#run") (param $ev i32) (param $idx i32) (param $payload i32) (result i32)
    (call $join (local.get $idx) (i32.const 0))   ;; leave the set
    (call $step))

  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32) unreachable))
