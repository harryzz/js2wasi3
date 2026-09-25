//! The host ABI of a command (`wasi:cli/run@0.3.1`): every WASI call lives here. JS lowering
//! reaches it only through `Intrinsic::ConsoleWrite`.
//!
//! - **Imports**: a fixed list ([`IMPORTS`]), so function indices don't depend on what a program
//!   uses. Per stream (stdout, stderr): `write-via-stream` and the `stream<u8>` / `future` built-ins
//!   for it. Plus `waitable-set.new`, `waitable.join` and `task.return` of `run`.
//! - **Output** is appended to a per-stream buffer in linear memory while JS runs (grown by
//!   doubling through the bump `cabi_realloc`; nothing is freed). A stream is opened on first
//!   use: `stream.new`, then `write-via-stream` hands the readable end to the host and returns a
//!   future for the result.
//! - **Uncaught exceptions** (T12): `run` catches the JS exception tag around `main`, appends
//!   `Uncaught <text>` to stderr ([`Helper::Uncaught`]) and makes `run` return `err` (exit status 1,
//!   like node). Output written before the exception is kept.
//! - **Event loop** (design D3, callback lift): `run` creates the waitable set, runs `main`, then
//!   [`Helper::Flush`]. Flush advances each open stream: write the unwritten bytes (a write may
//!   complete partially, or block: then it waits on the set), then drop the writable end and read
//!   the future (blocking reads wait too). When nothing waits, `task.return` (`ok`, or `err` if a
//!   stream failed) and `EXIT`; otherwise `WAIT` on the set. The callback applies the event and
//!   flushes again.
//!
//! Per stream, [`G_PHASE`]: 0 unused, 1 open, 2 waiting for a write, 3 waiting for the future, 4 done,
//! 5 the reader went away (later output is dropped; the stream still gets closed, and `run` fails).

use wasm_encoder::{BlockType, Catch, EntityType, Function, ImportSection, MemArg, ValType};

use crate::abi;
use crate::runtime::{self, Cx};

/// Streams: stdout and stderr.
pub const STREAMS: u32 = 2;

/// One core import. The function index of an import is its position in [`IMPORTS`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Import {
    WriteViaStream(u32),
    StreamNew(u32),
    StreamWrite(u32),
    StreamDropWritable(u32),
    FutureRead(u32),
    FutureDropReadable(u32),
    WaitableSetNew,
    WaitableJoin,
    TaskReturn,
}

pub const IMPORTS: [Import; 15] = [
    Import::WriteViaStream(0),
    Import::StreamNew(0),
    Import::StreamWrite(0),
    Import::StreamDropWritable(0),
    Import::FutureRead(0),
    Import::FutureDropReadable(0),
    Import::WriteViaStream(1),
    Import::StreamNew(1),
    Import::StreamWrite(1),
    Import::StreamDropWritable(1),
    Import::FutureRead(1),
    Import::FutureDropReadable(1),
    Import::WaitableSetNew,
    Import::WaitableJoin,
    Import::TaskReturn,
];

fn import(i: Import) -> u32 {
    IMPORTS.iter().position(|x| *x == i).unwrap() as u32
}

impl Import {
    /// Core module and field names, as wit-component expects them.
    pub fn names(self) -> (&'static str, String) {
        let iface = |s: u32| {
            if s == 0 {
                "wasi:cli/stdout@0.3.1"
            } else {
                "wasi:cli/stderr@0.3.1"
            }
        };
        // Built-ins for the payload types of `write-via-stream`: the stream is 0, the future 1.
        match self {
            Import::WriteViaStream(s) => (iface(s), "write-via-stream".into()),
            Import::StreamNew(s) => (iface(s), "[stream-new-0]write-via-stream".into()),
            Import::StreamWrite(s) => (
                iface(s),
                "[async-lower][stream-write-0]write-via-stream".into(),
            ),
            Import::StreamDropWritable(s) => {
                (iface(s), "[stream-drop-writable-0]write-via-stream".into())
            }
            Import::FutureRead(s) => (
                iface(s),
                "[async-lower][future-read-1]write-via-stream".into(),
            ),
            Import::FutureDropReadable(s) => {
                (iface(s), "[future-drop-readable-1]write-via-stream".into())
            }
            Import::WaitableSetNew => ("$root", "[waitable-set-new]".into()),
            Import::WaitableJoin => ("$root", "[waitable-join]".into()),
            Import::TaskReturn => ("[export]wasi:cli/run@0.3.1", "[task-return]run".into()),
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::{I32, I64};
        match self {
            Import::WriteViaStream(_) => (vec![I32], vec![I32]),
            Import::StreamNew(_) => (vec![], vec![I64]),
            Import::StreamWrite(_) => (vec![I32; 3], vec![I32]),
            Import::FutureRead(_) => (vec![I32; 2], vec![I32]),
            Import::StreamDropWritable(_) | Import::FutureDropReadable(_) | Import::TaskReturn => {
                (vec![I32], vec![])
            }
            Import::WaitableSetNew => (vec![], vec![I32]),
            Import::WaitableJoin => (vec![I32; 2], vec![]),
        }
    }
}

/// Add the imports; `type_id` gives the type index of a signature.
pub fn imports(
    section: &mut ImportSection,
    type_id: &mut dyn FnMut(Vec<ValType>, Vec<ValType>) -> u32,
) {
    for i in IMPORTS {
        let (params, results) = i.signature();
        let (module, field) = i.names();
        section.import(
            module,
            &field,
            EntityType::Function(type_id(params, results)),
        );
    }
}

// Globals, relative to the host base: per stream `STREAM_GLOBALS` of them, then the set and the error flag.
/// Writable end of the stream.
const G_W: u32 = 0;
/// Readable end of the `write-via-stream` future.
const G_F: u32 = 1;
const G_BUF: u32 = 2;
const G_LEN: u32 = 3;
const G_CAP: u32 = 4;
/// Bytes of the buffer written so far.
const G_OFF: u32 = 5;
const G_PHASE: u32 = 6;
const STREAM_GLOBALS: u32 = 7;
/// The waitable set of `run`.
const G_WS: u32 = STREAMS * STREAM_GLOBALS;
/// A stream failed: `run` returns `err`.
const G_ERR: u32 = G_WS + 1;
/// Number of host globals (all `(mut i32)`, initially 0).
pub const GLOBALS: u32 = G_ERR + 1;

const PHASE_OPEN: i32 = 1;
const PHASE_WRITING: i32 = 2;
const PHASE_CLOSING: i32 = 3;
const PHASE_DONE: i32 = 4;
const PHASE_DROPPED: i32 = 5;

/// Canonical ABI constants.
const BLOCKED: i32 = -1;
const STATUS_DROPPED: i32 = 1;
const EVENT_STREAM_WRITE: i32 = 3;
const EVENT_FUTURE_READ: i32 = 4;
const CALLBACK_EXIT: i32 = 0;
const CALLBACK_WAIT: i32 = 2;

/// Where a future's `result<_, error-code>` is read to (2 bytes per stream, below `abi::HEAP_BASE`).
fn result_addr(stream: u32) -> i32 {
    2 * stream as i32
}

fn mem8(offset: u64) -> MemArg {
    MemArg {
        offset,
        align: 0,
        memory_index: 0,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(s: (ref $string))`: append the UTF-8 of `s` to a stream's buffer, opening it first.
    Append(u32),
    /// `(packed: i32)`: account for a finished write (`count << 4 | status`).
    Advance(u32),
    /// `(status: i32)`: the future was read (or dropped): record an error, drop it.
    Closed(u32),
    /// `() -> i32`: write the unwritten bytes until a write blocks (1) or all are written (0).
    /// Then the buffer is empty again and gets reused.
    Pump(u32),
    /// `() -> i32`: move a stream forward until it blocks (1) or is finished (0).
    Step(u32),
    /// `() -> i32`: step every stream, then the callback code (`WAIT` or `EXIT` after `task.return`).
    Flush,
    /// `(v: jsval)`: report an uncaught exception: `Uncaught <describe(v)>` to stderr, `run` fails.
    Uncaught,
    /// `() -> i32`: the `run` export (async lift with callback).
    Run,
    /// `(event, index, payload: i32) -> i32`: its callback.
    Callback,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Append(0) => "append_stdout",
            Helper::Append(_) => "append_stderr",
            Helper::Advance(0) => "advance_stdout",
            Helper::Advance(_) => "advance_stderr",
            Helper::Closed(0) => "closed_stdout",
            Helper::Closed(_) => "closed_stderr",
            Helper::Pump(0) => "pump_stdout",
            Helper::Pump(_) => "pump_stderr",
            Helper::Step(0) => "step_stdout",
            Helper::Step(_) => "step_stderr",
            Helper::Flush => "flush",
            Helper::Uncaught => "uncaught",
            Helper::Run => "run",
            Helper::Callback => "run_callback",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::I32;
        match self {
            Helper::Append(_) => (vec![runtime::string_ref()], vec![]),
            Helper::Advance(_) | Helper::Closed(_) => (vec![I32], vec![]),
            Helper::Uncaught => (vec![runtime::jsval()], vec![]),
            Helper::Pump(_) | Helper::Step(_) | Helper::Flush | Helper::Run => (vec![], vec![I32]),
            Helper::Callback => (vec![I32; 3], vec![I32]),
        }
    }

    /// `base`: global index of the first host global. `main`: function index of `main`.
    pub fn body(self, cx: &mut dyn Cx, base: u32, main: u32) -> Function {
        let g = |stream: u32, k: u32| base + stream * STREAM_GLOBALS + k;
        let ws = base + G_WS;
        let err = base + G_ERR;
        match self {
            Helper::Append(st) => {
                let realloc = cx.helper(runtime::Helper::Abi(abi::Helper::Realloc));
                let len = cx.helper(runtime::Helper::Abi(abi::Helper::Utf8Len));
                let encode = cx.helper(runtime::Helper::Abi(abi::Helper::Utf8Encode));
                let pump = cx.helper(runtime::Helper::Host(Helper::Pump(st)));
                append(st, &g, len, encode, realloc, pump)
            }
            Helper::Pump(st) => {
                let advance = cx.helper(runtime::Helper::Host(Helper::Advance(st)));
                pump(st, &g, ws, advance)
            }
            Helper::Advance(st) => {
                const PACKED: u32 = 0;
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.global_get(g(st, G_OFF))
                    .local_get(PACKED)
                    .i32_const(4)
                    .i32_shr_u()
                    .i32_add()
                    .global_set(g(st, G_OFF));
                // The reader went away: nothing more can be written.
                s.local_get(PACKED)
                    .i32_const(0xf)
                    .i32_and()
                    .i32_const(STATUS_DROPPED)
                    .i32_eq();
                s.if_(BlockType::Empty);
                s.global_get(g(st, G_LEN)).global_set(g(st, G_OFF));
                s.i32_const(PHASE_DROPPED).global_set(g(st, G_PHASE));
                s.i32_const(1).global_set(err);
                s.end();
                s.end();
                f
            }
            Helper::Closed(st) => {
                const STATUS: u32 = 0;
                let mut f = Function::new([]);
                let mut s = f.instructions();
                // Dropped without a value, or `err(_)` (discriminant 1).
                s.local_get(STATUS)
                    .i32_const(0xf)
                    .i32_and()
                    .i32_const(0)
                    .i32_ne();
                s.i32_const(result_addr(st)).i32_load8_u(mem8(0));
                s.i32_or();
                s.if_(BlockType::Empty);
                s.i32_const(1).global_set(err);
                s.end();
                s.global_get(g(st, G_F))
                    .call(import(Import::FutureDropReadable(st)));
                s.i32_const(PHASE_DONE).global_set(g(st, G_PHASE));
                s.end();
                f
            }
            Helper::Step(st) => {
                let pump = cx.helper(runtime::Helper::Host(Helper::Pump(st)));
                let closed = cx.helper(runtime::Helper::Host(Helper::Closed(st)));
                step(st, &g, ws, pump, closed)
            }
            Helper::Flush => {
                let steps: Vec<u32> = (0..STREAMS)
                    .map(|st| cx.helper(runtime::Helper::Host(Helper::Step(st))))
                    .collect();
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.call(steps[0]);
                for step in &steps[1..] {
                    s.call(*step).i32_or();
                }
                s.if_(BlockType::Empty);
                s.global_get(ws)
                    .i32_const(4)
                    .i32_shl()
                    .i32_const(CALLBACK_WAIT)
                    .i32_or()
                    .return_();
                s.end();
                // `result<_, _>`: 0 is `ok`, 1 is `err`.
                s.global_get(err).call(import(Import::TaskReturn));
                s.i32_const(CALLBACK_EXIT);
                s.end();
                f
            }
            Helper::Uncaught => {
                let describe = cx.helper(runtime::Helper::Describe);
                let concat = cx.helper(runtime::Helper::Concat);
                let append = cx.helper(runtime::Helper::Host(Helper::Append(1)));
                let (prefix, newline) = (cx.str("Uncaught "), cx.str("\n"));
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.global_get(prefix);
                s.local_get(0).call(describe).call(concat);
                s.global_get(newline).call(concat);
                s.call(append);
                s.i32_const(1).global_set(err);
                s.end();
                f
            }
            Helper::Run => {
                let flush = cx.helper(runtime::Helper::Host(Helper::Flush));
                let uncaught = cx.helper(runtime::Helper::Host(Helper::Uncaught));
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.call(import(Import::WaitableSetNew)).global_set(ws);
                s.block(BlockType::Result(runtime::jsval()));
                s.try_table(
                    BlockType::Empty,
                    [Catch::One {
                        tag: runtime::TAG,
                        label: 0,
                    }],
                );
                s.call(main);
                s.call(flush).return_();
                s.end();
                s.unreachable();
                s.end();
                s.call(uncaught);
                s.call(flush);
                s.end();
                f
            }
            Helper::Callback => {
                const EVENT: u32 = 0;
                const INDEX: u32 = 1;
                const PAYLOAD: u32 = 2;
                let flush = cx.helper(runtime::Helper::Host(Helper::Flush));
                let mut f = Function::new([]);
                let mut s = f.instructions();
                for st in 0..STREAMS {
                    let advance = cx.helper(runtime::Helper::Host(Helper::Advance(st)));
                    let closed = cx.helper(runtime::Helper::Host(Helper::Closed(st)));
                    // A blocked write finished.
                    s.local_get(EVENT).i32_const(EVENT_STREAM_WRITE).i32_eq();
                    s.local_get(INDEX).global_get(g(st, G_W)).i32_eq().i32_and();
                    s.global_get(g(st, G_PHASE))
                        .i32_const(PHASE_WRITING)
                        .i32_eq()
                        .i32_and();
                    s.if_(BlockType::Empty);
                    s.i32_const(PHASE_OPEN).global_set(g(st, G_PHASE));
                    s.local_get(PAYLOAD).call(advance);
                    s.end();
                    // The future's value arrived.
                    s.local_get(EVENT).i32_const(EVENT_FUTURE_READ).i32_eq();
                    s.local_get(INDEX).global_get(g(st, G_F)).i32_eq().i32_and();
                    s.global_get(g(st, G_PHASE))
                        .i32_const(PHASE_CLOSING)
                        .i32_eq()
                        .i32_and();
                    s.if_(BlockType::Empty);
                    s.local_get(PAYLOAD).call(closed);
                    s.end();
                }
                s.call(flush);
                s.end();
                f
            }
        }
    }
}

/// `$append(str)`: see [`Helper::Append`].
fn append(
    st: u32,
    g: &dyn Fn(u32, u32) -> u32,
    len: u32,
    encode: u32,
    realloc: u32,
    pump: u32,
) -> Function {
    const STR: u32 = 0;
    const N: u32 = 1;
    const NEED: u32 = 2;
    const CAP: u32 = 3;
    const PAIR: u32 = 4;
    let mut f = Function::new([(3, ValType::I32), (1, ValType::I64)]);
    let mut s = f.instructions();
    s.global_get(g(st, G_PHASE))
        .i32_const(PHASE_DROPPED)
        .i32_eq();
    s.if_(BlockType::Empty);
    s.return_();
    s.end();
    // Open on first use: the readable end goes to the host, which returns the result future.
    s.global_get(g(st, G_PHASE)).i32_eqz();
    s.if_(BlockType::Empty);
    s.call(import(Import::StreamNew(st))).local_set(PAIR);
    s.local_get(PAIR)
        .i64_const(32)
        .i64_shr_u()
        .i32_wrap_i64()
        .global_set(g(st, G_W));
    s.local_get(PAIR)
        .i32_wrap_i64()
        .call(import(Import::WriteViaStream(st)))
        .global_set(g(st, G_F));
    s.i32_const(PHASE_OPEN).global_set(g(st, G_PHASE));
    s.end();
    // need = len + utf8_len(str); grow to max(need, 2 * cap, 256) if needed.
    s.local_get(STR).call(len).local_tee(N);
    s.global_get(g(st, G_LEN)).i32_add().local_set(NEED);
    s.local_get(NEED).global_get(g(st, G_CAP)).i32_gt_u();
    s.if_(BlockType::Empty);
    {
        s.global_get(g(st, G_CAP))
            .i32_const(1)
            .i32_shl()
            .local_set(CAP);
        s.local_get(CAP)
            .i32_const(256)
            .local_get(CAP)
            .i32_const(256)
            .i32_gt_u()
            .select();
        s.local_set(CAP);
        s.local_get(NEED)
            .local_get(CAP)
            .local_get(NEED)
            .local_get(CAP)
            .i32_gt_u()
            .select();
        s.local_set(CAP);
        // realloc copies the old contents.
        s.global_get(g(st, G_BUF))
            .global_get(g(st, G_LEN))
            .i32_const(1)
            .local_get(CAP)
            .call(realloc)
            .global_set(g(st, G_BUF));
        s.local_get(CAP).global_set(g(st, G_CAP));
    }
    s.end();
    s.local_get(STR)
        .global_get(g(st, G_BUF))
        .global_get(g(st, G_LEN))
        .i32_add()
        .call(encode);
    s.local_get(NEED).global_set(g(st, G_LEN));
    // Write now if the host takes it (a blocked write finishes in the event loop).
    s.call(pump).drop();
    s.end();
    f
}

/// `$pump() -> i32`: see [`Helper::Pump`].
fn pump(st: u32, g: &dyn Fn(u32, u32) -> u32, ws: u32, advance: u32) -> Function {
    const RC: u32 = 0;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.global_get(g(st, G_PHASE))
        .i32_const(PHASE_WRITING)
        .i32_eq();
    s.if_(BlockType::Empty);
    s.i32_const(1).return_();
    s.end();
    s.global_get(g(st, G_PHASE)).i32_const(PHASE_OPEN).i32_ne();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.loop_(BlockType::Empty);
    s.global_get(g(st, G_OFF))
        .global_get(g(st, G_LEN))
        .i32_ge_u();
    s.if_(BlockType::Empty);
    // All written: reuse the buffer.
    s.i32_const(0).global_set(g(st, G_OFF));
    s.i32_const(0).global_set(g(st, G_LEN));
    s.i32_const(0).return_();
    s.end();
    s.global_get(g(st, G_W));
    s.global_get(g(st, G_BUF))
        .global_get(g(st, G_OFF))
        .i32_add();
    s.global_get(g(st, G_LEN))
        .global_get(g(st, G_OFF))
        .i32_sub();
    s.call(import(Import::StreamWrite(st))).local_tee(RC);
    s.i32_const(BLOCKED).i32_eq();
    s.if_(BlockType::Empty);
    s.global_get(g(st, G_W))
        .global_get(ws)
        .call(import(Import::WaitableJoin));
    s.i32_const(PHASE_WRITING).global_set(g(st, G_PHASE));
    s.i32_const(1).return_();
    s.end();
    s.local_get(RC).call(advance);
    s.br(0);
    s.end();
    s.unreachable();
    s.end();
    f
}

/// `$step() -> i32`: see [`Helper::Step`].
fn step(st: u32, g: &dyn Fn(u32, u32) -> u32, ws: u32, pump: u32, closed: u32) -> Function {
    const RC: u32 = 0;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    // Waiting for the future: blocked.
    s.global_get(g(st, G_PHASE))
        .i32_const(PHASE_CLOSING)
        .i32_eq();
    s.if_(BlockType::Empty);
    s.i32_const(1).return_();
    s.end();
    // Open: write the rest first. Dropped: close right away. Otherwise (unused, done, waiting
    // for a write) `pump` says whether it's blocked.
    s.global_get(g(st, G_PHASE)).i32_const(PHASE_OPEN).i32_eq();
    s.if_(BlockType::Empty);
    s.call(pump);
    s.if_(BlockType::Empty);
    s.i32_const(1).return_();
    s.end();
    s.else_();
    s.global_get(g(st, G_PHASE))
        .i32_const(PHASE_DROPPED)
        .i32_ne();
    s.if_(BlockType::Empty);
    s.call(pump).return_();
    s.end();
    s.end();
    // All written: close the stream, then read the future.
    s.global_get(g(st, G_W))
        .call(import(Import::StreamDropWritable(st)));
    s.global_get(g(st, G_F))
        .i32_const(result_addr(st))
        .call(import(Import::FutureRead(st)))
        .local_tee(RC);
    s.i32_const(BLOCKED).i32_eq();
    s.if_(BlockType::Empty);
    s.global_get(g(st, G_F))
        .global_get(ws)
        .call(import(Import::WaitableJoin));
    s.i32_const(PHASE_CLOSING).global_set(g(st, G_PHASE));
    s.i32_const(1).return_();
    s.end();
    s.local_get(RC).call(closed);
    s.i32_const(0);
    s.end();
    f
}
