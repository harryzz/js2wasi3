;; Probe 9 (T59): blocking filesystem calls from a callback-lifted `run`, with no non-default wasmtime flag.
;; Sync-lowered `async func`s (`open-at`, `wait-for`) block; streams are read and written with async
;; `stream.read`/`stream.write`, blocking in `waitable-set.wait` when they'd block (sync stream ops need 🚝).
;; Expected: prints the preopened directory's `hello.txt`, exits ok.
(module
  (import "wasi:filesystem/preopens@0.3.1" "get-directories" (func $get_dirs (param i32)))
  (import "wasi:filesystem/types@0.3.1" "[method]descriptor.open-at"
    (func $open_at (param i32 i32 i32 i32 i32 i32 i32)))
  (import "wasi:filesystem/types@0.3.1" "[method]descriptor.read-via-stream"
    (func $read_via_stream (param i32 i64 i32)))
  (import "wasi:filesystem/types@0.3.1" "[async-lower][stream-read-0][method]descriptor.read-via-stream"
    (func $stream_read (param i32 i32 i32) (result i32)))
  (import "wasi:clocks/monotonic-clock@0.3.1" "wait-for" (func $wait_for (param i64)))
  (import "wasi:cli/stdout@0.3.1" "write-via-stream" (func $write_via_stream (param i32) (result i32)))
  (import "wasi:cli/stdout@0.3.1" "[stream-new-0]write-via-stream" (func $stream_new (result i64)))
  (import "wasi:cli/stdout@0.3.1" "[async-lower][stream-write-0]write-via-stream" (func $stream_write (param i32 i32 i32) (result i32)))
  (import "wasi:cli/stdout@0.3.1" "[stream-drop-writable-0]write-via-stream" (func $stream_drop_w (param i32)))
  (import "$root" "[waitable-set-new]" (func $ws_new (result i32)))
  (import "$root" "[waitable-join]" (func $join (param i32 i32)))
  (import "$root" "[waitable-set-wait]" (func $ws_wait (param i32 i32) (result i32)))
  (import "[export]wasi:cli/run@0.3.1" "[task-return]run" (func $task_return (param i32)))
  (memory (export "memory") 1)
  (data (i32.const 64) "hello.txt")
  (global $bump (mut i32) (i32.const 8192))

  ;; Block until waitable $h has an event; returns the event's payload (the stream op's result).
  (func $block (param $h i32) (result i32)
    (local $ws i32)
    (local.set $ws (call $ws_new))
    (call $join (local.get $h) (local.get $ws))
    (drop (call $ws_wait (local.get $ws) (i32.const 16)))
    (call $join (local.get $h) (i32.const 0))
    (i32.load (i32.const 20)))

  (func (export "[async-lift]wasi:cli/run@0.3.1#run") (result i32)
    (local $dir i32) (local $fd i32) (local $rs i32) (local $rc i32) (local $n i32) (local $s i64) (local $w i32)
    ;; a blocking 20 ms sleep: a sync-lowered `async func`
    (call $wait_for (i64.const 20000000))
    (call $get_dirs (i32.const 0))
    (if (i32.eqz (i32.load (i32.const 4))) (then unreachable))     ;; no preopens
    (local.set $dir (i32.load (i32.load (i32.const 0))))
    ;; open-at(dir, symlink-follow, "hello.txt", no open flags, read) -> result<descriptor, error-code> at 32
    (call $open_at (local.get $dir) (i32.const 1) (i32.const 64) (i32.const 9) (i32.const 0) (i32.const 1) (i32.const 32))
    (if (i32.load8_u (i32.const 32)) (then unreachable))           ;; err
    (local.set $fd (i32.load (i32.const 36)))
    (call $read_via_stream (local.get $fd) (i64.const 0) (i32.const 48))
    (local.set $rs (i32.load (i32.const 48)))
    ;; read until the stream is dropped (status 1), into 1024..
    (block $done
      (loop $more
        (local.set $rc (call $stream_read (local.get $rs) (i32.add (i32.const 1024) (local.get $n)) (i32.const 4096)))
        (if (i32.eq (local.get $rc) (i32.const -1)) (then (local.set $rc (call $block (local.get $rs)))))
        (local.set $n (i32.add (local.get $n) (i32.shr_u (local.get $rc) (i32.const 4))))
        (br_if $done (i32.eq (i32.and (local.get $rc) (i32.const 15)) (i32.const 1)))
        (br $more)))
    (local.set $s (call $stream_new))
    (local.set $w (i32.wrap_i64 (i64.shr_u (local.get $s) (i64.const 32))))
    (drop (call $write_via_stream (i32.wrap_i64 (local.get $s))))
    (local.set $rc (call $stream_write (local.get $w) (i32.const 1024) (local.get $n)))
    (if (i32.eq (local.get $rc) (i32.const -1)) (then (local.set $rc (call $block (local.get $w)))))
    (call $stream_drop_w (local.get $w))
    (call $task_return (i32.const 0))
    (i32.const 0))

  (func (export "[callback][async-lift]wasi:cli/run@0.3.1#run") (param i32 i32 i32) (result i32) unreachable)

  (func (export "cabi_realloc") (param $old i32) (param $osz i32) (param $al i32) (param $sz i32) (result i32)
    (local $p i32)
    (local.set $p (i32.and (i32.add (global.get $bump) (i32.sub (local.get $al) (i32.const 1)))
                           (i32.sub (i32.const 0) (local.get $al))))
    (global.set $bump (i32.add (local.get $p) (local.get $sz)))
    (local.get $p))
)
