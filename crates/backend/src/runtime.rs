//! The JS value runtime on WasmGC (design D1), emitted only when a module uses `jsval`.
//!
//! - A JS value is `(ref null eq)`. `null` (the Wasm null) is never a JS value: it only fills
//!   locals before their first assignment.
//! - Numbers: `i31ref` when the value is an integer in [-2^30, 2^30) and not -0, otherwise a
//!   `$number` struct holding the `f64`.
//! - Strings: `$string`, an `(array (mut i16))` of UTF-16 code units. String constants are
//!   immutable globals built with `array.new_fixed` (see [`Cx::string`]).
//! - `undefined`, `null`, `false`, `true`: four `$oddball` singletons (immutable globals 0–3)
//!   whose field is the kind below. Identity (`ref.eq`) compares them.
//! - Objects (T13): `$object`, a dictionary with a prototype; errors are objects of class
//!   `error`. See [`crate::object`] for the layout, the built-in prototypes and the helpers.
//!   `console.log` shows an error as `[name: message]` (node's format for an error without a stack).
//! - Types: one `rec` group `[$number, $oddball, $string, $i64s, $object, $keys, $vals, $bytes,
//!   $i32s, $code, $function, $cell_f64, $cell, $symbol, $array, $iter, $accessor, $promise,
//!   $reaction, $generator, $resume]` at type indices 0–20 (`$i64s` holds the number → string
//!   tables, see [`crate::numstr`]; `$keys`…`$i32s` are an object's arrays).
//! - Functions (T14): `$function` is a subtype of `$object` (so every object helper takes it) with
//!   the code to run and the environment (captured cells and values). See [`crate::func`].
//! - Exceptions (design D2): one tag, [`TAG`], `(param (ref null eq))`: `throw` throws the JS value.
//!   Runtime errors (reading a property of `null`) throw a TypeError the same way.
//!
//! The helpers know these values. Objects (T13) extend them.

use js2wasi3_ir::{ErrorKind, TypeofTag};
use wasm_encoder::{
    ArrayType, BlockType, CompositeInnerType, CompositeType, ConstExpr, FieldType, Function,
    GlobalSection, GlobalType, HeapType, Instruction, InstructionSink, RefType, StorageType,
    StructType, SubType, TypeSection, ValType,
};

use crate::{
    abi, array, asyncfn, asyncgen, bigint, func, generator, host, inspect, iter, mapset, numstr,
    object, promise, props, timers, typed, wrapper,
};

pub const T_NUMBER: u32 = 0;
pub const T_ODDBALL: u32 = 1;
pub const T_STRING: u32 = 2;
pub const T_I64S: u32 = 3;
pub const T_OBJECT: u32 = 4;
/// `(array (mut (ref null $string)))`: property keys (also key lists for `for-in`).
pub const T_KEYS: u32 = 5;
/// `(array (mut (ref null eq)))`: property values.
pub const T_VALS: u32 = 6;
/// `(array (mut i8))`: property attributes.
pub const T_BYTES: u32 = 7;
/// `(array (mut i32))`: an object's hash index.
pub const T_I32S: u32 = 8;
/// `(func (param eqref eqref eqref) (result eqref))`: what calls through a function value run
/// (the function object, `this`, the argument array as a `$vals`).
pub const T_CODE: u32 = 9;
/// `$function`: a subtype of `$object` with `code: (ref $code)` and `env: (ref null $vals)` (T14).
pub const T_FUNCTION: u32 = 10;
/// `(struct (mut f64))`: a captured `f64` variable.
pub const T_CELL_F64: u32 = 11;
/// `(struct (mut (ref null eq)))`: a captured `jsval` variable (Wasm null: not initialized yet).
pub const T_CELL: u32 = 12;
/// `$symbol`: a final subtype of `$string` holding the description (T15). Compared by identity.
pub const T_SYMBOL: u32 = 13;
/// `$array`: a final subtype of `$object` with `elems: mut (ref $vals)` (holes: Wasm null) and
/// `length: mut i32` (T15).
pub const T_ARRAY: u32 = 14;
/// `$iter`: the state of an iteration (T15, see [`crate::iter`]).
pub const T_ITER: u32 = 15;
/// `$accessor { get: mut eqref, set: mut eqref }`: the value slot of an accessor property (T16,
/// see [`crate::object`]). Never a JS value. Absent functions are `undefined`.
pub const T_ACCESSOR: u32 = 16;
/// `$promise`: a final subtype of `$object` with `state: mut i32`, `result: mut eqref`,
/// `reactions: mut (ref null $vals)`, `count: mut i32` and `handled: mut i32` (T17, see
/// [`crate::promise`]).
pub const T_PROMISE: u32 = 17;
/// `$reaction { derived: (ref null $promise), on_fulfilled: eqref, on_rejected: eqref }`: a
/// PromiseReaction pair (T17). Never a JS value.
pub const T_REACTION: u32 = 18;
/// `$generator`: a final subtype of `$object` with `resume: (ref $resume)`, `frame: (ref $vals)`,
/// `point`, `status` and `yielded` (`mut i32`s) (T18, see [`crate::generator`]).
pub const T_GENERATOR: u32 = 19;
/// `(func (param eqref i32 eqref) (result eqref))`: a generator's resume function (T18).
pub const T_RESUME: u32 = 20;
/// `$timer`: a final subtype of `$object` (the Timeout object) with `deadline: mut i64`,
/// `interval: i64`, `callback: eqref`, `args: (ref $vals)` (T19, see [`crate::timers`]); T25:
/// `delay: i64`, `id: i32`, `flags: mut i32`. Immediates are `$timer`s too.
pub const T_TIMER: u32 = 21;
/// `$map`: a final subtype of `$object` with `table: mut (ref $mtable)` (T25, see [`crate::mapset`]).
pub const T_MAP: u32 = 22;
/// `$mtable`: a `Map`/`Set` hash table (T25). Never a JS value.
pub const T_MTABLE: u32 = 23;
/// `$mapiter`: a final subtype of `$object` with `table: mut (ref null $mtable)`, `pos: mut i32`,
/// `mode: i32` (T25).
pub const T_MAPITER: u32 = 24;
/// `$buffer`: a final subtype of `$object` (an `ArrayBuffer`) with `data: (ref $bytes)` (T20, see
/// [`crate::typed`]).
pub const T_BUFFER: u32 = 25;
/// `$typed`: a final subtype of `$object` (a typed array) with `kind: i32`, `buffer: (ref
/// $buffer)`, `offset: i32` (bytes), `length: i32` (elements) (T20).
pub const T_TYPED: u32 = 26;
/// `$wrapper`: a final subtype of `$object` (a `String`/`Number`/`Boolean`/`Symbol` wrapper
/// object) with `prim: jsval` (T30, see [`crate::wrapper`]).
pub const T_WRAPPER: u32 = 27;
/// `$strbuf { buf: mut (ref null $string), len: mut i32, flat: mut (ref null $string) }` (T78): a
/// string being appended to (`str_buf_append`): its first `len` units of `buf` (room to grow), and
/// that string once made (`str_buf_get`; null after an append). Only ever in a variable's local.
pub const T_STRBUF: u32 = 28;
/// `$bigint`: a final `(array (mut i32))`, sign then magnitude limbs (T40, see [`crate::bigint`]).
/// Only in a module that can make a BigInt ([`Cx::bigint`]): the last type of the group then.
pub const T_BIGINT: u32 = 29;
/// Number of runtime types before the function types (one more with `$bigint`).
pub const TYPES: u32 = 29;

/// The exception tag: `(tag (param (ref null eq)))`, the only tag (index 0, not exported).
pub const TAG: u32 = 0;

pub const G_UNDEFINED: u32 = 0;
pub const G_NULL: u32 = 1;
pub const G_FALSE: u32 = 2;
pub const G_TRUE: u32 = 3;
/// `(mut (ref null $i64s))`: the number → string tables, loaded on first use.
pub const G_TABLES: u32 = 4;
/// An oddball that stands for a built-in method in a prototype (see [`crate::object`]).
pub const G_BUILTIN: u32 = 5;
/// `(mut (ref null $object))` per [`js2wasi3_ir::Builtin`]: its prototype, created on first use.
pub const G_PROTOS: u32 = 6;
/// `(ref $string)`: the well-known symbol `Symbol.iterator` (a `$symbol`, T15).
pub const G_SYM_ITERATOR: u32 = G_PROTOS + js2wasi3_ir::Builtin::ALL.len() as u32;
/// `(ref $string)`: a private symbol keying an iterator object's state (T15, see [`crate::iter`]).
pub const G_SYM_STATE: u32 = G_SYM_ITERATOR + 1;
/// `(mut (ref null $object))` per [`js2wasi3_ir::ErrorKind`]: the native error constructor as a
/// function object (T16), created on first use.
pub const G_ERROR_CTORS: u32 = G_SYM_STATE + 1;
/// `(mut (ref null $object))`: the `Promise` constructor (T17), created on first use.
pub const G_PROMISE_CTOR: u32 = G_ERROR_CTORS + js2wasi3_ir::ErrorKind::ALL.len() as u32;
/// `(mut (ref null $object))`: the `queueMicrotask` function (T17), created on first use.
pub const G_QUEUE_MICROTASK: u32 = G_PROMISE_CTOR + 1;
/// `(mut (ref null $vals))`: the job queue, 4 slots per job (see [`crate::promise`]).
pub const G_JOBS: u32 = G_QUEUE_MICROTASK + 1;
/// `(mut i32)`: the first slot of the next job, and one past the last job's slots.
pub const G_JOB_HEAD: u32 = G_JOBS + 1;
pub const G_JOB_TAIL: u32 = G_JOB_HEAD + 1;
/// `(mut (ref null $vals))` and `(mut i32)`: promises rejected while unhandled, and their count.
pub const G_REJECTED: u32 = G_JOB_TAIL + 1;
pub const G_REJECTED_N: u32 = G_REJECTED + 1;
/// `(mut (ref null $object))` per timer function (T19, `js2wasi3_ir::TIMER_NAMES`), created on
/// first use.
pub const G_TIMER_FNS: u32 = G_REJECTED_N + 1;
/// `(mut (ref null $vals))` and `(mut i32)`: the active timers (`$timer`s), and their count.
pub const G_TIMERS: u32 = G_TIMER_FNS + js2wasi3_ir::TIMER_NAMES.len() as u32;
pub const G_TIMERS_N: u32 = G_TIMERS + 1;
/// `(mut i64)`: the event loop's time (ns, `monotonic-clock.now`), set when it wakes up for
/// timers.
pub const G_LOOP_NOW: u32 = G_TIMERS_N + 1;
/// `(mut i32)`: the outstanding `wait-until` subtask (0: none), and `(mut i64)`: its deadline.
pub const G_WAIT: u32 = G_LOOP_NOW + 1;
pub const G_WAIT_AT: u32 = G_WAIT + 1;
/// `(mut i32)`: the last identity hash handed out (T25, see [`crate::mapset`]).
pub const G_ID_HASH: u32 = G_WAIT_AT + 1;
/// `(mut (ref null $object))` per [`js2wasi3_ir::Builtin`]: the constructor as a value (T25),
/// created on first use.
/// `(mut (ref null $vals))`: the well-known symbols other than `Symbol.iterator` (T25), made on
/// first use ([`Helper::WellKnownSymbol`]).
pub const G_WELL_KNOWN: u32 = G_ID_HASH + 1;
/// `console.log`'s circular-reference tracking (T25, see `crate::inspect`): the objects being
/// formatted (`(mut (ref null $vals))`, `(mut i32)`), and those found in a cycle.
pub const G_INSPECT_SEEN: u32 = G_WELL_KNOWN + 1;
pub const G_INSPECT_SEEN_N: u32 = G_INSPECT_SEEN + 1;
pub const G_INSPECT_CIRC: u32 = G_INSPECT_SEEN_N + 1;
pub const G_INSPECT_CIRC_N: u32 = G_INSPECT_CIRC + 1;
pub const G_CTORS: u32 = G_INSPECT_CIRC_N + 1;
/// `(mut (ref null $object))` per [`js2wasi3_ir::Namespace`]: `Math`/`JSON` as values (T25).
pub const G_NAMESPACES: u32 = G_CTORS + js2wasi3_ir::Builtin::ALL.len() as u32;
/// T25: `(mut (ref null $vals))` and `(mut i32)`: the queued immediates (`$timer`s); the same
/// for the `process.nextTick` queue (callback, arguments pairs); `(mut i32)`: the last timer id.
pub const G_IMMEDIATES: u32 = G_NAMESPACES + js2wasi3_ir::Namespace::ALL.len() as u32;
pub const G_IMMEDIATES_N: u32 = G_IMMEDIATES + 1;
pub const G_TICKS: u32 = G_IMMEDIATES_N + 1;
pub const G_TICKS_N: u32 = G_TICKS + 1;
pub const G_TIMER_ID: u32 = G_TICKS_N + 1;
/// `(mut i32)` (T34): an index key was added to `Array.prototype` or `Object.prototype`, or an
/// array's prototype was replaced: array writes to absent elements then look at the prototype chain.
pub const G_INDEX_PROTO: u32 = G_TIMER_ID + 1;
/// `(mut (ref null $object))` (T27): the built-in `RegExp.prototype.exec` as installed, so the
/// prelude can skip the lookup-and-call of RegExpExec while it's still in place.
pub const G_REGEXP_EXEC: u32 = G_INDEX_PROTO + 1;
/// `(mut (ref null $i32s))` (T74): the inline caches of constant-key property reads, one entry index
/// per site (`object::Helper::GetNamed`), grown on demand.
pub const G_IC: u32 = G_REGEXP_EXEC + 1;
/// `(mut (ref null $keys))` (T75): the property keys of the integers 0 to [`NUM_KEYS`] − 1, made on
/// first use (`object::Helper::ToKey`).
pub const G_NUM_KEYS: u32 = G_IC + 1;
/// How many integer keys `G_NUM_KEYS` holds.
pub const NUM_KEYS: i32 = 1024;
/// `(mut (ref null $vals))` (T76): `RegExp.prototype` as built, for `object::Helper::RegExpPlain`:
/// per key of [`crate::object::REGEXP_PLAIN_KEYS`] its entry index (an i31), its value (an
/// accessor's getter) and its attributes (an i31).
pub const G_REGEXP_SNAP: u32 = G_NUM_KEYS + 1;
/// `(mut i32)` (T90): starts at 1 and is bumped (never to 0) by every key added to, and every new
/// prototype of, an object flagged `object::IS_PROTO`.
pub const G_EPOCH: u32 = G_REGEXP_SNAP + 1;
/// `(mut (ref null $vals))` (T90): per constant-key write site (`object::Helper::SetNamed`, numbered
/// apart from the reads) two entries: the prototype from which the chain was found to have no such key,
/// and the `G_EPOCH` then (an i31; null: none). Grown on demand from 16 entries: it's live in every
/// collection, and wasmtime's copying collector runs often on a small heap.
pub const G_SET_IC: u32 = G_EPOCH + 1;
/// `(mut (ref null $string))` and `(mut i32)` (T94): the last string `object::Helper::Hash` hashed and its
/// hash. The same string object again (`ref.eq`) gives the stored hash: a key is hashed once across the
/// prototype levels of one lookup and across back-to-back uses (`m.get(k)` then `m.set(k, …)`). Strings
/// visible to JS are never mutated, so the hash stays right.
pub const G_HASH_KEY: u32 = G_SET_IC + 1;
pub const G_HASH_VAL: u32 = G_HASH_KEY + 1;
/// `(mut (ref null $vals))` (T97): for a typed array's `.length` without its getter (`object::Helper::GetLength`):
/// `[0]` `length`'s entry index in `%TypedArray%.prototype` as built (an i31) and `[1]` its getter (snapshot,
/// `object::Helper::TypedLengthSnapshot`), then two cached prototypes with the `G_EPOCH` at which their chain to
/// `%TypedArray%.prototype` had no other `length` (`[2]`, `[3]` and `[4]`, `[5]`; `object::Helper::TypedLengthFill`).
pub const G_TLEN: u32 = G_HASH_VAL + 1;
/// `(mut (ref null $vals))` (T99): per constant-key read site (`object::Helper::GetNamed`'s numbering) four
/// entries for a key found on a prototype: the receiver's prototype `P`, the holder `H`, the `G_EPOCH` then (an
/// i31) and the entry index in `H` (an i31); null when unfilled. Grown on demand from 16 sites
/// (`object::Helper::LookupNamedCached`).
pub const G_PIC: u32 = G_TLEN + 1;
/// Number of runtime globals before the user globals, without the BigInt ones.
pub const GLOBALS: u32 = G_PIC + 1;
/// `(mut (ref null $object))` (T40): `BigInt.prototype` and `BigInt` as a value, after the others,
/// only in a module that can make a BigInt ([`Cx::bigint`]).
pub const G_BIGINT_PROTO: u32 = GLOBALS;
pub const G_BIGINT_CTOR: u32 = GLOBALS + 1;
/// How many globals the BigInt support adds.
pub const BIGINT_GLOBALS: u32 = 2;
/// `(mut (ref null $object))` (T45): `SuppressedError.prototype` and `SuppressedError` as a value,
/// after the BigInt ones (present too then), only when [`js2wasi3_ir::Module::suppressed`].
pub const G_SUPPRESSED_PROTO: u32 = GLOBALS + BIGINT_GLOBALS;
pub const G_SUPPRESSED_CTOR: u32 = G_SUPPRESSED_PROTO + 1;
/// `(mut (ref null $object))` (T45): the prototypes and constructors of `DisposableStack` and
/// `AsyncDisposableStack`, after `SuppressedError`'s (present too then), only when
/// [`js2wasi3_ir::Module::stacks`].
pub const G_STACK_PROTOS: u32 = G_SUPPRESSED_CTOR + 1;
pub const G_STACK_CTORS: u32 = G_STACK_PROTOS + 2;
/// `(mut (ref null $object))` (T48): the prototypes and constructors of `BigInt64Array` and
/// `BigUint64Array`, after the stacks' (present too then), only when
/// [`js2wasi3_ir::Module::big_typed`].
pub const G_BIG_TYPED_PROTOS: u32 = G_STACK_CTORS + 2;
pub const G_BIG_TYPED_CTORS: u32 = G_BIG_TYPED_PROTOS + 2;
/// How many runtime globals come before the user's: the BigInt ones, `SuppressedError`'s, the
/// stacks' and the BigInt arrays' when used (each group brings the ones before it).
pub fn runtime_globals(bigint: bool, suppressed: bool, stacks: bool, big_typed: bool) -> u32 {
    let stacks = stacks || big_typed;
    GLOBALS
        + (bigint || suppressed || stacks) as u32 * BIGINT_GLOBALS
        + (suppressed || stacks) as u32 * 2
        + stacks as u32 * 4
        + big_typed as u32 * 4
}

/// The global holding built-in `b`'s prototype.
pub fn proto_global(b: js2wasi3_ir::Builtin) -> u32 {
    match b {
        js2wasi3_ir::Builtin::BigInt => G_BIGINT_PROTO,
        js2wasi3_ir::Builtin::Error(js2wasi3_ir::ErrorKind::SuppressedError) => G_SUPPRESSED_PROTO,
        js2wasi3_ir::Builtin::DisposableStack => G_STACK_PROTOS,
        js2wasi3_ir::Builtin::AsyncDisposableStack => G_STACK_PROTOS + 1,
        js2wasi3_ir::Builtin::Typed(k) if k >= js2wasi3_ir::TYPED_BIGINT64 => {
            G_BIG_TYPED_PROTOS + (k - js2wasi3_ir::TYPED_BIGINT64) as u32
        }
        _ => G_PROTOS + b.index(),
    }
}

/// The global holding built-in `b`'s constructor as a value (T25).
pub fn ctor_global(b: js2wasi3_ir::Builtin) -> u32 {
    match b {
        js2wasi3_ir::Builtin::BigInt => G_BIGINT_CTOR,
        js2wasi3_ir::Builtin::DisposableStack => G_STACK_CTORS,
        js2wasi3_ir::Builtin::AsyncDisposableStack => G_STACK_CTORS + 1,
        js2wasi3_ir::Builtin::Typed(k) if k >= js2wasi3_ir::TYPED_BIGINT64 => {
            G_BIG_TYPED_CTORS + (k - js2wasi3_ir::TYPED_BIGINT64) as u32
        }
        _ => G_CTORS + b.index(),
    }
}

/// `$oddball` kinds: the global index of each singleton equals its kind.
const K_UNDEFINED: i32 = 0;
const K_NULL: i32 = 1;
const K_FALSE: i32 = 2;
const K_TRUE: i32 = 3;
const K_BUILTIN: i32 = 4;

/// Smallest and one past the largest integer kept as an `i31ref`.
const I31_MIN: i32 = -(1 << 30);
const I31_END: i32 = 1 << 30;

/// What helper bodies need from the emitter.
pub trait Cx {
    /// Function index of a helper (emitted on demand).
    fn helper(&mut self, h: Helper) -> u32;
    /// Global index of an interned string constant.
    fn string(&mut self, units: &[u16]) -> u32;
    /// Index of the passive data segment with the number → string tables.
    fn tables_data(&mut self) -> u32;
    /// Global index of the ABI bump pointer (only in modules with string exports).
    fn bump_global(&mut self) -> u32;
    /// Global index of the first host global (commands only, see [`crate::host`]).
    fn host_globals(&mut self) -> u32;
    /// Global index of the first of `abi::LIB_GLOBALS` (T66: in a command, after the host's).
    fn lib_globals(&mut self) -> u32;
    /// A command whose event loop waits for non-blocking imports (T66).
    fn command_waits(&mut self) -> bool;
    /// Function index of `$to_int32` (JS ToInt32 of an `f64`).
    fn to_int32(&mut self) -> u32;
    /// The host imports (commands; libraries with async exports, T20).
    fn imports(&mut self) -> crate::host::Imports;
    /// A user world's import `i` (T20 slice 5): its function index, signature and unwrap
    /// function (a top-level `result`).
    fn user_import(&mut self, i: u32) -> (u32, js2wasi3_ir::AbiSig, Option<u32>);
    /// How many imports the user world has.
    fn user_import_count(&mut self) -> u32;
    /// The id of a boundary `future` or `stream` type (T20 slice 6).
    fn payload(&mut self, p: &js2wasi3_ir::Payload) -> u32;
    /// The function index of one of its intrinsics.
    fn payload_import(&mut self, id: u32, op: crate::abi::PayloadOp) -> u32;
    /// Its value type.
    fn payload_elem(&mut self, id: u32) -> Option<js2wasi3_ir::WitType>;
    fn payload_count(&mut self) -> u32;
    /// The cancel-read import of boundary type `id` (T66: only a command waiting for imports).
    fn payload_cancel(&mut self, id: u32) -> Option<u32>;
    /// Is it a `stream` (T20 slice 6b)?
    fn payload_is_stream(&mut self, id: u32) -> bool;
    /// Function indices of the prelude's `$$StreamReader` and `$$StreamPump` (T20 slice 6b):
    /// set when a boundary type is a stream.
    fn stream_funcs(&mut self) -> (u32, u32);
    /// Function indices of the prelude's `$$StreamReadAll`, `$$StreamDefer` and `$$StreamFlush`
    /// (T59: a command's blocking WASI imports); for the last two, whether they have a result.
    fn blocking_stream_funcs(&mut self) -> (u32, (u32, bool), (u32, bool));
    /// Does the library have top-level code (`init`, T20)?
    fn has_init(&mut self) -> bool;
    /// A library's `init`: its function index, whether it may throw (or leave a rejection), and
    /// whether it may queue jobs.
    fn init_func(&mut self) -> (u32, bool, bool);
    /// Function index of the command's `main`, and whether it returns a value (T19: with a
    /// top-level `await` it's an async function returning its promise).
    fn main_func(&mut self) -> (u32, bool);
    /// `ref.func` will be used on function `index` (it goes into a declarative element segment).
    fn declare_func(&mut self, index: u32);
    /// The slot of function `index` (a `$code`) in the code table (table 0, funcref): `$function`s
    /// hold slots and calls go through `call_indirect`. (T89, F350: a funcref read from a GC struct
    /// costs wasmtime's `get_interned_func_ref` libcall on every call.)
    fn code_slot(&mut self, index: u32) -> i32;
    /// Function index of the prelude's StringToNumber `(jsval) -> f64`, if the module has it (T21).
    fn string_to_number(&mut self) -> Option<u32>;
    /// The prelude's `$$DateCall` `$code` function index (T26): `Date` as a value's call entry.
    fn date_call(&mut self) -> Option<u32>;
    /// The prelude's `$$dateInspect` `(jsval) -> jsval` (T26): `console.log`'s text of a Date.
    fn date_inspect(&mut self) -> Option<u32>;
    /// The prelude's `$$inspectCustom` `(jsval, jsval) -> jsval` (T56): `util.inspect.custom`.
    fn inspect_custom(&mut self) -> Option<u32> {
        None
    }
    /// The prelude's generic fallback of a native `Array.prototype` method (T35): its `$code`
    /// function index.
    fn array_generic(&mut self, key: &str) -> Option<u32>;
    /// The prelude methods of a built-in prototype (T21): (key, `$code` function index, length).
    fn builtin_methods(&mut self, b: js2wasi3_ir::Builtin) -> Vec<(String, u32, u32)>;
    /// Can the module make a `Map`/`Set` (T25)?
    fn has_maps(&mut self) -> bool;
    /// Does the code name a property `bind` (T25)?
    fn has_bind(&mut self) -> bool;
    /// Does the code name a property `toString` (T25: `Function.prototype.toString`)?
    fn has_to_string(&mut self) -> bool;
    /// A built-in constructor used as a value (T25): its prelude code (a `$code`) and `length`.
    fn builtin_ctor(&mut self, b: js2wasi3_ir::Builtin) -> Option<(u32, u32)>;
    /// Do built-in prototypes without one get a constructor stub (T25: the code names
    /// `constructor`)?
    fn ctor_stubs(&mut self) -> bool;
    /// Does built-in `b`'s prototype have a constructor value (real or stub)?
    fn has_ctor(&mut self, b: js2wasi3_ir::Builtin) -> bool {
        self.builtin_ctor(b).is_some() || (b.ctor_length().is_some() && self.ctor_stubs())
    }
    /// `Math`/`JSON` as values (T25): their prelude functions in use, (key, code, length).
    fn namespace_methods(&mut self, ns: js2wasi3_ir::Namespace) -> Vec<(String, u32, u32)>;
    /// Does the user's code name property `name` (one of the lowering's gated names, T25)?
    fn names(&mut self, name: &str) -> bool;
    /// Can the program make an object with a built-in `Symbol.toStringTag` (T25: a map, promise,
    /// generator, iterator, `Math`/`JSON`, generator or async function), or name `toStringTag`?
    fn has_tags(&mut self) -> bool;
    /// Is `Math` or `JSON` used as a value?
    fn has_namespaces(&mut self) -> bool;
    /// Its statics implemented by the prelude: (key, code, length).
    fn builtin_statics(&mut self, b: js2wasi3_ir::Builtin) -> Vec<(String, u32, u32)>;

    /// Can the module make a BigInt (T40)? The helpers handle them only then.
    fn bigint(&mut self) -> bool;
    /// Does the module have `DisposableStack`/`AsyncDisposableStack` (T45)?
    fn stacks(&mut self) -> bool {
        false
    }
    /// Can the module make a resizable `ArrayBuffer` or detach one (T47)?
    fn rab(&mut self) -> bool {
        false
    }
    /// Does the module have `BigInt64Array`/`BigUint64Array` (T48, `Module::big_typed`)?
    fn big_typed(&mut self) -> bool {
        false
    }
    /// The typed array kinds in this module: 9, or 11 with the BigInt arrays (T48).
    fn typed_kinds(&mut self) -> u8 {
        if self.big_typed() { 11 } else { js2wasi3_ir::TYPED_KINDS }
    }
    /// Function index of the prelude's ToBigInt `(jsval) -> jsval` (T48).
    fn to_bigint(&mut self) -> Option<u32> {
        None
    }
    /// Can a constructor value be called with another new target (T47, `Module::subclassing`)?
    fn subclassing(&mut self) -> bool {
        false
    }
    /// `build --unhandled-rejections=none` (T50, `Module::ignore_unhandled`).
    fn ignore_unhandled(&mut self) -> bool {
        false
    }
    /// Is the entry CommonJS (T60, `Module::cjs_main`)?
    fn cjs_main(&mut self) -> bool {
        false
    }
    /// Does the command use `process.exit` or `process`'s at-exit hook (T55: `host::EXIT_TAG`
    /// and the ending state)?
    fn exits(&mut self) -> bool {
        false
    }
    /// The prelude's `$$processAtExit(status)` (T55): its function index, and whether it returns
    /// a value.
    fn process_at_exit(&mut self) -> Option<(u32, bool)> {
        None
    }
    /// The `Promise` statics implemented by the prelude (T50, `Module::promise_statics`):
    /// (key, code, length).
    fn promise_statics(&mut self) -> Vec<(String, u32, u32)> {
        Vec::new()
    }
    /// The well-known symbols that are own properties of `Symbol` as a value (T49,
    /// `Module::symbol_statics`).
    fn symbol_statics(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// Function index of the prelude's StringToBigInt `(jsval) -> jsval` (T40).
    fn string_to_bigint(&mut self) -> Option<u32>;
    /// Function indices of the backend's `f64` `%` and `**` helpers (T40).
    fn js_rem(&mut self) -> u32;
    fn js_pow(&mut self) -> u32;

    fn str(&mut self, s: &str) -> u32 {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.string(&units)
    }
}

pub fn jsval() -> ValType {
    ValType::Ref(RefType::EQREF)
}

pub fn string_ref() -> ValType {
    ValType::Ref(RefType {
        nullable: false,
        heap_type: string(),
    })
}

fn number() -> HeapType {
    HeapType::Concrete(T_NUMBER)
}

fn oddball() -> HeapType {
    HeapType::Concrete(T_ODDBALL)
}

pub fn string() -> HeapType {
    HeapType::Concrete(T_STRING)
}

fn i31() -> HeapType {
    HeapType::I31
}

pub fn symbol() -> HeapType {
    HeapType::Concrete(T_SYMBOL)
}

/// Push "local `v` is a string" (a `$string` that isn't a `$symbol`).
pub fn is_string(s: &mut InstructionSink, v: u32) {
    s.test_string(v);
    s.local_get(v).ref_test_non_null(symbol()).i32_eqz();
    s.i32_and();
}

/// Type tests of a local that avoid wasmtime's `is_subtype` libcall (T69). A `ref.test` against a
/// concrete type that isn't `final` (`$string`, `$object`) is an inline compare only when the value's
/// type is that exact type; any other value (a number box, an oddball, an array, a function) costs a
/// call into the runtime, even when the answer is no. These first test the abstract `array` /
/// `struct` kinds (in the object header, inline) and exact `final` types, so the concrete test runs
/// only where it's cheap or rare. The results are those of the plain `ref.test`.
pub trait JsTest {
    /// Push "local `v` is a `$string`" (a string or a symbol).
    fn test_string(&mut self, v: u32) -> &mut Self;
    /// Push "local `v` is an `$object`" (any object, including arrays and functions).
    fn test_object(&mut self, v: u32) -> &mut Self;
}

impl JsTest for InstructionSink<'_> {
    fn test_string(&mut self, v: u32) -> &mut Self {
        self.local_get(v).ref_test_non_null(HeapType::Abstract {
            shared: false,
            ty: wasm_encoder::AbstractHeapType::Array,
        });
        self.if_(BlockType::Result(ValType::I32));
        self.local_get(v).ref_test_non_null(string());
        self.else_();
        self.i32_const(0);
        self.end()
    }

    fn test_object(&mut self, v: u32) -> &mut Self {
        self.local_get(v).ref_test_non_null(HeapType::Abstract {
            shared: false,
            ty: wasm_encoder::AbstractHeapType::Struct,
        });
        self.if_(BlockType::Result(ValType::I32));
        // The non-object structs a value can be, then the commonest objects, all exact final types.
        self.local_get(v).ref_test_non_null(number());
        self.local_get(v).ref_test_non_null(oddball());
        self.i32_or();
        self.if_(BlockType::Result(ValType::I32));
        self.i32_const(0);
        self.else_();
        self.local_get(v).ref_test_non_null(HeapType::Concrete(T_FUNCTION));
        self.local_get(v).ref_test_non_null(HeapType::Concrete(T_ARRAY));
        self.i32_or();
        self.if_(BlockType::Result(ValType::I32));
        self.i32_const(1);
        self.else_();
        self.local_get(v).ref_test_non_null(object());
        self.end();
        self.end();
        self.else_();
        self.i32_const(0);
        self.end()
    }
}

/// Push local `v` as a `(ref $object)`, or run `not_object` (which must not fall through: it returns
/// or branches out) when `v` isn't an object (T92). One chain does the test and the cast: the abstract
/// `struct` test, the two non-object structs a value commonly is, then `br_on_cast` to the final
/// `$function`, `$array` and `$map` and last to `$object`, which is inline for a plain object. A
/// separate `ref.test`/`ref.cast (ref $object)` calls wasmtime's `is_subtype` for every object that
/// isn't exactly `$object` (`$object` isn't final): twice per method call on a `Map`, an array or a
/// function. The other final subtypes (promise, typed array, …) still take that call, in the last
/// `br_on_cast`.
pub fn object_or(s: &mut InstructionSink, v: u32, not_object: impl FnOnce(&mut InstructionSink)) {
    s.block(BlockType::Result(rt(false, T_OBJECT)));
    s.block(BlockType::Empty);
    s.local_get(v)
        .ref_test_non_null(HeapType::Abstract {
            shared: false,
            ty: wasm_encoder::AbstractHeapType::Struct,
        })
        .i32_eqz()
        .br_if(0);
    s.local_get(v).ref_test_non_null(number());
    s.local_get(v).ref_test_non_null(oddball());
    s.i32_or().br_if(0);
    s.local_get(v);
    for t in [T_FUNCTION, T_ARRAY, T_MAP, T_OBJECT] {
        s.br_on_cast(
            1,
            RefType::EQREF,
            RefType {
                nullable: false,
                heap_type: HeapType::Concrete(t),
            },
        );
    }
    s.drop();
    s.end();
    not_object(s);
    s.end();
}

/// If local `v` is an object, run `body` with it as a `(ref $object)` on the stack (`body` consumes it; it may
/// return or fall through); otherwise skip `body` (T96). The `object_or` chain: no `is_subtype` for an
/// object that isn't exactly `$object`, where `test_object` + `ref.cast (ref $object)` called it twice.
pub fn object_then(s: &mut InstructionSink, v: u32, body: impl FnOnce(&mut InstructionSink)) {
    s.block(BlockType::Empty);
    object_or(s, v, |s| {
        s.br(1);
    });
    body(s);
    s.end();
}

pub fn object() -> HeapType {
    HeapType::Concrete(T_OBJECT)
}

/// `(ref null? $t)` for a runtime type.
pub fn rt(nullable: bool, t: u32) -> ValType {
    ValType::Ref(RefType {
        nullable,
        heap_type: HeapType::Concrete(t),
    })
}

/// The `typeof` code of a tag: its index in [`TypeofTag::ALL`].
pub fn typeof_code(tag: TypeofTag) -> i32 {
    TypeofTag::ALL.iter().position(|t| *t == tag).unwrap() as i32
}

/// Emit the `rec` group. Must be the first entries of the type section. With `bigint` (T40) it
/// ends with `$bigint`. With `rab` (T47) `$buffer` and `$typed` have the fields of resizable and
/// detached buffers (see `typed.rs`).
pub fn types(types: &mut TypeSection, bigint: bool, rab: bool) {
    let sub_of = |inner, is_final, supertype_idx| SubType {
        is_final,
        supertype_idx,
        composite_type: CompositeType {
            inner,
            shared: false,
            descriptor: None,
            describes: None,
        },
    };
    let sub = |inner| sub_of(inner, true, None);
    let field = |ty: StorageType, mutable| FieldType {
        element_type: ty,
        mutable,
    };
    let st = |ty: ValType| {
        sub(CompositeInnerType::Struct(StructType {
            fields: Box::new([field(StorageType::Val(ty), false)]),
        }))
    };
    let object_fields = vec![
        // `class` is mutable: it also holds the extensibility bits (T16).
        field(StorageType::Val(ValType::I32), true),
        field(StorageType::Val(rt(true, T_OBJECT)), true),
        field(StorageType::Val(rt(false, T_KEYS)), true),
        field(StorageType::Val(rt(false, T_VALS)), true),
        field(StorageType::Val(rt(false, T_BYTES)), true),
        field(StorageType::Val(ValType::I32), true),
        field(StorageType::Val(rt(true, T_I32S)), true),
    ];
    let mut array_fields = object_fields.clone();
    array_fields.push(field(StorageType::Val(rt(false, T_VALS)), true));
    array_fields.push(field(StorageType::Val(ValType::I32), true));
    // Element attributes (T32): null while every element has the default ones.
    array_fields.push(field(StorageType::Val(rt(true, T_BYTES)), true));
    let mut function_fields = object_fields.clone();
    // `code` and `construct` (T89): slots in the code table (-1: none), not funcrefs.
    function_fields.push(field(StorageType::Val(ValType::I32), false));
    function_fields.push(field(StorageType::Val(rt(true, T_VALS)), false));
    function_fields.push(field(StorageType::Val(ValType::I32), false));
    function_fields.push(field(StorageType::Val(ValType::I32), false));
    function_fields.push(field(StorageType::Val(rt(true, T_STRING)), true));
    let mut generator_fields = object_fields.clone();
    generator_fields.push(field(StorageType::Val(rt(false, T_RESUME)), false));
    generator_fields.push(field(StorageType::Val(rt(false, T_VALS)), false));
    generator_fields.push(field(StorageType::Val(ValType::I32), true));
    generator_fields.push(field(StorageType::Val(ValType::I32), true));
    generator_fields.push(field(StorageType::Val(ValType::I32), true));
    let mut timer_fields = object_fields.clone();
    timer_fields.push(field(StorageType::Val(ValType::I64), true));
    timer_fields.push(field(StorageType::Val(ValType::I64), false));
    timer_fields.push(field(StorageType::Val(jsval()), false));
    timer_fields.push(field(StorageType::Val(rt(false, T_VALS)), false));
    // T25: the delay (ns), the id, the flags.
    timer_fields.push(field(StorageType::Val(ValType::I64), false));
    timer_fields.push(field(StorageType::Val(ValType::I32), false));
    timer_fields.push(field(StorageType::Val(ValType::I32), true));
    let mut map_fields = object_fields.clone();
    map_fields.push(field(StorageType::Val(rt(false, T_MTABLE)), true));
    let mut buffer_fields = object_fields.clone();
    buffer_fields.push(field(StorageType::Val(rt(false, T_BYTES)), rab));
    if rab {
        // T47: `max` (a number; null: fixed-length), `detached`.
        buffer_fields.push(field(StorageType::Val(jsval()), false));
        buffer_fields.push(field(StorageType::Val(ValType::I32), true));
    }
    let mut typed_fields = object_fields.clone();
    typed_fields.push(field(StorageType::Val(ValType::I32), false));
    typed_fields.push(field(StorageType::Val(rt(false, T_BUFFER)), false));
    typed_fields.push(field(StorageType::Val(ValType::I32), false));
    typed_fields.push(field(StorageType::Val(ValType::I32), false));
    if rab {
        // T47: `tracking` (1: the length follows the buffer's).
        typed_fields.push(field(StorageType::Val(ValType::I32), false));
    }
    let mut mapiter_fields = object_fields.clone();
    mapiter_fields.push(field(StorageType::Val(rt(true, T_MTABLE)), true));
    mapiter_fields.push(field(StorageType::Val(ValType::I32), true));
    mapiter_fields.push(field(StorageType::Val(ValType::I32), false));
    let mut wrapper_fields = object_fields.clone();
    wrapper_fields.push(field(StorageType::Val(jsval()), false));
    let mut promise_fields = object_fields.clone();
    promise_fields.push(field(StorageType::Val(ValType::I32), true));
    promise_fields.push(field(StorageType::Val(jsval()), true));
    promise_fields.push(field(StorageType::Val(rt(true, T_VALS)), true));
    promise_fields.push(field(StorageType::Val(ValType::I32), true));
    promise_fields.push(field(StorageType::Val(ValType::I32), true));
    let mut group = vec![
        st(ValType::F64),
        st(ValType::I32),
        // $string: not final, `$symbol` extends it.
        sub_of(
            CompositeInnerType::Array(ArrayType(field(StorageType::I16, true))),
            false,
            None,
        ),
        sub(CompositeInnerType::Array(ArrayType(field(
            StorageType::Val(ValType::I64),
            false,
        )))),
        // $object: class, proto, keys, vals, attrs, count, index (see `object.rs`). Not final:
        // `$function` extends it.
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: object_fields.clone().into_boxed_slice(),
            }),
            false,
            None,
        ),
        sub(CompositeInnerType::Array(ArrayType(field(
            StorageType::Val(rt(true, T_STRING)),
            true,
        )))),
        sub(CompositeInnerType::Array(ArrayType(field(
            StorageType::Val(jsval()),
            true,
        )))),
        sub(CompositeInnerType::Array(ArrayType(field(
            StorageType::I8,
            true,
        )))),
        sub(CompositeInnerType::Array(ArrayType(field(
            StorageType::Val(ValType::I32),
            true,
        )))),
        sub(CompositeInnerType::Func(wasm_encoder::FuncType::new(
            [jsval(), jsval(), jsval()],
            [jsval()],
        ))),
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: function_fields.into_boxed_slice(),
            }),
            true,
            Some(T_OBJECT),
        ),
        sub(CompositeInnerType::Struct(StructType {
            fields: Box::new([field(StorageType::Val(ValType::F64), true)]),
        })),
        sub(CompositeInnerType::Struct(StructType {
            fields: Box::new([field(StorageType::Val(jsval()), true)]),
        })),
        sub_of(
            CompositeInnerType::Array(ArrayType(field(StorageType::I16, true))),
            true,
            Some(T_STRING),
        ),
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: array_fields.into_boxed_slice(),
            }),
            true,
            Some(T_OBJECT),
        ),
        // $iter: kind, obj, next, index, value (see `iter.rs`).
        sub(CompositeInnerType::Struct(StructType {
            fields: Box::new([
                field(StorageType::Val(ValType::I32), true),
                field(StorageType::Val(jsval()), false),
                field(StorageType::Val(jsval()), false),
                field(StorageType::Val(ValType::I32), true),
                field(StorageType::Val(jsval()), true),
            ]),
        })),
        sub(CompositeInnerType::Struct(StructType {
            fields: Box::new([
                field(StorageType::Val(jsval()), true),
                field(StorageType::Val(jsval()), true),
            ]),
        })),
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: promise_fields.into_boxed_slice(),
            }),
            true,
            Some(T_OBJECT),
        ),
        sub(CompositeInnerType::Struct(StructType {
            fields: Box::new([
                field(StorageType::Val(rt(true, T_PROMISE)), false),
                field(StorageType::Val(jsval()), false),
                field(StorageType::Val(jsval()), false),
            ]),
        })),
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: generator_fields.into_boxed_slice(),
            }),
            true,
            Some(T_OBJECT),
        ),
        sub(CompositeInnerType::Func(wasm_encoder::FuncType::new(
            [jsval(), ValType::I32, jsval()],
            [jsval()],
        ))),
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: timer_fields.into_boxed_slice(),
            }),
            true,
            Some(T_OBJECT),
        ),
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: map_fields.into_boxed_slice(),
            }),
            true,
            Some(T_OBJECT),
        ),
        // $mtable: keys, vals, index, used, size, next, removed, hashes (see `mapset.rs`).
        sub(CompositeInnerType::Struct(StructType {
            fields: Box::new([
                field(StorageType::Val(rt(false, T_VALS)), true),
                field(StorageType::Val(rt(false, T_VALS)), true),
                field(StorageType::Val(rt(false, T_I32S)), true),
                field(StorageType::Val(ValType::I32), true),
                field(StorageType::Val(ValType::I32), true),
                field(StorageType::Val(rt(true, T_MTABLE)), true),
                field(StorageType::Val(rt(true, T_I32S)), true),
                field(StorageType::Val(rt(false, T_I32S)), true),
            ]),
        })),
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: mapiter_fields.into_boxed_slice(),
            }),
            true,
            Some(T_OBJECT),
        ),
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: buffer_fields.into_boxed_slice(),
            }),
            true,
            Some(T_OBJECT),
        ),
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: typed_fields.into_boxed_slice(),
            }),
            true,
            Some(T_OBJECT),
        ),
        sub_of(
            CompositeInnerType::Struct(StructType {
                fields: wrapper_fields.into_boxed_slice(),
            }),
            true,
            Some(T_OBJECT),
        ),
        // $strbuf: buf, len, flat (T78).
        sub(CompositeInnerType::Struct(StructType {
            fields: Box::new([
                field(StorageType::Val(rt(true, T_STRING)), true),
                field(StorageType::Val(ValType::I32), true),
                field(StorageType::Val(rt(true, T_STRING)), true),
            ]),
        })),
    ];
    if bigint {
        group.push(sub(CompositeInnerType::Array(ArrayType(field(
            StorageType::Val(ValType::I32),
            true,
        )))));
    }
    types.ty().rec(group);
}

/// Emit the runtime globals. Must be the first entries of the global section. With `bigint`
/// (T40), [`G_BIGINT_PROTO`] and [`G_BIGINT_CTOR`] follow.
pub fn globals(
    globals: &mut GlobalSection,
    bigint: bool,
    suppressed: bool,
    stacks: bool,
    big_typed: bool,
) {
    for kind in [K_UNDEFINED, K_NULL, K_FALSE, K_TRUE] {
        globals.global(
            GlobalType {
                val_type: ValType::Ref(RefType {
                    nullable: false,
                    heap_type: oddball(),
                }),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([
                Instruction::I32Const(kind),
                Instruction::StructNew(T_ODDBALL),
            ]),
        );
    }
    globals.global(
        GlobalType {
            val_type: ValType::Ref(RefType {
                nullable: true,
                heap_type: HeapType::Concrete(T_I64S),
            }),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Concrete(T_I64S)),
    );
    globals.global(
        GlobalType {
            val_type: ValType::Ref(RefType {
                nullable: false,
                heap_type: oddball(),
            }),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            Instruction::I32Const(K_BUILTIN),
            Instruction::StructNew(T_ODDBALL),
        ]),
    );
    for _ in js2wasi3_ir::Builtin::ALL {
        globals.global(
            GlobalType {
                val_type: rt(true, T_OBJECT),
                mutable: true,
                shared: false,
            },
            &ConstExpr::ref_null(object()),
        );
    }
    for description in ["Symbol.iterator", "state"] {
        let mut insns: Vec<Instruction> = description
            .encode_utf16()
            .map(|u| Instruction::I32Const(u as i32))
            .collect();
        insns.push(Instruction::ArrayNewFixed {
            array_type_index: T_SYMBOL,
            array_size: insns.len() as u32,
        });
        globals.global(
            GlobalType {
                val_type: string_ref(),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended(insns),
        );
    }
    for _ in 0..js2wasi3_ir::ErrorKind::ALL.len() + 2 {
        globals.global(
            GlobalType {
                val_type: rt(true, T_OBJECT),
                mutable: true,
                shared: false,
            },
            &ConstExpr::ref_null(object()),
        );
    }
    // The job queue (head, tail) and the rejected promises (count).
    let vals = GlobalType {
        val_type: rt(true, T_VALS),
        mutable: true,
        shared: false,
    };
    let int = GlobalType {
        val_type: ValType::I32,
        mutable: true,
        shared: false,
    };
    let null_vals = ConstExpr::ref_null(HeapType::Concrete(T_VALS));
    globals.global(vals, &null_vals);
    globals.global(int, &ConstExpr::i32_const(0));
    globals.global(int, &ConstExpr::i32_const(0));
    globals.global(vals, &null_vals);
    globals.global(int, &ConstExpr::i32_const(0));
    // Timers (T19): the functions, the list, the loop time, the wait.
    for _ in 0..js2wasi3_ir::TIMER_NAMES.len() {
        globals.global(
            GlobalType {
                val_type: rt(true, T_OBJECT),
                mutable: true,
                shared: false,
            },
            &ConstExpr::ref_null(object()),
        );
    }
    let long = GlobalType {
        val_type: ValType::I64,
        mutable: true,
        shared: false,
    };
    globals.global(vals, &null_vals);
    globals.global(int, &ConstExpr::i32_const(0));
    globals.global(long, &ConstExpr::i64_const(0));
    globals.global(int, &ConstExpr::i32_const(0));
    globals.global(long, &ConstExpr::i64_const(0));
    // T25: the identity hash counter.
    globals.global(int, &ConstExpr::i32_const(0));
    // T25: the well-known symbols, `console.log`'s cycle tracking, the built-in constructors and
    // namespaces as values.
    globals.global(vals, &null_vals);
    for _ in 0..2 {
        globals.global(vals, &null_vals);
        globals.global(int, &ConstExpr::i32_const(0));
    }
    for _ in 0..js2wasi3_ir::Builtin::ALL.len() + js2wasi3_ir::Namespace::ALL.len() {
        globals.global(
            GlobalType {
                val_type: rt(true, T_OBJECT),
                mutable: true,
                shared: false,
            },
            &ConstExpr::ref_null(object()),
        );
    }
    // T25: immediates, ticks, the timer id.
    for _ in 0..2 {
        globals.global(vals, &null_vals);
        globals.global(int, &ConstExpr::i32_const(0));
    }
    globals.global(int, &ConstExpr::i32_const(0));
    // T34: index keys on the arrays' prototype chain.
    globals.global(int, &ConstExpr::i32_const(0));
    // T27: `RegExp.prototype.exec`; T40: `BigInt.prototype`, `BigInt`.
    // T45: `SuppressedError.prototype` and its constructor.
    // T48: the BigInt arrays' prototypes and constructors.
    let extra = runtime_globals(bigint, suppressed, stacks, big_typed) - GLOBALS;
    globals.global(
        GlobalType {
            val_type: rt(true, T_OBJECT),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(object()),
    );
    // T74: the inline caches.
    globals.global(
        GlobalType {
            val_type: rt(true, T_I32S),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Concrete(T_I32S)),
    );
    // T75: the integer keys.
    globals.global(
        GlobalType {
            val_type: rt(true, T_KEYS),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Concrete(T_KEYS)),
    );
    // T76: the RegExp.prototype snapshot.
    globals.global(
        GlobalType {
            val_type: rt(true, T_VALS),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Concrete(T_VALS)),
    );
    // T90: the epoch and the prototypes of the write sites.
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(1),
    );
    globals.global(
        GlobalType {
            val_type: rt(true, T_VALS),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Concrete(T_VALS)),
    );
    // T94: the string hash memo (key, hash).
    globals.global(
        GlobalType {
            val_type: rt(true, T_STRING),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(string()),
    );
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    );
    // T97: the typed-array `.length` snapshot and prototype cache.
    globals.global(
        GlobalType {
            val_type: rt(true, T_VALS),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Concrete(T_VALS)),
    );
    // T99: the prototype-hit caches of the constant-key read sites.
    globals.global(
        GlobalType {
            val_type: rt(true, T_VALS),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(HeapType::Concrete(T_VALS)),
    );
    for _ in 0..extra {
        globals.global(
            GlobalType {
                val_type: rt(true, T_OBJECT),
                mutable: true,
                shared: false,
            },
            &ConstExpr::ref_null(object()),
        );
    }
}

/// The placeholders of the constants' globals (T57): an empty `$string`, and (with BigInts) a
/// zero `$bigint`. Immutable, so constant expressions can read them.
pub fn const_placeholders(globals: &mut GlobalSection, bigint: bool) {
    let mut one = |ty: u32, insns: Vec<Instruction>| {
        globals.global(
            GlobalType {
                val_type: ValType::Ref(RefType {
                    nullable: false,
                    heap_type: HeapType::Concrete(ty),
                }),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended(insns),
        );
    };
    one(
        T_STRING,
        vec![Instruction::ArrayNewFixed {
            array_type_index: T_STRING,
            array_size: 0,
        }],
    );
    if bigint {
        one(
            T_BIGINT,
            vec![
                Instruction::I32Const(0),
                Instruction::ArrayNewFixed {
                    array_type_index: T_BIGINT,
                    array_size: 1,
                },
            ],
        );
    }
}

/// A string constant's global (T57): `(global (mut (ref $string)) (global.get placeholder))`,
/// set by the start function.
pub fn string_global(globals: &mut GlobalSection, placeholder: u32) {
    globals.global(
        GlobalType {
            val_type: string_ref(),
            mutable: true,
            shared: false,
        },
        &ConstExpr::global_get(placeholder),
    );
}

/// A constant `f64` as a JS value, in a constant expression.
pub fn const_number(v: f64) -> ConstExpr {
    match small_int(v) {
        Some(i) => ConstExpr::extended([Instruction::I32Const(i), Instruction::RefI31]),
        None => ConstExpr::extended([
            Instruction::F64Const(v.into()),
            Instruction::StructNew(T_NUMBER),
        ]),
    }
}

/// `v` if it's stored as an `i31ref`.
pub fn small_int(v: f64) -> Option<i32> {
    let i = v as i32;
    (i as f64 == v && (I31_MIN..I31_END).contains(&i) && !(v == 0.0 && v.is_sign_negative()))
        .then_some(i)
}

/// Push `v` (a constant) as a JS value.
pub fn push_number(s: &mut InstructionSink, v: f64) {
    match small_int(v) {
        Some(i) => {
            s.i32_const(i).ref_i31();
        }
        None => {
            s.f64_const(v.into()).struct_new(T_NUMBER);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    Box,
    ToNumber,
    ToBoolean,
    IsNumber,
    IsNullish,
    StrictEq,
    LooseEq,
    /// `() -> jsval`: `Symbol.<WELL_KNOWN_SYMBOLS[k]>` (T25), made on first use.
    WellKnownSymbol(u8),
    Typeof,
    TypeofString,
    StrEq,
    Concat,
    ToString,
    /// `(s: jsval) -> (ref $string)`: QuoteJSONString of string `s` (T76).
    JsonQuote,
    /// `(dst: jsval, i: i32, src: jsval)`: copy string `src` into string `dst` at `i` (T76).
    StrCopy,
    StrBufGet,
    StrBufAppend,
    StrBufLen,
    /// `(r: (ref $i32s), base: i32, n: i32, c: i32) -> i32` (T78): is `c` in one of the `n` sorted
    /// ranges `[lo, hi]` of `r` from `base`?
    ReInRanges,
    /// See `Intrinsic::ReAltsFind` (T78).
    ReAltsFind,
    /// `(buf: jsval, i: i32, s: jsval) -> i32`: QuoteJSONString of `s` written into `buf` at `i`;
    /// the index after it (T76).
    JsonQuoteInto,
    /// `(s: jsval, i: i32, mode: i32) -> i32`: see `Intrinsic::JsonScan` (T76).
    JsonScan,
    JsAdd,
    Inspect,
    NoFormat,
    /// `(v: jsval, prop: (ref $string))`: throw the TypeError for reading `prop` of `undefined`/`null`.
    ThrowRead,
    /// `(v: jsval) -> (ref $string)`: how an uncaught exception is shown: ToString for an
    /// error, the `console.log` text otherwise.
    Describe,
    /// `(description: jsval) -> jsval`: `Symbol(description)` (T15).
    NewSymbol,
    /// `(hay, needle: (ref $string), pos: i32) -> i32`: does `needle` occur in `hay` at `pos`?
    StrAt,
    /// `(s: (ref $string), from: i32) -> (ref $string)`: `s.slice(from)`.
    StrSlice,
    /// `(s: (ref $string), start, end: i32) -> (ref $string)`: the units `[start, end)` (T21).
    StrSub,
    /// `(a, b: jsval) -> i32`: `Intrinsic::JsLess` (T21).
    JsLess {
        swap: bool,
        or_equal: bool,
    },
    Num(numstr::Helper),
    Big(bigint::Helper),
    Obj(object::Helper),
    Wrap(wrapper::Helper),
    Func(func::Helper),
    Arr(array::Helper),
    Iter(iter::Helper),
    Prop(props::Helper),
    Prom(promise::Helper),
    Gen(generator::Helper),
    Async(asyncfn::Helper),
    AsyncGen(asyncgen::Helper),
    Typed(typed::Helper),
    Timer(timers::Helper),
    Map(mapset::Helper),
    Fmt(inspect::Helper),
    Abi(abi::Helper),
    Host(host::Helper),
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Box => "box_number",
            Helper::ToNumber => "to_number",
            Helper::ToBoolean => "to_boolean",
            Helper::IsNumber => "is_number",
            Helper::IsNullish => "is_nullish",
            Helper::StrictEq => "strict_eq",
            Helper::LooseEq => "loose_eq",
            Helper::WellKnownSymbol(_) => "well_known_symbol",
            Helper::Typeof => "typeof",
            Helper::TypeofString => "typeof_string",
            Helper::StrEq => "string_eq",
            Helper::Concat => "concat",
            Helper::ToString => "to_string",
            Helper::JsonQuote => "json_quote",
            Helper::StrCopy => "str_copy",
            Helper::StrBufGet => "str_buf_get",
            Helper::StrBufAppend => "str_buf_append",
            Helper::StrBufLen => "str_buf_len",
            Helper::ReInRanges => "re_in_ranges",
            Helper::ReAltsFind => "re_alts_find",
            Helper::JsonQuoteInto => "json_quote_into",
            Helper::JsonScan => "json_scan",
            Helper::JsAdd => "js_add",
            Helper::Inspect => "inspect",
            Helper::NoFormat => "no_format",
            Helper::ThrowRead => "throw_read_error",
            Helper::Describe => "describe",
            Helper::StrAt => "string_at",
            Helper::StrSlice => "string_slice",
            Helper::StrSub => "string_sub",
            Helper::JsLess { .. } => "js_less",
            Helper::NewSymbol => "new_symbol",
            Helper::Num(h) => h.name(),
            Helper::Big(h) => h.name(),
            Helper::Obj(h) => h.name(),
            Helper::Wrap(h) => h.name(),
            Helper::Func(h) => h.name(),
            Helper::Arr(h) => h.name(),
            Helper::Iter(h) => h.name(),
            Helper::Prop(h) => h.name(),
            Helper::Prom(h) => h.name(),
            Helper::Gen(h) => h.name(),
            Helper::Async(h) => h.name(),
            Helper::AsyncGen(h) => h.name(),
            Helper::Typed(h) => h.name(),
            Helper::Timer(h) => h.name(),
            Helper::Map(h) => h.name(),
            Helper::Fmt(h) => h.name(),
            Helper::Abi(h) => h.name(),
            Helper::Host(h) => h.name(),
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        let s = string_ref();
        match self {
            Helper::Box => (vec![ValType::F64], vec![v]),
            Helper::ToNumber => (vec![v], vec![ValType::F64]),
            Helper::ToBoolean | Helper::IsNumber | Helper::IsNullish | Helper::Typeof => {
                (vec![v], vec![ValType::I32])
            }
            Helper::StrictEq | Helper::LooseEq => (vec![v, v], vec![ValType::I32]),
            Helper::WellKnownSymbol(_) => (vec![], vec![v]),
            Helper::TypeofString | Helper::ToString => (vec![v], vec![s]),
            Helper::StrEq => (vec![s, s], vec![ValType::I32]),
            Helper::Concat => (vec![s, s], vec![s]),
            Helper::JsonQuote => (vec![v], vec![string_ref()]),
            Helper::StrCopy => (vec![v, ValType::I32, v], vec![]),
            Helper::StrBufGet => (vec![v], vec![v]),
            Helper::StrBufAppend => (vec![v, v], vec![v]),
            Helper::StrBufLen => (vec![v], vec![ValType::F64]),
            Helper::ReInRanges => (
                vec![rt(false, T_I32S), ValType::I32, ValType::I32, ValType::I32],
                vec![ValType::I32],
            ),
            Helper::ReAltsFind => (
                vec![v, v, v, ValType::I32, ValType::I32, v, v, v],
                vec![ValType::I32],
            ),
            Helper::JsonQuoteInto => (vec![v, ValType::I32, v], vec![ValType::I32]),
            Helper::JsonScan => (vec![v, ValType::I32, ValType::I32], vec![ValType::I32]),
            Helper::JsAdd => (vec![v, v], vec![v]),
            Helper::NoFormat | Helper::NewSymbol => (vec![v], vec![v]),
            Helper::Inspect | Helper::Describe => (vec![v], vec![s]),
            Helper::StrAt => (vec![s, s, ValType::I32], vec![ValType::I32]),
            Helper::StrSlice => (vec![s, ValType::I32], vec![s]),
            Helper::StrSub => (vec![s, ValType::I32, ValType::I32], vec![s]),
            Helper::JsLess { .. } => (vec![v, v], vec![ValType::I32]),
            Helper::ThrowRead => (vec![v, s], vec![]),
            Helper::Num(h) => h.signature(),
            Helper::Big(h) => h.signature(),
            Helper::Obj(h) => h.signature(),
            Helper::Wrap(h) => h.signature(),
            Helper::Func(h) => h.signature(),
            Helper::Arr(h) => h.signature(),
            Helper::Iter(h) => h.signature(),
            Helper::Prop(h) => h.signature(),
            Helper::Prom(h) => h.signature(),
            Helper::Gen(h) => h.signature(),
            Helper::Async(h) => h.signature(),
            Helper::AsyncGen(h) => h.signature(),
            Helper::Typed(h) => h.signature(),
            Helper::Timer(h) => h.signature(),
            Helper::Map(h) => h.signature(),
            Helper::Fmt(h) => h.signature(),
            Helper::Abi(h) => h.signature(),
            Helper::Host(h) => h.signature(),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Box => box_number(),
            Helper::ToNumber => to_number(cx),
            Helper::ToBoolean => to_boolean(cx),
            Helper::IsNumber => is_number(),
            Helper::IsNullish => is_nullish(),
            Helper::StrictEq => strict_eq(cx),
            Helper::LooseEq => loose_eq(cx),
            Helper::WellKnownSymbol(k) => well_known_symbol(cx, k),
            Helper::Typeof => typeof_(cx),
            Helper::TypeofString => typeof_string(cx),
            Helper::StrEq => str_eq(),
            Helper::Concat => concat(),
            Helper::ToString => to_string(cx),
            Helper::JsonQuote => json_quote(),
            Helper::StrCopy => str_copy(),
            Helper::StrBufGet => str_buf_get(),
            Helper::StrBufAppend => str_buf_append(cx),
            Helper::StrBufLen => str_buf_len(),
            Helper::ReInRanges => re_in_ranges(),
            Helper::ReAltsFind => re_alts_find(cx),
            Helper::JsonQuoteInto => json_quote_into(),
            Helper::JsonScan => json_scan(),
            Helper::JsAdd => js_add(cx),
            Helper::Inspect => inspect(cx),
            Helper::NoFormat => no_format(),
            Helper::ThrowRead => throw_read(cx),
            Helper::Describe => describe(cx),
            Helper::StrAt => str_at(),
            Helper::StrSlice => str_slice(),
            Helper::StrSub => str_sub(),
            Helper::JsLess { swap, or_equal } => js_less(cx, swap, or_equal),
            Helper::NewSymbol => new_symbol(cx),
            Helper::Num(h) => h.body(cx),
            Helper::Big(h) => h.body(cx),
            Helper::Obj(h) => h.body(cx),
            Helper::Wrap(h) => h.body(cx),
            Helper::Func(h) => h.body(cx),
            Helper::Arr(h) => h.body(cx),
            Helper::Iter(h) => h.body(cx),
            Helper::Prop(h) => h.body(cx),
            Helper::Prom(h) => h.body(cx),
            Helper::Gen(h) => h.body(cx),
            Helper::Async(h) => h.body(cx),
            Helper::AsyncGen(h) => h.body(cx),
            Helper::Typed(h) => h.body(cx),
            Helper::Timer(h) => h.body(cx),
            Helper::Map(h) => h.body(cx),
            Helper::Fmt(h) => h.body(cx),
            Helper::Host(h) => {
                let base = cx.host_globals();
                h.body(cx, base)
            }
            Helper::Abi(h) => {
                let bump = cx.bump_global();
                h.body(cx, bump)
            }
        }
    }
}

/// `$box_number(x: f64) -> jsval`.
fn box_number() -> Function {
    const X: u32 = 0;
    const I: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    // i = trunc_sat(x); keep as i31 if f64(i) == x, i in range and x is not -0.
    s.local_get(X)
        .i32_trunc_sat_f64_s()
        .local_tee(I)
        .f64_convert_i32_s()
        .local_get(X)
        .f64_eq();
    s.local_get(I).i32_const(I31_MIN).i32_ge_s().i32_and();
    s.local_get(I).i32_const(I31_END).i32_lt_s().i32_and();
    s.local_get(X)
        .i64_reinterpret_f64()
        .i64_const(i64::MIN)
        .i64_ne()
        .i32_and();
    s.if_(BlockType::Result(jsval()));
    s.local_get(I).ref_i31();
    s.else_();
    s.local_get(X).struct_new(T_NUMBER);
    s.end();
    s.end();
    f
}

/// Push the `$oddball` kind of local `v` (which must be an oddball).
fn kind(s: &mut InstructionSink, v: u32) {
    s.local_get(v)
        .ref_cast_non_null(oddball())
        .struct_get(T_ODDBALL, 0);
}

/// Push local `v` cast to `(ref $string)`.
fn as_string(s: &mut InstructionSink, v: u32) {
    s.local_get(v).ref_cast_non_null(string());
}

/// `$to_number(v: jsval) -> f64`: JS ToNumber on primitives.
fn to_number(cx: &mut dyn Cx) -> Function {
    let to_primitive_num = cx.helper(Helper::Obj(object::Helper::ObjectToNumber));
    let throw = cx.helper(Helper::Obj(object::Helper::Throw(ErrorKind::TypeError)));
    let no_num = cx.str("Cannot convert a Symbol value to a number");
    let str_to_num = cx.string_to_number();
    const V: u32 = 0;
    const K: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(i31());
    s.if_(BlockType::Result(ValType::F64));
    s.local_get(V)
        .ref_cast_non_null(i31())
        .i31_get_s()
        .f64_convert_i32_s();
    s.else_();
    s.local_get(V).ref_test_non_null(number());
    s.if_(BlockType::Result(ValType::F64));
    s.local_get(V)
        .ref_cast_non_null(number())
        .struct_get(T_NUMBER, 0);
    s.else_();
    s.local_get(V).ref_test_non_null(symbol());
    s.if_(BlockType::Empty);
    s.global_get(no_num).call(throw);
    s.end();
    if cx.bigint() {
        let no_big = cx.str("Cannot convert a BigInt value to a number");
        bigint::is_bigint(&mut s, V);
        s.if_(BlockType::Empty);
        s.global_get(no_big).call(throw);
        s.end();
    }
    // ToNumber of a string: the prelude's StringToNumber (T21). It's linked when the IR converts a
    // JS value to a number; a runtime-only conversion in a module without one traps (F85).
    s.test_string(V);
    s.if_(BlockType::Empty);
    match str_to_num {
        Some(f) => s.local_get(V).call(f).return_(),
        None => s.unreachable(),
    };
    s.end();
    // An object's ToPrimitive is a string: "" is 0; one starting with `[` ("[object Object]") or a
    // letter other than `I` (an error's "Name: message") can't be numeric, so NaN. Others: parsed.
    object_then(&mut s, V, |s| {
        s.call(to_primitive_num);
        s.return_();
    });
    // undefined → NaN, null/false → 0, true → 1.
    kind(&mut s, V);
    s.local_set(K);
    s.f64_const(1.0.into());
    s.f64_const(f64::NAN.into())
        .f64_const(0.0.into())
        .local_get(K)
        .i32_const(K_UNDEFINED)
        .i32_eq()
        .select();
    s.local_get(K).i32_const(K_TRUE).i32_eq().select();
    s.end();
    s.end();
    s.end();
    f
}

/// `$to_boolean(v: jsval) -> i32`: JS ToBoolean on primitives.
fn to_boolean(cx: &mut dyn Cx) -> Function {
    const V: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    // T75: the commonest first: oddballs (`true` only), numbers, strings.
    s.local_get(V).ref_test_non_null(oddball());
    s.if_(BlockType::Empty);
    kind(&mut s, V);
    s.i32_const(K_TRUE).i32_eq().return_();
    s.end();
    s.local_get(V).ref_test_non_null(i31());
    s.if_(BlockType::Empty);
    s.local_get(V).ref_cast_non_null(i31()).i31_get_s();
    s.i32_const(0).i32_ne().return_();
    s.end();
    s.local_get(V).ref_test_non_null(number());
    s.if_(BlockType::Empty);
    // false for ±0 and NaN.
    s.local_get(V)
        .ref_cast_non_null(number())
        .struct_get(T_NUMBER, 0)
        .f64_abs()
        .f64_const(0.0.into())
        .f64_gt()
        .return_();
    s.end();
    is_string(&mut s, V);
    s.if_(BlockType::Empty);
    // false only for "".
    as_string(&mut s, V);
    s.array_len().i32_const(0).i32_ne().return_();
    s.end();
    if cx.bigint() {
        // T40: false only for 0n.
        bigint::is_bigint(&mut s, V);
        s.if_(BlockType::Empty);
        s.local_get(V)
            .ref_cast_non_null(bigint::bigint())
            .array_len()
            .i32_const(1)
            .i32_gt_u()
            .return_();
        s.end();
    }
    // Objects and symbols are truthy.
    s.i32_const(1);
    s.end();
    f
}

/// `$is_number(v: jsval) -> i32`.
fn is_number() -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).ref_test_non_null(i31());
    s.local_get(0).ref_test_non_null(number());
    s.i32_or();
    s.end();
    f
}

/// The function index of `$is_nullish`.
pub fn is_nullish_fn(cx: &mut dyn Cx) -> u32 {
    cx.helper(Helper::IsNullish)
}

/// `$is_nullish(v: jsval) -> i32`: `undefined` or `null`.
fn is_nullish() -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).ref_test_non_null(oddball());
    s.if_(BlockType::Result(ValType::I32));
    kind(&mut s, 0);
    s.i32_const(K_NULL).i32_le_u();
    s.else_();
    s.i32_const(0);
    s.end();
    s.end();
    f
}

/// Push "locals `a` and `b` are both strings".
fn both_strings(s: &mut InstructionSink, a: u32, b: u32) {
    s.test_string(a);
    s.test_string(b);
    s.i32_and();
}

/// `$strict_eq(a, b) -> i32`: JS `===`. Numbers compare by value (NaN ≠ NaN, +0 = -0),
/// strings by content, everything else by identity.
fn strict_eq(cx: &mut dyn Cx) -> Function {
    let str_eq = cx.helper(Helper::StrEq);
    const A: u32 = 0;
    const B: u32 = 1;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    // T75: the same value is equal unless it's a NaN; then numbers (i31 or boxed) inline.
    s.local_get(A).local_get(B).ref_eq();
    s.if_(BlockType::Empty);
    s.local_get(A).ref_test_non_null(number());
    s.if_(BlockType::Empty);
    s.local_get(A)
        .ref_cast_non_null(number())
        .struct_get(T_NUMBER, 0);
    s.local_get(A)
        .ref_cast_non_null(number())
        .struct_get(T_NUMBER, 0);
    s.f64_eq().return_();
    s.end();
    s.i32_const(1).return_();
    s.end();
    let as_f64 = |s: &mut InstructionSink, v: u32| {
        s.local_get(v).ref_test_non_null(i31());
        s.if_(BlockType::Result(ValType::F64));
        s.local_get(v)
            .ref_cast_non_null(i31())
            .i31_get_s()
            .f64_convert_i32_s();
        s.else_();
        s.local_get(v)
            .ref_cast_non_null(number())
            .struct_get(T_NUMBER, 0);
        s.end();
    };
    let is_num = |s: &mut InstructionSink, v: u32| {
        s.local_get(v).ref_test_non_null(i31());
        s.local_get(v).ref_test_non_null(number());
        s.i32_or();
    };
    is_num(&mut s, A);
    s.if_(BlockType::Empty);
    // Two different i31s differ; else compare as f64s if both are numbers.
    is_num(&mut s, B);
    s.if_(BlockType::Empty);
    as_f64(&mut s, A);
    as_f64(&mut s, B);
    s.f64_eq().return_();
    s.end();
    s.i32_const(0).return_();
    s.end();
    both_strings(&mut s, A, B);
    s.if_(BlockType::Result(ValType::I32));
    as_string(&mut s, A);
    as_string(&mut s, B);
    s.call(str_eq);
    s.else_();
    if cx.bigint() {
        // T40: BigInts compare by value.
        bigint::is_bigint(&mut s, A);
        bigint::is_bigint(&mut s, B);
        s.i32_and();
        s.if_(BlockType::Result(ValType::I32));
        let cmp = cx.helper(Helper::Big(bigint::Helper::Cmp));
        s.local_get(A).ref_cast_non_null(bigint::bigint());
        s.local_get(B).ref_cast_non_null(bigint::bigint());
        s.call(cmp).i32_eqz();
        s.else_();
        s.i32_const(0);
        s.end();
    } else {
        // Not the same reference (checked above).
        s.i32_const(0);
    }
    s.end();
    s.end();
    f
}

/// T75: replace the `jsval` on the stack by its ToNumber as a JS value: a number (i31 or `$number`)
/// stays as it is; `t` is a `jsval` scratch local.
pub fn as_number_value(s: &mut InstructionSink, t: u32, to_number: u32, box_: u32) {
    s.local_tee(t).ref_test_non_null(i31());
    s.local_get(t).ref_test_non_null(number()).i32_or();
    s.if_(BlockType::Result(jsval()));
    s.local_get(t);
    s.else_();
    s.local_get(t).call(to_number).call(box_);
    s.end();
}

/// T75: push `typeof v === tag` for the `jsval` `v` on the stack if it needs no call (`t` is a
/// `jsval` scratch local); true (it always does since T76).
pub fn typeof_is(s: &mut InstructionSink, t: u32, tag: TypeofTag, bigint: bool) -> bool {
    match tag {
        TypeofTag::Number => {
            s.local_tee(t).ref_test_non_null(i31());
            s.local_get(t).ref_test_non_null(number()).i32_or();
        }
        TypeofTag::String => {
            s.local_set(t);
            is_string(s, t);
        }
        TypeofTag::Symbol => {
            s.ref_test_non_null(symbol());
        }
        TypeofTag::Function => {
            s.ref_test_non_null(func::function());
        }
        TypeofTag::Undefined => {
            s.global_get(G_UNDEFINED).ref_eq();
        }
        TypeofTag::Boolean => {
            s.local_tee(t).global_get(G_TRUE).ref_eq();
            s.local_get(t).global_get(G_FALSE).ref_eq().i32_or();
        }
        TypeofTag::Object => {
            // T76: `null`, or an object that isn't a function.
            s.local_tee(t).global_get(G_NULL).ref_eq();
            s.test_object(t);
            s.local_get(t).ref_test_non_null(func::function()).i32_eqz();
            s.i32_and().i32_or();
        }
        TypeofTag::BigInt if bigint => {
            s.local_set(t);
            bigint::is_bigint(s, t);
        }
        // No `$bigint` type: never a BigInt.
        TypeofTag::BigInt => {
            s.drop().i32_const(0);
        }
    }
    true
}

/// T75: the constant operand of an inline `===`.
pub enum EqConst {
    /// An oddball (`undefined`, `null`, `true`, `false`) in this global: equal only to itself.
    Oddball(u32),
    /// A string constant in global `global`, `len` code units long.
    Str { global: u32, len: u32 },
    /// A number.
    Num(f64),
}

/// Push `v === c` for the `jsval` `v` on the stack; `t` is a `jsval` scratch local and `str_eq`
/// the `$string_eq` function.
pub fn strict_eq_const(s: &mut InstructionSink, t: u32, c: EqConst, str_eq: u32) {
    match c {
        EqConst::Oddball(g) => {
            s.global_get(g).ref_eq();
        }
        EqConst::Str { global, len } => {
            // Identity (the same constant), else a string of the same length and units.
            s.local_tee(t).global_get(global).ref_eq();
            s.if_(BlockType::Result(ValType::I32));
            s.i32_const(1);
            s.else_();
            s.test_string(t);
            s.if_(BlockType::Result(ValType::I32));
            s.local_get(t)
                .ref_cast_non_null(string())
                .array_len()
                .i32_const(len as i32)
                .i32_eq();
            s.if_(BlockType::Result(ValType::I32));
            s.local_get(t).ref_cast_non_null(string());
            s.global_get(global).call(str_eq);
            s.else_();
            s.i32_const(0);
            s.end();
            s.else_();
            s.i32_const(0);
            s.end();
            s.end();
        }
        EqConst::Num(x) if x.is_nan() => {
            s.drop().i32_const(0);
        }
        EqConst::Num(x) => {
            s.local_tee(t).ref_test_non_null(i31());
            s.if_(BlockType::Result(ValType::I32));
            match small_int(x) {
                Some(i) => {
                    s.local_get(t).ref_cast_non_null(i31()).i31_get_s();
                    s.i32_const(i).i32_eq();
                }
                // -0 is an i31 0; other non-i31 constants never equal an i31.
                None if x == 0.0 => {
                    s.local_get(t)
                        .ref_cast_non_null(i31())
                        .i31_get_s()
                        .i32_eqz();
                }
                None => {
                    s.i32_const(0);
                }
            }
            s.else_();
            s.local_get(t).ref_test_non_null(number());
            s.if_(BlockType::Result(ValType::I32));
            s.local_get(t)
                .ref_cast_non_null(number())
                .struct_get(T_NUMBER, 0)
                .f64_const(x.into())
                .f64_eq();
            s.else_();
            s.i32_const(0);
            s.end();
            s.end();
        }
    }
}

/// `$loose_eq(a, b) -> i32`: JS `==`. Two objects compare by identity. `null == undefined`; a
/// nullish value equals nothing else. An object compares as its ToPrimitive. A symbol equals only
/// itself. Two strings compare by content; anything else compares as numbers.
fn loose_eq(cx: &mut dyn Cx) -> Function {
    let is_nullish = cx.helper(Helper::IsNullish);
    let to_number = cx.helper(Helper::ToNumber);
    let str_eq = cx.helper(Helper::StrEq);
    const NA: u32 = 2;
    const NB: u32 = 3;
    let to_primitive = cx.helper(Helper::Obj(object::Helper::ToPrimitive));
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    // Two objects compare by identity; one object compares as its ToPrimitive.
    s.test_object(0);
    s.test_object(1);
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(0).local_get(1).ref_eq().return_();
    s.end();
    s.local_get(0).call(is_nullish).local_set(NA);
    s.local_get(1).call(is_nullish).local_set(NB);
    s.local_get(NA).local_get(NB).i32_or();
    s.if_(BlockType::Empty);
    s.local_get(NA).local_get(NB).i32_and().return_();
    s.end();
    for v in [0, 1] {
        object_then(&mut s, v, |s| {
            s.i32_const(0).call(to_primitive).local_set(v);
        });
    }
    // A symbol equals only itself (T25).
    s.local_get(0).ref_test_non_null(symbol());
    s.local_get(1).ref_test_non_null(symbol());
    s.i32_or();
    s.if_(BlockType::Empty);
    s.local_get(0).local_get(1).ref_eq().return_();
    s.end();
    if cx.bigint() {
        // T40: a BigInt equals a BigInt, a string or a number (a boolean as one) of the same
        // value.
        let big_eq = cx.helper(Helper::Big(bigint::Helper::LooseEq));
        bigint::is_bigint(&mut s, 0);
        s.if_(BlockType::Empty);
        s.local_get(0).ref_cast_non_null(bigint::bigint());
        s.local_get(1).call(big_eq).return_();
        s.end();
        bigint::is_bigint(&mut s, 1);
        s.if_(BlockType::Empty);
        s.local_get(1).ref_cast_non_null(bigint::bigint());
        s.local_get(0).call(big_eq).return_();
        s.end();
    }
    s.block(BlockType::Result(ValType::I32));
    both_strings(&mut s, 0, 1);
    s.if_(BlockType::Result(ValType::I32));
    as_string(&mut s, 0);
    as_string(&mut s, 1);
    s.call(str_eq);
    s.else_();
    s.local_get(0).call(to_number);
    s.local_get(1).call(to_number);
    s.f64_eq();
    s.end();
    s.end();
    s.end();
    f
}

/// `$typeof(v) -> i32`: a [`typeof_code`].
fn typeof_(cx: &mut dyn Cx) -> Function {
    const V: u32 = 0;
    const K: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(i31());
    s.local_get(V).ref_test_non_null(number()).i32_or();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(typeof_code(TypeofTag::Number));
    s.else_();
    s.local_get(V).ref_test_non_null(symbol());
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(typeof_code(TypeofTag::Symbol));
    s.else_();
    s.test_string(V);
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(typeof_code(TypeofTag::String));
    s.else_();
    s.local_get(V).ref_test_non_null(func::function());
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(typeof_code(TypeofTag::Function));
    s.else_();
    let big = cx.bigint();
    if big {
        bigint::is_bigint(&mut s, V);
        s.if_(BlockType::Result(ValType::I32));
        s.i32_const(typeof_code(TypeofTag::BigInt));
        s.else_();
    }
    s.test_object(V);
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(typeof_code(TypeofTag::Object));
    s.else_();
    // Oddball kinds 0..=3 map to undefined, object (null), boolean, boolean: kind - (kind == 3).
    const _: () = assert!(K_UNDEFINED == 0 && K_NULL == 1 && K_FALSE == 2 && K_TRUE == 3);
    kind(&mut s, V);
    s.local_tee(K);
    s.local_get(K).i32_const(K_TRUE).i32_eq();
    s.i32_sub();
    if big {
        s.end();
    }
    s.end();
    s.end();
    s.end();
    s.end();
    s.end();
    s.end();
    f
}

/// `$typeof_string(v) -> (ref $string)`: JS `typeof` as a value.
fn typeof_string(cx: &mut dyn Cx) -> Function {
    let typeof_ = cx.helper(Helper::Typeof);
    let names: Vec<u32> = TypeofTag::ALL.iter().map(|t| cx.str(t.as_str())).collect();
    const V: u32 = 0;
    const C: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(V).call(typeof_).local_set(C);
    // if code == 0 then name0 else if code == 1 ... else last name
    let (last, rest) = names.split_last().unwrap();
    for (i, g) in rest.iter().enumerate() {
        s.local_get(C).i32_const(i as i32).i32_eq();
        s.if_(BlockType::Result(string_ref()));
        s.global_get(*g);
        s.else_();
    }
    s.global_get(*last);
    for _ in rest {
        s.end();
    }
    s.end();
    f
}

/// `$string_eq(a, b: (ref $string)) -> i32`: same length and code units.
fn str_eq() -> Function {
    const A: u32 = 0;
    const B: u32 = 1;
    const I: u32 = 2;
    const N: u32 = 3;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(A).local_get(B).ref_eq();
    s.if_(BlockType::Empty);
    s.i32_const(1).return_();
    s.end();
    // A symbol is only equal to itself (T15).
    s.local_get(A).ref_test_non_null(symbol());
    s.local_get(B).ref_test_non_null(symbol());
    s.i32_or();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.local_get(A).array_len().local_tee(N);
    s.local_get(B).array_len().i32_ne();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(A).local_get(I).array_get_u(T_STRING);
    s.local_get(B).local_get(I).array_get_u(T_STRING);
    s.i32_ne();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.i32_const(1);
    s.end();
    f
}

/// `$concat(a, b: (ref $string)) -> (ref $string)`.
fn concat() -> Function {
    const A: u32 = 0;
    const B: u32 = 1;
    const R: u32 = 2;
    let mut f = Function::new([(
        1,
        ValType::Ref(RefType {
            nullable: true,
            heap_type: string(),
        }),
    )]);
    let mut s = f.instructions();
    // Concatenating "" gives the other string back: no copy.
    s.local_get(A).array_len().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(B).return_();
    s.end();
    s.local_get(B).array_len().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(A).return_();
    s.end();
    s.local_get(A).array_len();
    s.local_get(B).array_len();
    s.i32_add().array_new_default(T_STRING).local_set(R);
    // array.copy dst dst_off src src_off len
    s.local_get(R).i32_const(0).local_get(A).i32_const(0);
    s.local_get(A).array_len();
    s.array_copy(T_STRING, T_STRING);
    s.local_get(R)
        .local_get(A)
        .array_len()
        .local_get(B)
        .i32_const(0);
    s.local_get(B).array_len();
    s.array_copy(T_STRING, T_STRING);
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

/// `$to_string(v: jsval) -> (ref $string)`: JS ToString (objects: ToString of ToPrimitive, hint string).
fn to_string(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(Helper::ToString);
    let is_number = cx.helper(Helper::IsNumber);
    let to_number = cx.helper(Helper::ToNumber);
    let number_to_string = cx.helper(Helper::Num(numstr::Helper::NumberToString));
    let to_primitive = cx.helper(Helper::Obj(object::Helper::ToPrimitive));
    let names = [
        cx.str("undefined"),
        cx.str("null"),
        cx.str("false"),
        cx.str("true"),
    ];
    const V: u32 = 0;
    const K: u32 = 1;
    let throw = cx.helper(Helper::Obj(object::Helper::Throw(ErrorKind::TypeError)));
    let no_str = cx.str("Cannot convert a Symbol value to a string");
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(symbol());
    s.if_(BlockType::Empty);
    s.global_get(no_str).call(throw);
    s.end();
    s.test_string(V);
    s.if_(BlockType::Empty);
    as_string(&mut s, V);
    s.return_();
    s.end();
    s.local_get(V).call(is_number);
    s.if_(BlockType::Empty);
    s.local_get(V)
        .call(to_number)
        .call(number_to_string)
        .return_();
    s.end();
    if cx.bigint() {
        let big_to_string = cx.helper(Helper::Big(bigint::Helper::ToString));
        bigint::is_bigint(&mut s, V);
        s.if_(BlockType::Empty);
        s.local_get(V).ref_cast_non_null(bigint::bigint());
        s.i32_const(10).call(big_to_string).return_();
        s.end();
    }
    object_then(&mut s, V, |s| {
        s.i32_const(2).call(to_primitive).call(to_string).return_();
    });
    kind(&mut s, V);
    s.local_set(K);
    for (k, g) in names[..3].iter().enumerate() {
        s.local_get(K).i32_const(k as i32).i32_eq();
        s.if_(BlockType::Empty);
        s.global_get(*g).return_();
        s.end();
    }
    s.global_get(names[3]);
    s.end();
    f
}

/// `$json_quote(s: jsval) -> (ref $string)` (T76): QuoteJSONString. One pass finds the result's length;
/// with nothing to escape the units are copied between the quotes, else a second pass writes the
/// escapes (`\b \t \n \f \r \" \\`, `\u00xx` for other controls and lone surrogates).
fn json_quote() -> Function {
    const V: u32 = 0;
    const S: u32 = 1;
    const R: u32 = 2;
    const LEN: u32 = 3;
    const N: u32 = 4;
    const I: u32 = 5;
    const J: u32 = 6;
    const U: u32 = 7;
    const E: u32 = 8;
    let sref = |nullable| {
        ValType::Ref(RefType {
            nullable,
            heap_type: string(),
        })
    };
    let mut f = Function::new([(2, sref(true)), (6, ValType::I32)]);
    let mut s = f.instructions();
    let unit = |s: &mut InstructionSink, i: u32| {
        s.local_get(S)
            .ref_as_non_null()
            .local_get(i)
            .array_get_u(T_STRING);
    };
    // U is one of 8, 9, 10, 12, 13 (a two-character escape).
    let short = |s: &mut InstructionSink| {
        s.local_get(U)
            .i32_const(8)
            .i32_sub()
            .i32_const(6)
            .i32_lt_u();
        s.local_get(U).i32_const(11).i32_ne().i32_and();
    };
    // U is a high surrogate followed by a low one (at I + 1).
    let paired = |s: &mut InstructionSink| {
        s.local_get(U).i32_const(0xdbff).i32_le_u();
        s.if_(BlockType::Result(ValType::I32));
        s.local_get(I)
            .i32_const(1)
            .i32_add()
            .local_get(LEN)
            .i32_lt_u();
        s.if_(BlockType::Result(ValType::I32));
        s.local_get(S).ref_as_non_null();
        s.local_get(I).i32_const(1).i32_add().array_get_u(T_STRING);
        s.i32_const(0xfc00).i32_and().i32_const(0xdc00).i32_eq();
        s.else_();
        s.i32_const(0);
        s.end();
        s.else_();
        s.i32_const(0);
        s.end();
    };
    let set = |s: &mut InstructionSink, at: i32, val: &dyn Fn(&mut InstructionSink)| {
        s.local_get(R).ref_as_non_null();
        s.local_get(J);
        if at != 0 {
            s.i32_const(at).i32_add();
        }
        val(s);
        s.array_set(T_STRING);
    };
    s.local_get(V).ref_cast_non_null(string()).local_tee(S);
    s.array_len()
        .local_tee(LEN)
        .i32_const(2)
        .i32_add()
        .local_set(N);
    // Pass 1: the length.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    unit(&mut s, I);
    s.local_tee(U).i32_const(32).i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(N).i32_const(1).i32_const(5);
    short(&mut s);
    s.select().i32_add().local_set(N);
    s.else_();
    s.local_get(U).i32_const(34).i32_eq();
    s.local_get(U).i32_const(92).i32_eq().i32_or();
    s.if_(BlockType::Empty);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.else_();
    s.local_get(U)
        .i32_const(0xf800)
        .i32_and()
        .i32_const(0xd800)
        .i32_eq();
    s.if_(BlockType::Empty);
    paired(&mut s);
    s.if_(BlockType::Empty);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.else_();
    s.local_get(N).i32_const(5).i32_add().local_set(N);
    s.end();
    s.end();
    s.end();
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(N).array_new_default(T_STRING).local_set(R);
    s.local_get(R)
        .ref_as_non_null()
        .i32_const(0)
        .i32_const(34)
        .array_set(T_STRING);
    s.local_get(R)
        .ref_as_non_null()
        .local_get(N)
        .i32_const(1)
        .i32_sub()
        .i32_const(34)
        .array_set(T_STRING);
    // Nothing to escape: copy.
    s.local_get(N)
        .local_get(LEN)
        .i32_const(2)
        .i32_add()
        .i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(R).ref_as_non_null().i32_const(1);
    s.local_get(S).ref_as_non_null().i32_const(0).local_get(LEN);
    s.array_copy(T_STRING, T_STRING);
    s.local_get(R).ref_as_non_null().return_();
    s.end();
    // Pass 2: write. E: 0 = the unit itself, > 0 = `\` and E, -1 = `\u` and 4 hex digits.
    s.i32_const(1).local_set(J);
    s.i32_const(0).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    unit(&mut s, I);
    s.local_set(U);
    s.i32_const(0).local_set(E);
    s.local_get(U).i32_const(32).i32_lt_u();
    s.if_(BlockType::Empty);
    // 8 → b, 9 → t, 10 → n, 12 → f, 13 → r, others → \u.
    s.i32_const(98)
        .i32_const(116)
        .i32_const(110)
        .i32_const(102)
        .i32_const(114);
    s.local_get(U).i32_const(12).i32_eq().select();
    s.local_get(U).i32_const(10).i32_eq().select();
    s.local_get(U).i32_const(9).i32_eq().select();
    s.local_get(U).i32_const(8).i32_eq().select();
    s.i32_const(-1);
    short(&mut s);
    s.select().local_set(E);
    s.else_();
    s.local_get(U).i32_const(34).i32_eq();
    s.local_get(U).i32_const(92).i32_eq().i32_or();
    s.if_(BlockType::Empty);
    s.local_get(U).local_set(E);
    s.else_();
    s.local_get(U)
        .i32_const(0xf800)
        .i32_and()
        .i32_const(0xd800)
        .i32_eq();
    s.if_(BlockType::Empty);
    paired(&mut s);
    s.if_(BlockType::Empty);
    set(&mut s, 0, &|s| {
        s.local_get(U);
    });
    set(&mut s, 1, &|s| {
        s.local_get(S).ref_as_non_null();
        s.local_get(I).i32_const(1).i32_add().array_get_u(T_STRING);
    });
    s.local_get(J).i32_const(2).i32_add().local_set(J);
    s.local_get(I).i32_const(2).i32_add().local_set(I);
    s.br(4);
    s.end();
    s.i32_const(-1).local_set(E);
    s.end();
    s.end();
    s.end();
    s.local_get(E).i32_eqz();
    s.if_(BlockType::Empty);
    set(&mut s, 0, &|s| {
        s.local_get(U);
    });
    s.local_get(J).i32_const(1).i32_add().local_set(J);
    s.else_();
    set(&mut s, 0, &|s| {
        s.i32_const(92);
    });
    s.local_get(E).i32_const(0).i32_gt_s();
    s.if_(BlockType::Empty);
    set(&mut s, 1, &|s| {
        s.local_get(E);
    });
    s.local_get(J).i32_const(2).i32_add().local_set(J);
    s.else_();
    set(&mut s, 1, &|s| {
        s.i32_const('u' as i32);
    });
    for (k, shift) in [12, 8, 4, 0].into_iter().enumerate() {
        set(&mut s, 2 + k as i32, &|s| {
            // d < 10 ? 48 + d : 87 + d
            s.local_get(U)
                .i32_const(shift)
                .i32_shr_u()
                .i32_const(15)
                .i32_and()
                .local_tee(E);
            s.i32_const(48).i32_add();
            s.local_get(E).i32_const(87).i32_add();
            s.local_get(E).i32_const(10).i32_lt_u().select();
        });
    }
    s.local_get(J).i32_const(6).i32_add().local_set(J);
    s.end();
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

/// `$str_copy(dst, i, src)` (T76): `array.copy` of all of string `src` into string `dst` at `i`.
fn str_copy() -> Function {
    let mut f = Function::new([(1, string_ref())]);
    let mut s = f.instructions();
    s.local_get(2).ref_cast_non_null(string()).local_set(3);
    s.local_get(0).ref_cast_non_null(string()).local_get(1);
    s.local_get(3).i32_const(0).local_get(3).array_len();
    s.array_copy(T_STRING, T_STRING);
    s.end();
    f
}

fn strbuf() -> HeapType {
    HeapType::Concrete(T_STRBUF)
}

/// `$str_buf_get(v) -> jsval` (T78): a `$strbuf`'s string (made from its buffer once per run of
/// appends, then kept in `flat`); any other value as it is.
fn str_buf_get() -> Function {
    const V: u32 = 0;
    const B: u32 = 1;
    const R: u32 = 2;
    let mut f = Function::new([
        (1, ValType::Ref(RefType { nullable: true, heap_type: strbuf() })),
        (1, ValType::Ref(RefType { nullable: true, heap_type: string() })),
    ]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_nullable(strbuf()).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(V).return_();
    s.end();
    s.local_get(V).ref_cast_non_null(strbuf()).local_tee(B);
    s.struct_get(T_STRBUF, 2).local_tee(R).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(R).return_();
    s.end();
    s.local_get(B).struct_get(T_STRBUF, 1).array_new_default(T_STRING).local_set(R);
    s.local_get(R).i32_const(0);
    s.local_get(B).struct_get(T_STRBUF, 0).i32_const(0);
    s.local_get(B).struct_get(T_STRBUF, 1);
    s.array_copy(T_STRING, T_STRING);
    s.local_get(B).local_get(R).struct_set(T_STRBUF, 2);
    s.local_get(R);
    s.end();
    f
}

/// `$str_buf_append(v, s) -> jsval` (T78): string `s` appended to `v`, a `$strbuf` (in place; its
/// buffer doubles when full) or a string (a new `$strbuf` with room for as much again). Any other
/// `v` (not expected) gives `v + s`. Appending "" changes nothing.
fn str_buf_append(cx: &mut dyn Cx) -> Function {
    let js_add = cx.helper(Helper::JsAdd);
    const V: u32 = 0;
    const SV: u32 = 1;
    const B: u32 = 2;
    const S: u32 = 3;
    const N: u32 = 4;
    const NEW: u32 = 5;
    const NB: u32 = 6;
    let mut f = Function::new([
        (1, ValType::Ref(RefType { nullable: true, heap_type: strbuf() })),
        (1, ValType::Ref(RefType { nullable: true, heap_type: string() })),
        (2, ValType::I32),
        (1, ValType::Ref(RefType { nullable: true, heap_type: string() })),
    ]);
    let mut s = f.instructions();
    s.local_get(SV).ref_cast_non_null(string()).local_tee(S);
    s.array_len().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(V).return_();
    s.end();
    s.local_get(V).ref_test_nullable(strbuf());
    s.if_(BlockType::Empty);
    s.local_get(V).ref_cast_non_null(strbuf()).local_set(B);
    s.else_();
    s.test_string(V).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(V).local_get(SV).call(js_add).return_();
    s.end();
    // A new buffer holding `v`: room for twice what it will hold after this append (16 at least).
    s.local_get(V).ref_cast_non_null(string()).array_len().local_set(N);
    s.local_get(N).local_get(S).array_len().i32_add().i32_const(1).i32_shl();
    s.local_tee(NEW).i32_const(16).local_get(NEW).i32_const(16).i32_gt_u().select();
    s.array_new_default(T_STRING).local_set(NB);
    s.local_get(NB).i32_const(0);
    s.local_get(V).ref_cast_non_null(string()).i32_const(0);
    s.local_get(N);
    s.array_copy(T_STRING, T_STRING);
    s.local_get(NB).local_get(N).ref_null(string()).struct_new(T_STRBUF).local_set(B);
    s.end();
    // Grow the buffer when `s` doesn't fit: to twice the new length.
    s.local_get(B).struct_get(T_STRBUF, 1).local_get(S).array_len().i32_add().local_set(NEW);
    s.local_get(NEW);
    s.local_get(B).struct_get(T_STRBUF, 0).ref_as_non_null().array_len();
    s.i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(NEW).i32_const(1).i32_shl().array_new_default(T_STRING).local_set(NB);
    s.local_get(NB).i32_const(0);
    s.local_get(B).struct_get(T_STRBUF, 0).i32_const(0);
    s.local_get(B).struct_get(T_STRBUF, 1);
    s.array_copy(T_STRING, T_STRING);
    s.local_get(B).local_get(NB).struct_set(T_STRBUF, 0);
    s.end();
    s.local_get(B).struct_get(T_STRBUF, 0).local_get(B).struct_get(T_STRBUF, 1);
    s.local_get(S).i32_const(0).local_get(S).array_len();
    s.array_copy(T_STRING, T_STRING);
    s.local_get(B).local_get(NEW).struct_set(T_STRBUF, 1);
    s.local_get(B).ref_null(string()).struct_set(T_STRBUF, 2);
    s.local_get(B).ref_as_non_null();
    s.end();
    f
}

/// `$str_buf_len(v) -> f64` (T78): the length of a `$strbuf`'s string, or of string `v`.
fn str_buf_len() -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).ref_test_nullable(strbuf());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(0).ref_cast_non_null(strbuf()).struct_get(T_STRBUF, 1);
    s.else_();
    s.local_get(0).ref_cast_non_null(string()).array_len();
    s.end();
    s.f64_convert_i32_u();
    s.end();
    f
}

/// `$re_in_ranges(r, base, n, c) -> i32` (T78): binary search of `n` sorted pairs `[lo, hi]` of
/// `r` from `base` for `c`.
fn re_in_ranges() -> Function {
    const R: u32 = 0;
    const BASE: u32 = 1;
    const N: u32 = 2;
    const C: u32 = 3;
    const LO: u32 = 4;
    const MID: u32 = 5;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    // `N` is `hi` from here on.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(LO).local_get(N).i32_ge_u().br_if(1);
    s.local_get(LO).local_get(N).i32_add().i32_const(1).i32_shr_u().local_set(MID);
    s.local_get(C);
    s.local_get(R).local_get(BASE).local_get(MID).i32_const(1).i32_shl().i32_add();
    s.array_get(T_I32S);
    s.i32_lt_s();
    s.if_(BlockType::Empty);
    s.local_get(MID).local_set(N).br(1);
    s.end();
    s.local_get(C);
    s.local_get(R).local_get(BASE).local_get(MID).i32_const(1).i32_shl().i32_add();
    s.i32_const(1).i32_add().array_get(T_I32S);
    s.i32_gt_s();
    s.if_(BlockType::Empty);
    s.local_get(MID).i32_const(1).i32_add().local_set(LO).br(1);
    s.end();
    s.i32_const(1).return_();
    s.end().end();
    s.i32_const(0);
    s.end();
    f
}

/// `$re_alts_find(alts, classes, s, from, flags, ascii, wide, caps) -> i32` (T78): see
/// `Intrinsic::ReAltsFind`; regexp.js's `$$reAltsProg` describes `alts` and `classes`. With `i` the
/// pattern is ASCII only (its characters and class ranges), so only ASCII letters need folding: a
/// non-ASCII unit folds to a non-ASCII one, which no element matches but a negated class.
fn re_alts_find(cx: &mut dyn Cx) -> Function {
    let in_ranges = cx.helper(Helper::ReInRanges);
    const AV: u32 = 0;
    const CV: u32 = 1;
    const SV: u32 = 2;
    const FROM: u32 = 3;
    const FLAGS: u32 = 4;
    const FAV: u32 = 5;
    const FWV: u32 = 6;
    const CAPSV: u32 = 7;
    const A: u32 = 8;
    const CL: u32 = 9;
    const FA: u32 = 10;
    const FW: u32 = 11;
    const S: u32 = 12;
    const POS: u32 = 13;
    const LEN: u32 = 14;
    const COUNT: u32 = 15;
    const I: u32 = 16;
    const K: u32 = 17;
    const N: u32 = 18;
    const J: u32 = 19;
    const C: u32 = 20;
    const KIND: u32 = 21;
    const V: u32 = 22;
    const NWIDE: u32 = 23;
    let i32s = ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(T_I32S) });
    let mut f = Function::new([
        (4, i32s),
        (1, ValType::Ref(RefType { nullable: true, heap_type: string() })),
        (11, ValType::I32),
    ]);
    let mut s = f.instructions();
    let i32s_ht = HeapType::Concrete(T_I32S);
    s.local_get(AV).ref_cast_non_null(i32s_ht).local_set(A);
    s.local_get(CV).ref_cast_non_null(i32s_ht).local_set(CL);
    s.local_get(SV).ref_cast_non_null(string()).local_tee(S);
    s.array_len().local_set(LEN);
    // The filter (JS `null`: none). Not with `y`: only `from` is tried.
    s.local_get(FAV).ref_test_non_null(i32s_ht);
    s.local_get(FLAGS).i32_const(2).i32_and().i32_eqz();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(FAV).ref_cast_non_null(i32s_ht).local_set(FA);
    s.local_get(FWV).ref_cast_non_null(i32s_ht).local_tee(FW);
    s.array_len().i32_const(1).i32_shr_u().local_set(NWIDE);
    s.end();
    s.local_get(A).i32_const(0).array_get(T_I32S).local_set(COUNT);
    s.local_get(FROM).local_set(POS);
    // block 0: no match
    s.block(BlockType::Empty);
    // loop: each start position
    s.loop_(BlockType::Empty);
    // Every alternative needs a unit.
    s.local_get(POS).local_get(LEN).i32_ge_s().br_if(1);
    s.local_get(FA).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    // depths here: 0 scan loop, 1 found block, 2 if, 3 position loop, 4 no-match block
    s.local_get(POS).local_get(LEN).i32_ge_s().br_if(4);
    s.local_get(S).local_get(POS).array_get_u(T_STRING).local_tee(C);
    s.i32_const(128).i32_lt_u();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(FA).local_get(C).array_get(T_I32S);
    s.else_();
    s.local_get(NWIDE).i32_eqz();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(0);
    s.else_();
    s.local_get(FW).ref_as_non_null().i32_const(0).local_get(NWIDE).local_get(C).call(in_ranges);
    s.end();
    s.end();
    s.br_if(1);
    s.local_get(POS).i32_const(1).i32_add().local_set(POS);
    s.br(0);
    s.end().end();
    s.end();
    // The alternatives at POS, in order.
    s.i32_const(1).local_set(I);
    s.i32_const(0).local_set(K);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    // depths: 0 alternatives loop, 1 alternatives block, 2 position loop, 3 no-match block
    s.local_get(K).local_get(COUNT).i32_ge_s().br_if(1);
    s.local_get(A).local_get(I).array_get(T_I32S).local_set(N);
    s.block(BlockType::Empty);
    // depths: 0 this alternative fails, 1 alternatives loop
    s.local_get(POS).local_get(N).i32_add().local_get(LEN).i32_gt_s().br_if(0);
    s.i32_const(0).local_set(J);
    s.loop_(BlockType::Empty);
    // depths: 0 element loop, 1 this alternative fails
    s.local_get(J).local_get(N).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(CAPSV).ref_cast_non_null(i32s_ht).i32_const(0).local_get(POS).array_set(T_I32S);
    s.local_get(CAPSV).ref_cast_non_null(i32s_ht).i32_const(1);
    s.local_get(POS).local_get(N).i32_add().array_set(T_I32S);
    s.local_get(POS).local_get(N).i32_add().return_();
    s.end();
    s.local_get(S).local_get(POS).local_get(J).i32_add().array_get_u(T_STRING).local_set(C);
    // `i`: an ASCII lowercase letter as uppercase.
    s.local_get(FLAGS).i32_const(1).i32_and();
    s.local_get(C).i32_const(97).i32_sub().i32_const(26).i32_lt_u();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(C).i32_const(32).i32_sub().local_set(C);
    s.end();
    s.local_get(A).local_get(I).i32_const(1).i32_add().local_get(J).i32_const(1).i32_shl().i32_add();
    s.array_get(T_I32S).local_set(KIND);
    s.local_get(A).local_get(I).i32_const(2).i32_add().local_get(J).i32_const(1).i32_shl().i32_add();
    s.array_get(T_I32S).local_set(V);
    s.local_get(KIND).i32_const(1).i32_eq();
    s.if_(BlockType::Empty);
    // depths: 0 if, 1 element loop, 2 this alternative fails
    s.local_get(C).local_get(V).i32_ne().br_if(2);
    s.else_();
    s.local_get(CL).ref_as_non_null().local_get(V).i32_const(1).i32_add();
    s.local_get(CL).local_get(V).array_get(T_I32S);
    s.local_get(C).call(in_ranges);
    s.local_get(KIND).i32_const(3).i32_eq();
    s.i32_eq().br_if(2);
    s.end();
    s.local_get(J).i32_const(1).i32_add().local_set(J);
    s.br(0);
    s.end();
    s.end();
    // The next alternative.
    s.local_get(I).i32_const(1).i32_add().local_get(N).i32_const(1).i32_shl().i32_add().local_set(I);
    s.local_get(K).i32_const(1).i32_add().local_set(K);
    s.br(0);
    s.end().end();
    // No alternative at POS.
    s.local_get(FLAGS).i32_const(2).i32_and().br_if(1);
    s.local_get(POS).i32_const(1).i32_add().local_set(POS);
    s.br(0);
    s.end().end();
    s.i32_const(-1);
    s.end();
    f
}

/// `$json_quote_into(buf, i, s) -> i32` (T76): QuoteJSONString of `s` written into `buf` from `i`
/// (which has room for it); the index after the closing quote. Runs of units that need no escape
/// are copied with `array.copy`.
fn json_quote_into() -> Function {
    const BUF: u32 = 0;
    const AT: u32 = 1;
    const SV: u32 = 2;
    const B: u32 = 3;
    const S: u32 = 4;
    const LEN: u32 = 5;
    const I: u32 = 6;
    const FROM: u32 = 7;
    const U: u32 = 8;
    const E: u32 = 9;
    let mut f = Function::new([(2, string_ref()), (5, ValType::I32)]);
    let mut s = f.instructions();
    let set = |s: &mut InstructionSink, val: &dyn Fn(&mut InstructionSink)| {
        s.local_get(B).local_get(AT);
        val(s);
        s.array_set(T_STRING);
        s.local_get(AT).i32_const(1).i32_add().local_set(AT);
    };
    // Copy the run [FROM, I) to AT.
    let flush = |s: &mut InstructionSink| {
        s.local_get(B).local_get(AT).local_get(S).local_get(FROM);
        s.local_get(I).local_get(FROM).i32_sub();
        s.array_copy(T_STRING, T_STRING);
        s.local_get(AT)
            .local_get(I)
            .local_get(FROM)
            .i32_sub()
            .i32_add()
            .local_set(AT);
    };
    s.local_get(BUF).ref_cast_non_null(string()).local_set(B);
    s.local_get(SV).ref_cast_non_null(string()).local_tee(S);
    s.array_len().local_set(LEN);
    set(&mut s, &|s| {
        s.i32_const(34);
    });
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    s.local_get(S)
        .local_get(I)
        .array_get_u(T_STRING)
        .local_tee(U);
    // Needs an escape: < 32, `"`, `\`, or a surrogate (unless a pair).
    s.i32_const(32).i32_lt_u();
    s.local_get(U).i32_const(34).i32_eq().i32_or();
    s.local_get(U).i32_const(92).i32_eq().i32_or();
    s.local_get(U)
        .i32_const(0xf800)
        .i32_and()
        .i32_const(0xd800)
        .i32_eq()
        .i32_or();
    s.i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(1);
    s.end();
    // A high surrogate followed by a low one: part of the run.
    s.local_get(U)
        .i32_const(0xfc00)
        .i32_and()
        .i32_const(0xd800)
        .i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(I)
        .i32_const(1)
        .i32_add()
        .local_get(LEN)
        .i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(S)
        .local_get(I)
        .i32_const(1)
        .i32_add()
        .array_get_u(T_STRING);
    s.i32_const(0xfc00).i32_and().i32_const(0xdc00).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(I).i32_const(2).i32_add().local_set(I);
    s.br(3);
    s.end();
    s.end();
    s.end();
    flush(&mut s);
    // E: `\` and a letter or the unit itself, or -1 for `\u` and 4 hex digits.
    s.i32_const(98)
        .i32_const(116)
        .i32_const(110)
        .i32_const(102)
        .i32_const(114);
    s.local_get(U).i32_const(12).i32_eq().select();
    s.local_get(U).i32_const(10).i32_eq().select();
    s.local_get(U).i32_const(9).i32_eq().select();
    s.local_get(U).i32_const(8).i32_eq().select();
    s.i32_const(-1);
    s.local_get(U)
        .i32_const(8)
        .i32_sub()
        .i32_const(6)
        .i32_lt_u();
    s.local_get(U).i32_const(11).i32_ne().i32_and();
    s.select();
    // `"` and `\` escape as themselves.
    s.local_get(U);
    s.local_get(U).i32_const(34).i32_eq();
    s.local_get(U).i32_const(92).i32_eq().i32_or();
    s.i32_eqz().select().local_set(E);
    set(&mut s, &|s| {
        s.i32_const(92);
    });
    s.local_get(E).i32_const(0).i32_gt_s();
    s.if_(BlockType::Empty);
    set(&mut s, &|s| {
        s.local_get(E);
    });
    s.else_();
    set(&mut s, &|s| {
        s.i32_const('u' as i32);
    });
    for shift in [12, 8, 4, 0] {
        set(&mut s, &|s| {
            s.local_get(U)
                .i32_const(shift)
                .i32_shr_u()
                .i32_const(15)
                .i32_and()
                .local_tee(E);
            s.i32_const(48).i32_add();
            s.local_get(E).i32_const(87).i32_add();
            s.local_get(E).i32_const(10).i32_lt_u().select();
        });
    }
    s.end();
    s.local_get(I)
        .i32_const(1)
        .i32_add()
        .local_tee(I)
        .local_set(FROM);
    s.br(0).end().end();
    flush(&mut s);
    set(&mut s, &|s| {
        s.i32_const(34);
    });
    s.local_get(AT);
    s.end();
    f
}

/// `$json_scan(s, i, mode) -> i32` (T76): see `Intrinsic::JsonScan`.
fn json_scan() -> Function {
    const SV: u32 = 0;
    const I: u32 = 1;
    const MODE: u32 = 2;
    const S: u32 = 3;
    const LEN: u32 = 4;
    const U: u32 = 5;
    let mut f = Function::new([(1, string_ref()), (2, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(SV).ref_cast_non_null(string()).local_tee(S);
    s.array_len().local_set(LEN);
    s.local_get(MODE).i32_eqz();
    s.if_(BlockType::Empty);
    // Whitespace: space, \n, \r, \t.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    s.local_get(S)
        .local_get(I)
        .array_get_u(T_STRING)
        .local_tee(U);
    s.i32_const(32).i32_ne();
    s.local_get(U).i32_const(10).i32_ne().i32_and();
    s.local_get(U).i32_const(13).i32_ne().i32_and();
    s.local_get(U).i32_const(9).i32_ne().i32_and();
    s.br_if(1);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(I).return_();
    s.end();
    // A string's plain units.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    s.local_get(S)
        .local_get(I)
        .array_get_u(T_STRING)
        .local_tee(U);
    s.i32_const(34).i32_eq();
    s.local_get(U).i32_const(92).i32_eq().i32_or();
    s.local_get(U).i32_const(32).i32_lt_u().i32_or();
    s.br_if(1);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(I);
    s.end();
    f
}

/// `$js_add(a, b: jsval) -> jsval`: JS `+`. Objects become their ToPrimitive first. If either is
/// then a string, concatenate the ToString of both; otherwise add the ToNumber of both.
fn js_add(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(Helper::ToString);
    let concat = cx.helper(Helper::Concat);
    let to_number = cx.helper(Helper::ToNumber);
    let box_ = cx.helper(Helper::Box);
    let to_primitive = cx.helper(Helper::Obj(object::Helper::ToPrimitive));
    const I: u32 = 2;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    // T75: two i31s add as i32s (no overflow: each is in [-2^30, 2^30)); two numbers as f64s.
    s.local_get(0).ref_test_non_null(i31());
    s.local_get(1).ref_test_non_null(i31());
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(0).ref_cast_non_null(i31()).i31_get_s();
    s.local_get(1).ref_cast_non_null(i31()).i31_get_s();
    s.i32_add().local_tee(I);
    s.i32_const(I31_MIN).i32_ge_s();
    s.local_get(I).i32_const(I31_END).i32_lt_s().i32_and();
    s.if_(BlockType::Empty);
    s.local_get(I).ref_i31().return_();
    s.end();
    s.local_get(I)
        .f64_convert_i32_s()
        .struct_new(T_NUMBER)
        .return_();
    s.end();
    s.local_get(0).ref_test_non_null(i31());
    s.local_get(0).ref_test_non_null(number());
    s.i32_or();
    s.local_get(1).ref_test_non_null(i31());
    s.local_get(1).ref_test_non_null(number());
    s.i32_or();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(0).call(to_number);
    s.local_get(1).call(to_number);
    s.f64_add().call(box_).return_();
    s.end();
    // Two strings: concatenated as they are.
    is_string(&mut s, 0);
    is_string(&mut s, 1);
    s.i32_and();
    s.if_(BlockType::Empty);
    as_string(&mut s, 0);
    as_string(&mut s, 1);
    s.call(concat).return_();
    s.end();
    for v in [0, 1] {
        object_then(&mut s, v, |s| {
            s.i32_const(0).call(to_primitive).local_set(v);
        });
    }
    s.test_string(0);
    s.test_string(1);
    s.i32_or();
    s.if_(BlockType::Result(jsval()));
    s.local_get(0).call(to_string);
    s.local_get(1).call(to_string);
    s.call(concat);
    s.else_();
    if cx.bigint() {
        // T40: a BigInt adds only to a BigInt (the binary helper throws otherwise).
        let add = cx.helper(Helper::Big(bigint::Helper::Binary(js2wasi3_ir::BinOp::Add)));
        bigint::is_bigint(&mut s, 0);
        bigint::is_bigint(&mut s, 1);
        s.i32_or();
        s.if_(BlockType::Empty);
        s.local_get(0).local_get(1).call(add).return_();
        s.end();
    }
    s.local_get(0).call(to_number);
    s.local_get(1).call(to_number);
    s.f64_add().call(box_);
    s.end();
    s.end();
    f
}

/// `$inspect(v) -> (ref $string)`: how `console.log` shows a primitive: ToString, except
/// that `-0` shows as `-0`. An error shows as `[<ToString>]` (node, for an error without a stack).
/// A function shows as `[Function: name]` or `[Function (anonymous)]`.
/// Other objects trap (`inspect_object`): node's `util.inspect` format isn't implemented (F24).
fn inspect(cx: &mut dyn Cx) -> Function {
    let format = cx.helper(Helper::Fmt(inspect::Helper::Format));
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0)
        .i32_const(0)
        .i32_const(0)
        .i32_const(0)
        .call(format);
    s.end();
    f
}

/// `$no_format(v) -> jsval`: `v`, trapping if it's a string containing `%` (a `console.log`
/// format string, not implemented).
fn no_format() -> Function {
    const V: u32 = 0;
    const I: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    is_string(&mut s, V);
    s.if_(BlockType::Empty);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I);
    as_string(&mut s, V);
    s.array_len().i32_ge_u().br_if(1);
    as_string(&mut s, V);
    s.local_get(I)
        .array_get_u(T_STRING)
        .i32_const('%' as i32)
        .i32_eq();
    s.if_(BlockType::Empty);
    s.unreachable();
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.end();
    s.local_get(V);
    s.end();
    f
}

/// `$throw_read_error(v: jsval, prop: (ref $string))`: throw node's TypeError for reading a
/// property of `undefined` / `null`: "Cannot read properties of null (reading 'prop')".
fn throw_read(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(Helper::ToString);
    let concat = cx.helper(Helper::Concat);
    let throw = cx.helper(Helper::Obj(object::Helper::Throw(ErrorKind::TypeError)));
    let parts = [
        cx.str("Cannot read properties of "),
        cx.str(" (reading '"),
        cx.str("')"),
    ];
    const V: u32 = 0;
    const PROP: u32 = 1;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.global_get(parts[0]);
    s.local_get(V).call(to_string).call(concat);
    s.global_get(parts[1]).call(concat);
    s.local_get(PROP).call(concat);
    s.global_get(parts[2]).call(concat);
    s.call(throw);
    s.end();
    f
}

/// `$new_symbol(description)`: a `$symbol` whose content is the description (`undefined`: empty).
fn new_symbol(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(Helper::ToString);
    const D: u32 = 0;
    const S: u32 = 1;
    let mut f = Function::new([(
        1,
        ValType::Ref(RefType {
            nullable: true,
            heap_type: string(),
        }),
    )]);
    let mut s = f.instructions();
    s.local_get(D).global_get(G_UNDEFINED).ref_eq();
    s.if_(BlockType::Result(string_ref()));
    s.array_new_fixed(T_STRING, 0);
    s.else_();
    s.local_get(D).call(to_string);
    s.end();
    s.local_tee(S);
    s.array_len().array_new_default(T_SYMBOL);
    s.local_tee(D);
    s.ref_cast_non_null(symbol()).i32_const(0);
    s.local_get(S).ref_as_non_null().i32_const(0);
    s.local_get(S).ref_as_non_null().array_len();
    s.array_copy(T_SYMBOL, T_STRING);
    s.local_get(D);
    s.end();
    f
}

/// `$describe(v) -> (ref $string)`: see [`Helper::Describe`].
fn describe(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(Helper::ToString);
    let inspect = cx.helper(Helper::Inspect);
    let get = cx.helper(Helper::Obj(object::Helper::Get));
    let is_nullish = cx.helper(Helper::IsNullish);
    let ctor_name = cx.helper(Helper::Prop(crate::props::Helper::CtorName));
    let str_at = cx.helper(Helper::StrAt);
    let str_eq = cx.helper(Helper::StrEq);
    let slice = cx.helper(Helper::StrSlice);
    let concat = cx.helper(Helper::Concat);
    let (name_key, error, open, close) =
        (cx.str("name"), cx.str("Error"), cx.str(" ["), cx.str("]"));
    const V: u32 = 0;
    const H: u32 = 1;
    const N: u32 = 2;
    const C: u32 = 3;
    const LEN: u32 = 4;
    const I: u32 = 5;
    const T: u32 = 6;
    let mut f = Function::new([(3, rt(true, T_STRING)), (2, ValType::I32), (1, jsval())]);
    let mut s = f.instructions();
    object::is_error(&mut s, V);
    s.i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(V).call(inspect).return_();
    s.end();
    if cx.names("any") || cx.names("AggregateError") {
        // An error whose `stack` has no frames (`Promise.any`'s, T25): node inspects it.
        let own_get = cx.helper(Helper::Obj(object::Helper::OwnGet));
        let stack = cx.str("stack");
        s.local_get(V).ref_cast_non_null(object());
        s.global_get(stack).call(own_get).local_set(T);
        object::frameless_stack(cx, &mut s, V, T);
        s.if_(BlockType::Empty);
        s.local_get(V).call(inspect).return_();
        s.end();
    }
    s.local_get(V).call(to_string).local_set(H);
    // node's `improveStack`: an error whose constructor isn't named like it shows both,
    // `Ctor [name]: message` (or `Ctor: message` when the constructor's name contains it).
    s.local_get(V).global_get(name_key).call(get).local_tee(T);
    s.call(is_nullish);
    s.if_(BlockType::Result(string_ref()));
    s.global_get(error);
    s.else_();
    s.local_get(T).call(to_string);
    s.end();
    s.local_tee(N).ref_as_non_null().array_len().local_set(LEN);
    s.local_get(V)
        .ref_cast_non_null(object())
        .call(ctor_name)
        .local_tee(C);
    s.ref_is_null();
    s.if_(BlockType::Empty);
    s.local_get(H).ref_as_non_null().return_();
    s.end();
    // name ends with "Error", the header starts with name followed by ':' or nothing.
    s.local_get(LEN).i32_const(5).i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(H).ref_as_non_null().return_();
    s.end();
    s.local_get(N).ref_as_non_null().global_get(error);
    s.local_get(LEN).i32_const(5).i32_sub();
    s.call(str_at);
    s.local_get(H)
        .ref_as_non_null()
        .local_get(N)
        .ref_as_non_null();
    s.i32_const(0).call(str_at);
    s.i32_and();
    s.local_get(H)
        .ref_as_non_null()
        .array_len()
        .local_get(LEN)
        .i32_eq();
    s.local_get(H)
        .ref_as_non_null()
        .array_len()
        .local_get(LEN)
        .i32_gt_u();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(H)
        .ref_as_non_null()
        .local_get(LEN)
        .array_get_u(T_STRING)
        .i32_const(':' as i32)
        .i32_eq();
    s.else_();
    s.i32_const(0);
    s.end();
    s.i32_or();
    s.i32_and();
    s.i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(H).ref_as_non_null().return_();
    s.end();
    s.local_get(N)
        .ref_as_non_null()
        .local_get(C)
        .ref_as_non_null()
        .call(str_eq);
    s.if_(BlockType::Empty);
    s.local_get(H).ref_as_non_null().return_();
    s.end();
    // Does the constructor's name contain `name`?
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I)
        .local_get(LEN)
        .i32_add()
        .local_get(C)
        .ref_as_non_null()
        .array_len()
        .i32_gt_u()
        .br_if(1);
    s.local_get(C)
        .ref_as_non_null()
        .local_get(N)
        .ref_as_non_null()
        .local_get(I)
        .call(str_at);
    s.if_(BlockType::Empty);
    s.local_get(C).ref_as_non_null();
    s.local_get(H).ref_as_non_null().local_get(LEN).call(slice);
    s.call(concat).return_();
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(C)
        .ref_as_non_null()
        .global_get(open)
        .call(concat);
    s.local_get(N).ref_as_non_null().call(concat);
    s.global_get(close).call(concat);
    s.local_get(H).ref_as_non_null().local_get(LEN).call(slice);
    s.call(concat);
    s.end();
    f
}

fn str_at() -> Function {
    const H: u32 = 0;
    const N: u32 = 1;
    const P: u32 = 2;
    const I: u32 = 3;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(P)
        .local_get(N)
        .array_len()
        .i32_add()
        .local_get(H)
        .array_len()
        .i32_gt_u();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).array_len().i32_ge_u().br_if(1);
    s.local_get(H)
        .local_get(P)
        .local_get(I)
        .i32_add()
        .array_get_u(T_STRING);
    s.local_get(N).local_get(I).array_get_u(T_STRING);
    s.i32_ne();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.i32_const(1);
    s.end();
    f
}

fn str_slice() -> Function {
    const S: u32 = 0;
    const FROM: u32 = 1;
    const R: u32 = 2;
    let mut f = Function::new([(1, rt(true, T_STRING))]);
    let mut s = f.instructions();
    s.local_get(S)
        .array_len()
        .local_get(FROM)
        .i32_sub()
        .array_new_default(T_STRING)
        .local_set(R);
    s.local_get(R).ref_as_non_null().i32_const(0);
    s.local_get(S).local_get(FROM);
    s.local_get(S).array_len().local_get(FROM).i32_sub();
    s.array_copy(T_STRING, T_STRING);
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

/// `$string_sub(s, start, end) -> (ref $string)`: the units `[start, end)`.
fn str_sub() -> Function {
    const S: u32 = 0;
    const START: u32 = 1;
    const END: u32 = 2;
    const R: u32 = 3;
    let mut f = Function::new([(1, rt(true, T_STRING))]);
    let mut s = f.instructions();
    s.local_get(END)
        .local_get(START)
        .i32_sub()
        .array_new_default(T_STRING)
        .local_set(R);
    s.local_get(R).ref_as_non_null().i32_const(0);
    s.local_get(S).local_get(START);
    s.local_get(END).local_get(START).i32_sub();
    s.array_copy(T_STRING, T_STRING);
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

/// `$js_less(a, b) -> i32`: `Intrinsic::JsLess`. ToPrimitive (hint number) of `a`, then `b`; two
/// strings compare by code units, anything else as numbers.
fn js_less(cx: &mut dyn Cx, swap: bool, or_equal: bool) -> Function {
    let to_number = cx.helper(Helper::ToNumber);
    let to_primitive = cx.helper(Helper::Obj(object::Helper::ToPrimitive));
    const A: u32 = 0;
    const B: u32 = 1;
    const I: u32 = 2;
    const N: u32 = 3;
    const D: u32 = 4;
    let (x, y) = if swap { (B, A) } else { (A, B) };
    let mut f = Function::new([(3, ValType::I32)]);
    let mut s = f.instructions();
    // T75: two i31s compare as i32s.
    s.local_get(A).ref_test_non_null(i31());
    s.local_get(B).ref_test_non_null(i31());
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(x).ref_cast_non_null(i31()).i31_get_s();
    s.local_get(y).ref_cast_non_null(i31()).i31_get_s();
    if or_equal {
        s.i32_ge_s();
    } else {
        s.i32_lt_s();
    }
    s.return_();
    s.end();
    for v in [A, B] {
        s.test_object(v);
        s.if_(BlockType::Empty);
        s.local_get(v)
            .ref_cast_non_null(object())
            .i32_const(1)
            .call(to_primitive)
            .local_set(v);
        s.end();
    }
    both_strings(&mut s, A, B);
    s.local_get(A).ref_test_non_null(symbol());
    s.local_get(B).ref_test_non_null(symbol());
    s.i32_or().i32_eqz().i32_and();
    s.if_(BlockType::Empty);
    // D = the first differing unit's difference (x - y), else the length difference.
    s.local_get(x).ref_cast_non_null(string()).array_len();
    s.local_get(y).ref_cast_non_null(string()).array_len();
    s.local_get(x).ref_cast_non_null(string()).array_len();
    s.local_get(y).ref_cast_non_null(string()).array_len();
    s.i32_lt_u().select().local_set(N);
    s.local_get(x).ref_cast_non_null(string()).array_len();
    s.local_get(y).ref_cast_non_null(string()).array_len();
    s.i32_sub().local_set(D);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(x)
        .ref_cast_non_null(string())
        .local_get(I)
        .array_get_u(T_STRING);
    s.local_get(y)
        .ref_cast_non_null(string())
        .local_get(I)
        .array_get_u(T_STRING);
    s.i32_sub().local_tee(D).br_if(1);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.local_get(x).ref_cast_non_null(string()).array_len();
    s.local_get(y).ref_cast_non_null(string()).array_len();
    s.i32_sub().local_set(D);
    s.br(0).end().end();
    s.local_get(D).i32_const(0);
    if or_equal {
        s.i32_ge_s();
    } else {
        s.i32_lt_s();
    }
    s.return_();
    s.end();
    if cx.bigint() {
        // T40: a BigInt compares with a BigInt, a string (as a BigInt; not one: false) or a
        // number (by value; NaN: false). `c` is x's comparison with y: -1, 0, 1, or 2 (unordered).
        let big_cmp = cx.helper(Helper::Big(bigint::Helper::CmpAny));
        bigint::is_bigint(&mut s, A);
        bigint::is_bigint(&mut s, B);
        s.i32_or();
        s.if_(BlockType::Empty);
        s.local_get(x).local_get(y).call(big_cmp).local_tee(D);
        if or_equal {
            s.i32_const(0).i32_ge_s();
            s.local_get(D).i32_const(2).i32_ne().i32_and();
        } else {
            s.i32_const(-1).i32_eq();
        }
        s.return_();
        s.end();
    }
    s.local_get(A).call(to_number);
    s.local_get(B).call(to_number);
    if swap {
        // (b, a) on the stack order: compare y op x with y = a.
        if or_equal {
            s.f64_le()
        } else {
            s.f64_gt()
        };
    } else if or_equal {
        s.f64_ge();
    } else {
        s.f64_lt();
    }
    s.end();
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typeof_codes_match_oddball_kinds() {
        assert_eq!(typeof_code(TypeofTag::Undefined), K_UNDEFINED);
        assert_eq!(typeof_code(TypeofTag::Object), K_NULL);
        assert_eq!(typeof_code(TypeofTag::Boolean), K_FALSE);
        assert_eq!(typeof_code(TypeofTag::Number), 3);
    }

    #[test]
    fn small_ints() {
        assert_eq!(small_int(0.0), Some(0));
        assert_eq!(small_int(-0.0), None);
        assert_eq!(small_int(1.5), None);
        assert_eq!(small_int(f64::NAN), None);
        assert_eq!(small_int(-1073741824.0), Some(-1073741824));
        assert_eq!(small_int(1073741823.0), Some(1073741823));
        assert_eq!(small_int(1073741824.0), None);
        assert_eq!(small_int(1e300), None);
    }
}

/// `$well_known_symbol<k>()` (T25): `Symbol.<name>`, one per program, described `Symbol.<name>`.
fn well_known_symbol(cx: &mut dyn Cx, k: u8) -> Function {
    // T45: `dispose`/`asyncDispose` have slots only when the code names them.
    let n = if cx.names("dispose") || cx.names("asyncDispose") || cx.stacks() { 14 } else { 12 };
    let desc: Vec<u16> = format!("Symbol.{}", js2wasi3_ir::WELL_KNOWN_SYMBOLS[k as usize])
        .encode_utf16()
        .collect();
    let _ = cx;
    const S: u32 = 0;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.global_get(G_WELL_KNOWN).ref_is_null();
    s.if_(BlockType::Empty);
    s.i32_const(n)
        .array_new_default(T_VALS)
        .global_set(G_WELL_KNOWN);
    s.end();
    s.global_get(G_WELL_KNOWN)
        .ref_as_non_null()
        .i32_const(k as i32)
        .array_get(T_VALS)
        .local_tee(S)
        .ref_is_null();
    s.if_(BlockType::Empty);
    for u in &desc {
        s.i32_const(*u as i32);
    }
    s.array_new_fixed(T_SYMBOL, desc.len() as u32).local_set(S);
    s.global_get(G_WELL_KNOWN)
        .ref_as_non_null()
        .i32_const(k as i32)
        .local_get(S)
        .array_set(T_VALS);
    s.end();
    s.local_get(S);
    s.end();
    f
}
