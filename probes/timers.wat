;; Probe 8 (T19): timers. Two `wait-until` subtasks (+50 ms) in flight at once, then a +5 s one
;; that is cancelled with `subtask.cancel`. Expected: exits ok after ~50 ms, not 100 ms or 5 s.
(module
  (import "wasi:clocks/monotonic-clock@0.3.1" "now" (func $now (result i64)))
  (import "wasi:clocks/monotonic-clock@0.3.1" "[async-lower]wait-until" (func $wait_until (param i64) (result i32)))
  (import "$root" "[waitable-set-new]" (func $ws_new (result i32)))
  (import "$root" "[waitable-join]" (func $join (param i32 i32)))
  (import "$root" "[subtask-drop]" (func $subtask_drop (param i32)))
  (import "$root" "[subtask-cancel]" (func $subtask_cancel (param i32) (result i32)))
  (import "[export]wasi:cli/run@0.3.1" "[task-return]run" (func $task_return (param i32)))
  (memory (export "memory") 1)
  (global $ws (mut i32) (i32.const 0))
  (global $left (mut i32) (i32.const 2))
  (global $long (mut i32) (i32.const 0))

  (func $start (param $ns i64) (result i32)
    (local $st i32)
    (local.set $st (call $wait_until (i64.add (call $now) (local.get $ns))))
    ;; state STARTED (1) expected: join the subtask to the set
    (if (i32.ne (i32.and (local.get $st) (i32.const 15)) (i32.const 1)) (then unreachable))
    (call $join (i32.shr_u (local.get $st) (i32.const 4)) (global.get $ws))
    (i32.shr_u (local.get $st) (i32.const 4)))

  (func (export "[async-lift]wasi:cli/run@0.3.1#run") (result i32)
    (global.set $ws (call $ws_new))
    (drop (call $start (i64.const 50000000)))
    (drop (call $start (i64.const 50000000)))
    (global.set $long (call $start (i64.const 5000000000)))
    (i32.or (i32.const 2) (i32.shl (global.get $ws) (i32.const 4))))

  (func (export "[callback][async-lift]wasi:cli/run@0.3.1#run") (param $ev i32) (param $idx i32) (param $payload i32) (result i32)
    (local $st i32)
    ;; EVENT_SUBTASK (1), RETURNED (2)
    (if (i32.ne (local.get $ev) (i32.const 1)) (then unreachable))
    (if (i32.ne (local.get $payload) (i32.const 2)) (then unreachable))
    (call $subtask_drop (local.get $idx))
    (global.set $left (i32.sub (global.get $left) (i32.const 1)))
    (if (i32.eqz (global.get $left))
      (then
        ;; cancel the long one (leave the set first): sync cancel returns its final state
        (call $join (global.get $long) (i32.const 0))
        (local.set $st (call $subtask_cancel (global.get $long)))
        (if (i32.ne (local.get $st) (i32.const 4)) (then unreachable))  ;; CANCELLED_BEFORE_RETURNED
        (call $subtask_drop (global.get $long))
        (call $task_return (i32.const 0))
        (return (i32.const 0))))
    (i32.or (i32.const 2) (i32.shl (global.get $ws) (i32.const 4))))

  (func (export "cabi_realloc") (param i32 i32 i32 i32) (result i32) unreachable))
