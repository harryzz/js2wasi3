//! IR: the single mid-level representation between the frontend and the backend.
//! JS-aware (closures, exceptions, suspension points) but independent of Wasm encoding.
//!
//! # Shape
//! A **structured tree**: statements nest like Wasm `block`/`loop`/`if`, so the backend
//! emits them directly, with no CFG and no relooper. Every function owns a flat, numbered
//! local list (params first, like Wasm), which keeps later liveness passes (the T18
//! suspension transform) cheap. The IR holds no oxc types. Source positions are plain [`Pos`].
//!
//! # Node kinds
//! Statements ([`Stmt`]):
//! - `Expr(e)`: evaluate `e` and discard its value.
//! - `Set(l, e)` / `SetGlobal(g, e)`: assign to a local / global.
//! - `Block { label, body }`: `Break(label)` jumps to just after the block.
//! - `Loop { label, body }`: `Continue(label)` jumps to the start of the body, `Break(label)`
//!   jumps to just after the loop. The body does not repeat implicitly: it ends with an
//!   explicit `Continue`, or the loop exits after one pass (matching Wasm's `loop`).
//! - `If { cond, then, els }`: `cond` has type `Bool`.
//! - `Break(label)` / `Continue(label)`: labels must name an enclosing `Block`/`Loop`
//!   (`Continue` only a `Loop`).
//! - `Return(e?)`.
//! - `Try { body, catch_local, catch_body }`: JS `try/catch`. A JS value thrown in `body` (by
//!   `Throw`, a callee or a runtime helper) is stored into `catch_local` (a `jsval` local, if any),
//!   then `catch_body` runs. Wasm traps are never caught. `finally` is lowered away before
//!   reaching the IR (T12: a completion local and a dispatch after one copy of the finally body).
//! - `Throw(e)`: throw the JS value `e` (`jsval`).
//!
//! Expressions ([`Expr`]):
//! - `Const(c)`: `f64`, `i32` or `bool` literal, or `undefined` / `null` (type `jsval`).
//! - `Local(l)` / `Global(g)`: read.
//! - `Tee(l, e)`: assign to a local and yield the value (JS assignment expressions).
//! - `Unary { op, ty, arg }` / `Binary { op, ty, lhs, rhs }`: machine ops on operands of type
//!   `ty` (`F64` or `I32`, `Bool` for logical `Not`/`Eq`/`Ne`). Comparisons yield `Bool`.
//!   These are machine ops, except on `JsVal`: `Eq`/`Ne` are JS `===`/`!==` and `LooseEq`/`LooseNe`
//!   are JS `==`/`!=` (both only on `JsVal`). Arithmetic on JS values goes through `JsVal → F64` first.
//! - `TypeofIs { arg, tag }`: `typeof arg === "<tag>"`, as `Bool` (`arg` is `JsVal`).
//! - `Str(units)`: a string literal (UTF-16 code units), type `JsVal`.
//! - `Intrinsic { op, args }`: a runtime operation with a fixed signature ([`Intrinsic::signature`]),
//!   e.g. JS `+` on JS values or ToString. Some have no result (writing to stdout), like a void call.
//! - `Seq { body, value }`: run `body`, then evaluate `value`. `body` must not `Break`/`Continue` out.
//! - `Convert { from, to, arg }`: `F64 → I32` is JS `ToInt32` (modulo 2^32, NaN/±inf → 0),
//!   not a saturating truncation. `I32 → F64` is signed. `F64 → JsVal` boxes a number, `JsVal → F64` is
//!   JS `ToNumber`, `Bool → JsVal` picks the `true`/`false` value, `JsVal → Bool` is JS `ToBoolean`.
//!   (T08: primitives only; strings and objects extend the runtime helpers.)
//! - `Cond { cond, then, els, ty }`: conditional expression (`?:`, `&&`, `||` after lowering).
//! - `Call { func, args }`: direct call.
//! - `Closure(c)` (T14): a new function object ([`Closure`]), a `jsval`.
//! - `Array(elems)` (T15): a new array of the `jsval` elements (`None`: a hole).
//! - `CallRef { callee, this, args, what }` (T14): call the JS value `callee` with `this` and the
//!   arguments (all `jsval`), a `jsval`. A TypeError "`what` is not a function" if it isn't one.
//! - `New { callee, new_target, args, what }` (T16): `new callee(...args)` (`new_target` defaults to
//!   the callee), a `jsval`. A TypeError "`what` is not a constructor" if it isn't one.
//! - Cells (T14): a captured variable lives in a cell, an internal `jsval` made by
//!   `Intrinsic::NewCell` and used by `CellGet`/`CellSet`. A function object's captured cells and
//!   values are its environment (`EnvGet`), and a function called through a value gets its
//!   arguments as an internal array (`ArgGet`).
//! - `Suspend { kind, arg }` (T18): a `yield` (T19: `await`) of `arg` in a generator (an async
//!   function). It evaluates to the value the generator is resumed with; `Intrinsic::ResumeMode`, read right after it, says how
//!   (next / throw / return: the lowering emits the throw and the return path itself). The
//!   suspension transform ([`suspend`]) removes every `Suspend` before the IR reaches the backend.

use std::fmt;

mod print;
pub mod suspend;
mod throws;
mod validate;

pub use throws::{can_queue, may_throw};
pub use validate::validate;

/// Value types. Wasm types get chosen in the backend: `Bool` is an `i32` 0/1, `JsVal` is `(ref null eq)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Type {
    F64,
    I32,
    Bool,
    /// Dynamic JS value (T08).
    JsVal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlobalId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FuncId(pub u32);
/// Unique within one function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LabelId(pub u32);

/// 1-based source position, for diagnostics about unsupported IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Module {
    pub functions: Vec<Function>,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
    /// T109: object layouts, each a constructor's `this.k = …` keys in order (1 to 8 keys): see
    /// [`Intrinsic::CreateThisLayout`] and [`Intrinsic::LayoutSet`].
    pub layouts: Vec<Vec<String>>,
    /// The command entry point (a `wasi:cli/run` command runs it), for a module without exports.
    /// Takes no params and has no result.
    pub main: Option<FuncId>,
    /// A library's top-level code (T20, F11): module evaluation, run once before the first
    /// export call. Takes no params and has no result.
    pub init: Option<FuncId>,
    /// Built-in prototype methods implemented by the prelude (T21): installed on their prototype
    /// in place of the sentinels.
    pub builtin_methods: Vec<BuiltinMethod>,
    /// The prelude's generic `Array.prototype` `push`/`pop`/`join`/`toString` (T35): the native
    /// methods call these `$code` functions when `this` isn't an array. (key, code)
    pub array_generic: Vec<(String, FuncId)>,
    /// Built-in constructors used as values (T25): `proto` is the built-in, `code` its prelude
    /// function (`$$Array`), called and constructed; `key` is unused.
    pub builtin_ctors: Vec<BuiltinMethod>,
    /// Statics of those constructors implemented by the prelude (`Array.from`): own properties of
    /// the constructor value.
    pub builtin_statics: Vec<BuiltinMethod>,
    /// The `Promise` statics implemented by the prelude (T50, `builtins/promise.js`): own methods
    /// of the native `Promise` value, in place of native ones of the same key.
    pub promise_statics: Vec<BuiltinMethod>,
    /// `build --unhandled-rejections=none` (T50): unhandled rejections are ignored.
    pub ignore_unhandled: bool,
    /// `build --gc-reserve=<MiB>` (T98): the start function allocates (and drops) a byte array of this
    /// many MiB, so wasmtime's GC heap grows once and later collections are rarer (0: none).
    pub gc_reserve_mib: u32,
    /// `build --count` (T112): fast paths count their hits and misses, printed to stderr at exit.
    pub count: bool,
    /// T132: method-call cache sites ([`Intrinsic::MethodCheck`]); the start function sizes their flags for it.
    pub method_sites: u32,
    /// T114: regex literals compiled at build time to programs for the runtime's regex interpreter
    /// (`Intrinsic::RegexId`, `Intrinsic::ReExec`; format in `lower/src/regex.rs`); a literal outside the
    /// subset isn't listed and runs in the prelude's VM.
    pub regexes: Vec<Vec<u16>>,
    /// `build --regex-vm` (T114): every regex runs in the prelude's VM (`RegexId` is `null`).
    pub regex_vm: bool,
    /// The entry is a CommonJS module (T60): after its code node runs the `nextTick` queue before
    /// the microtasks (after an ES module's, the microtasks first).
    pub cjs_main: bool,
    /// The code names a `constructor` property (T25): the built-in prototypes of
    /// [`Builtin::ctor_length`] without a constructor in [`Module::builtin_ctors`] get a stub one
    /// (identity, `name`, `length`, `prototype`; calling it traps).
    pub ctor_stubs: bool,
    /// `Math`/`JSON` used as values (T25): their prelude functions the code names, as own
    /// methods (`key` is the property; `proto` unused, `Object`).
    pub namespace_methods: Vec<(Namespace, BuiltinMethod)>,
    /// `Math` or `JSON` is used as a value: `console.log` checks for them.
    pub namespaces: bool,
    /// The code names `Promise` (T25: the run-time `Symbol.toStringTag` checks look for promises).
    pub promises: bool,
    /// Property names in the user's code that gate native built-in methods (T25: `keys`,
    /// `entries`): see `js2wasi3_lower`'s `GATED`.
    pub names: std::collections::BTreeSet<String>,
    /// The prelude's StringToNumber `(JsVal) -> F64` (T21), called by the runtime's ToNumber on a
    /// string; without it, that traps.
    pub string_to_number: Option<FuncId>,
    /// The prelude's `$$DateCall` adapter (T26): the call entry of `Date` as a value (`Date()` gives a
    /// string; `new Date` runs `$$Date`).
    pub date_call: Option<FuncId>,
    /// The prelude's `$$dateInspect(d) -> string` (T26): `console.log`'s text of a Date.
    pub date_inspect: Option<FuncId>,
    /// The prelude's `$$processAtExit(status)` (T55, `process`): the event loop calls it once when
    /// the program ends (`status`: 1 after an uncaught exception, else 0) to emit `'exit'` and
    /// apply `process.exitCode`.
    pub process_at_exit: Option<FuncId>,
    /// The prelude's `$$inspectCustom(v, depth)` (T56): the formatter calls it first for every
    /// object; it gives `v` itself unless `v` has a `util.inspect.custom` method, else its result.
    pub inspect_custom: Option<FuncId>,
    /// The module can make a `Map`/`Set` (T25): `console.log` links their formatting.
    pub maps: bool,
    /// The prelude's `$$StreamReader(h, id)` and `$$StreamPump(iterable, w, id)` (T20 slice 6b,
    /// JS-value params): a lifted stream is a reader (an async generator), a lowered one is
    /// pumped. Set when the user world has a `stream`.
    pub stream_reader: Option<FuncId>,
    pub stream_pump: Option<FuncId>,
    /// T59, a command's blocking WASI imports: `$$StreamReadAll(h, id)` reads a returned stream to
    /// its end (one `list<T>`-shaped value), `$$StreamDefer(v, w, id)` keeps a stream argument's
    /// value to write after the call, `$$StreamFlush()` writes them.
    pub stream_read_all: Option<FuncId>,
    pub stream_defer: Option<FuncId>,
    pub stream_flush: Option<FuncId>,
    /// The code names a property `bind` (T25): `Function.prototype.bind` is installed.
    pub bind: bool,
    /// The code names a property `toString` (T25): `Function.prototype.toString` is installed and
    /// user functions carry their source text ([`Closure::source`]).
    pub to_string: bool,
    /// Functions a user world imports (T20 slice 5), called with [`Intrinsic::CallImport`].
    pub imports: Vec<Import>,
    /// The code can make a BigInt (T40: a BigInt literal, or it names the global `BigInt`): the
    /// runtime's `$bigint` type and its cases in the generic helpers exist only then.
    pub bigint: bool,
    /// The code can make a `SuppressedError` (T45: it names it, or uses `using`): its prototype and
    /// constructor globals exist only then (after the BigInt ones, which are then there too).
    pub suppressed: bool,
    /// The code names `DisposableStack` or `AsyncDisposableStack` (T45): their prototype and
    /// constructor globals exist only then (after `SuppressedError`'s, which are then there too).
    pub stacks: bool,
    /// The code can make a resizable `ArrayBuffer` or detach one (T47: its source has
    /// `maxByteLength` or `transfer`): `$buffer` and `$typed` have the fields for it, and every
    /// native reader of a typed array's length checks IsTypedArrayOutOfBounds, only then.
    pub rab: bool,
    /// The code has a class with `extends`, or names `construct` (T47): only then can a
    /// constructor value be called with another new target, so only then do the constructor values
    /// of `Map`/`Set`/`ArrayBuffer`/typed arrays take its prototype (`func::Helper::ConstructFrom`).
    pub subclassing: bool,
    /// The code names `BigInt64Array` or `BigUint64Array` (T48; implies [`Module::bigint`]): typed
    /// array kinds 9–10 and their prototype and constructor globals exist only then.
    pub big_typed: bool,
    /// `Symbol` is a value (T49): the well-known symbols (`iterator` and [`WELL_KNOWN_SYMBOLS`])
    /// the code names, which are its own properties (constants).
    pub symbol_statics: Vec<String>,
    /// The prelude's ToBigInt `(JsVal) -> JsVal` (T48): element writes of the BigInt arrays. Set with
    /// [`Module::big_typed`].
    pub to_bigint: Option<FuncId>,
    /// The prelude's StringToBigInt `(JsVal) -> JsVal` (T40): a BigInt, or `undefined` if the
    /// string isn't a StringIntegerLiteral. Set with [`Module::bigint`].
    pub string_to_bigint: Option<FuncId>,
    /// T127: the prelude's NewPromiseCapability `(c: JsVal) -> JsVal` (`[promise, resolve, reject]`), for the native
    /// `then` on a promise whose species isn't `%Promise%`. Only in modules with built-in subclasses that name
    /// `then`/`catch`/`finally`.
    pub promise_capability: Option<FuncId>,
}

/// A function a user world imports (T20 slice 5): core import `name` of module `module` (the
/// interface's name, as wit-component expects it; an `async func` adds `[async-lower]`).
/// `sig.result` is the whole WIT result (a top-level `result` too; `throws` is unused).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub sig: AbiSig,
    /// A top-level `result`'s unwrapping `(result: jsval) -> jsval`: `ok`'s value, or throws
    /// `err`'s. An `async func` (`sig.is_async`, `[async-lower]`) calls it when the subtask
    /// returns, settling the call's promise with what it gives or throws.
    pub unwrap: Option<FuncId>,
}

/// A prototype method implemented in the prelude: `proto.prototype[key]` is a function object
/// named `key` with this `length`, running `code` (the `$code` signature).
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltinMethod {
    pub proto: Builtin,
    pub key: String,
    pub code: FuncId,
    pub length: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
    pub init: Const,
}

/// A function exported from the component under `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub func: FuncId,
    /// `name` was written as a string literal in the source (`export { f as "my-f" }`).
    pub string_name: bool,
    pub pos: Option<Pos>,
    /// Types at the component boundary. `func` takes [`AbiType::param_type`]s and returns an
    /// [`AbiType::result_type`] (an async export: its promise).
    pub sig: AbiSig,
}

/// A value type at the component boundary (a WIT type). Generated worlds use `F64` and
/// `String`; a user world (T20) any scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbiType {
    Bool,
    S8,
    U8,
    S16,
    U16,
    S32,
    U32,
    S64,
    U64,
    F32,
    F64,
    Char,
    String,
    /// A compound WIT type (T20 slice 4b): [`AbiSig::types`]`[i]`, passed as a JS value.
    Compound(u32),
}

/// A compound WIT type of a user world (T20 slice 4b) and its JS shape (jco's):
/// `option` ↔ the value or `undefined`, `result` (not at the top) ↔ `{ tag: "ok" | "err", val }`,
/// `tuple` ↔ an array, `record` / `flags` ↔ an object (keys: the camelCase field / flag names),
/// `enum` ↔ the case's WIT name; slice 4b-2: `variant` ↔ `{ tag, val }` (`tag` the case's WIT
/// name, no `val` for a case without a payload), `list<T>` ↔ an array, or a typed array for the
/// numeric `T`s up to 32 bits and `f64` ([`WitType::typed_kind`]: `list<u8>` ↔ `Uint8Array`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WitType {
    Scalar(AbiType),
    Option(Box<WitType>),
    Result {
        ok: Option<Box<WitType>>,
        err: Option<Box<WitType>>,
    },
    Tuple(Vec<WitType>),
    /// Fields: (JS key, type).
    Record(Vec<(String, WitType)>),
    /// Case names.
    Enum(Vec<String>),
    /// Flag JS keys (at most 32).
    Flags(Vec<String>),
    /// Cases: (WIT name, payload).
    Variant(Vec<(String, Option<WitType>)>),
    List(Box<WitType>),
    /// `future<T>` (T20 slice 6a) ↔ a Promise (lowered: any value, awaited, then written).
    Future(Box<Payload>),
    /// `stream<T>` (T20 slice 6b) ↔ an async iterable of chunks shaped like `list<T>`
    /// (`stream<u8>`: `Uint8Array`s). Lifted: an async generator reading the stream; lowered:
    /// any (async) iterable, pumped into a new stream.
    Stream(Box<Payload>),
}

/// A `future` or `stream` type of one function (T20 slice 6): the core module and function name
/// its intrinsics are imported with (`[future-new-<index>]<func>` from `module`), and the value
/// (a stream's element) type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Payload {
    /// `[export]$root`, `[export]ns:pkg/iface`, or an imported interface's module.
    pub module: String,
    pub func: String,
    /// Its position in the function's futures and streams (wit-parser's
    /// `find_futures_and_streams`).
    pub index: u32,
    pub elem: Option<WitType>,
    /// A `stream` (its `elem` is always set), not a `future`.
    pub stream: bool,
}

impl WitType {
    /// The typed array kind (an index into [`TYPED_NAMES`]) a `list` of this element type is, as
    /// in jco (64-bit integers would be `BigInt64Array`s: plain arrays here, F124).
    pub fn typed_kind(&self) -> Option<u8> {
        Some(match self {
            WitType::Scalar(AbiType::S8) => 0,
            WitType::Scalar(AbiType::U8) => 1,
            WitType::Scalar(AbiType::S16) => 3,
            WitType::Scalar(AbiType::U16) => 4,
            WitType::Scalar(AbiType::S32) => 5,
            WitType::Scalar(AbiType::U32) => 6,
            WitType::Scalar(AbiType::F32) => 7,
            WitType::Scalar(AbiType::F64) => 8,
            _ => return None,
        })
    }

    /// Does a value of this type contain a typed array?
    pub fn has_typed(&self) -> bool {
        match self {
            WitType::Scalar(_) | WitType::Enum(_) | WitType::Flags(_) => false,
            WitType::Option(t) => t.has_typed(),
            WitType::Result { ok, err } => {
                ok.as_deref().is_some_and(WitType::has_typed)
                    || err.as_deref().is_some_and(WitType::has_typed)
            }
            WitType::Tuple(ts) => ts.iter().any(WitType::has_typed),
            WitType::Record(fs) => fs.iter().any(|(_, t)| t.has_typed()),
            WitType::Variant(cs) => cs
                .iter()
                .any(|(_, t)| t.as_ref().is_some_and(WitType::has_typed)),
            WitType::List(t) => t.typed_kind().is_some() || t.has_typed(),
            WitType::Future(p) => p.elem.as_ref().is_some_and(WitType::has_typed),
            WitType::Stream(p) => p
                .elem
                .as_ref()
                .is_some_and(|t| t.typed_kind().is_some() || t.has_typed()),
        }
    }

    /// The futures and streams in this type, in order.
    pub fn payloads<'a>(&'a self, out: &mut Vec<&'a Payload>) {
        match self {
            WitType::Scalar(_) | WitType::Enum(_) | WitType::Flags(_) => {}
            WitType::Option(t) | WitType::List(t) => t.payloads(out),
            WitType::Result { ok, err } => {
                for t in [ok, err].into_iter().flatten() {
                    t.payloads(out);
                }
            }
            WitType::Tuple(ts) => ts.iter().for_each(|t| t.payloads(out)),
            WitType::Record(fs) => fs.iter().for_each(|(_, t)| t.payloads(out)),
            WitType::Variant(cs) => cs.iter().flat_map(|(_, t)| t).for_each(|t| t.payloads(out)),
            WitType::Future(p) | WitType::Stream(p) => {
                if let Some(t) = &p.elem {
                    t.payloads(out);
                }
                out.push(p);
            }
        }
    }
}

/// The `err` of a throwing export's `result`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ErrType {
    /// A generated world's `js-error` record, made from the exception (T12).
    #[default]
    JsError,
    /// A user world's `result<T>` / `result<T, E>` (T20): no payload, or the thrown value's
    /// (jco's convention: an own `payload` property, else a thrown Error traps, else the value).
    Empty,
    Of(AbiType),
}

impl AbiType {
    /// The IR type an exported function takes for it: a number (`f64`), a boolean, or a JS
    /// string (`char` too).
    pub fn param_type(self) -> Type {
        match self {
            AbiType::Bool => Type::Bool,
            AbiType::Char | AbiType::String | AbiType::Compound(_) => Type::JsVal,
            _ => Type::F64,
        }
    }

    /// The IR type an exported function returns for it: the integers up to 32 bits as JS
    /// ToInt32 (`i32`; the adapter narrows them), other numbers as `f64`.
    pub fn result_type(self) -> Type {
        match self {
            AbiType::S8
            | AbiType::U8
            | AbiType::S16
            | AbiType::U16
            | AbiType::S32
            | AbiType::U32 => Type::I32,
            t => t.param_type(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AbiSig {
    pub params: Vec<AbiType>,
    pub result: Option<AbiType>,
    /// The function may throw a JS value: the WIT result is `result<T, js-error>` (T12).
    pub throws: bool,
    /// The call may queue promise jobs (T20, F65): they run after it, and an unhandled rejection
    /// fails the call (`throws` is set too).
    pub jobs: bool,
    /// An `async` function (T20): a WIT `async func`, lifted with the callback ABI; `result` is
    /// what the promise it returns settles to (`throws` is set).
    pub is_async: bool,
    /// With `throws`: the `result`'s `err` (T20).
    pub err: ErrType,
    /// A command's WASI import (T59): sync-lowered (an `async func` blocks), its streams and
    /// futures read and written to the end around the call.
    pub blocking: bool,
    /// The compound types [`AbiType::Compound`] refers to (T20).
    pub types: Vec<WitType>,
}

impl AbiSig {
    /// The WIT type of `t` (a compound one from [`AbiSig::types`]).
    pub fn wit(&self, t: AbiType) -> WitType {
        match t {
            AbiType::Compound(i) => self.types[i as usize].clone(),
            t => WitType::Scalar(t),
        }
    }
}

impl AbiSig {
    /// All `f64` (the T07/T08 boundary), for a function with `params` parameters.
    pub fn numbers(params: u32, result: bool) -> AbiSig {
        AbiSig {
            params: vec![AbiType::F64; params as usize],
            result: result.then_some(AbiType::F64),
            throws: false,
            jobs: false,
            is_async: false,
            err: ErrType::JsError,
            types: Vec::new(),
            blocking: false,
        }
    }
}

/// A native error constructor (`new TypeError(m)`): the `name` of the errors it creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    Error,
    TypeError,
    RangeError,
    ReferenceError,
    SyntaxError,
    EvalError,
    UriError,
    /// T25: `new AggregateError(errors, message, options)`.
    AggregateError,
    /// T45: `new SuppressedError(error, suppressed, message)`. Not in [`ErrorKind::ALL`] (the runtime's
    /// global layout): its prototype and constructor globals exist only when [`Module::suppressed`].
    SuppressedError,
}

impl ErrorKind {
    pub const ALL: [ErrorKind; 8] = [
        ErrorKind::Error,
        ErrorKind::TypeError,
        ErrorKind::RangeError,
        ErrorKind::ReferenceError,
        ErrorKind::SyntaxError,
        ErrorKind::EvalError,
        ErrorKind::UriError,
        ErrorKind::AggregateError,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ErrorKind::Error => "Error",
            ErrorKind::TypeError => "TypeError",
            ErrorKind::RangeError => "RangeError",
            ErrorKind::ReferenceError => "ReferenceError",
            ErrorKind::SyntaxError => "SyntaxError",
            ErrorKind::EvalError => "EvalError",
            ErrorKind::UriError => "URIError",
            ErrorKind::AggregateError => "AggregateError",
            ErrorKind::SuppressedError => "SuppressedError",
        }
    }

    pub fn parse(s: &str) -> Option<ErrorKind> {
        if s == "SuppressedError" {
            return Some(ErrorKind::SuppressedError);
        }
        ErrorKind::ALL.into_iter().find(|k| k.name() == s)
    }
}

/// A namespace object of built-in functions (T25: `Math`, `JSON` as values; T36: `Reflect`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    Math,
    Json,
    Reflect,
}

impl Namespace {
    pub const ALL: [Namespace; 3] = [Namespace::Math, Namespace::Json, Namespace::Reflect];

    pub fn name(self) -> &'static str {
        match self {
            Namespace::Math => "Math",
            Namespace::Json => "JSON",
            Namespace::Reflect => "Reflect",
        }
    }

    pub fn parse(s: &str) -> Option<Namespace> {
        Namespace::ALL.into_iter().find(|n| n.name() == s)
    }

    pub fn index(self) -> u32 {
        Namespace::ALL.iter().position(|n| *n == self).unwrap() as u32
    }
}

/// The well-known symbols other than `Symbol.iterator` (T25), by index.
pub const WELL_KNOWN_SYMBOLS: [&str; 14] = [
    "asyncIterator",
    "hasInstance",
    "isConcatSpreadable",
    "match",
    "matchAll",
    "replace",
    "search",
    "species",
    "split",
    "toPrimitive",
    "toStringTag",
    "unscopables",
    // T45: the array holding them has these two slots only when the code names them (so other
    // programs keep their bytes).
    "dispose",
    "asyncDispose",
];

/// The index of well-known symbol `Symbol.<name>` in [`WELL_KNOWN_SYMBOLS`].
pub fn well_known_symbol(name: &str) -> Option<u8> {
    WELL_KNOWN_SYMBOLS
        .iter()
        .position(|n| *n == name)
        .map(|k| k as u8)
}

/// `Math`'s constants.
pub const MATH_CONSTANTS: [(&str, f64); 8] = [
    ("E", std::f64::consts::E),
    ("LN10", std::f64::consts::LN_10),
    ("LN2", std::f64::consts::LN_2),
    ("LOG10E", std::f64::consts::LOG10_E),
    ("LOG2E", std::f64::consts::LOG2_E),
    ("PI", std::f64::consts::PI),
    ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
    ("SQRT2", std::f64::consts::SQRT_2),
];

/// `Number`'s constants.
pub const NUMBER_CONSTANTS: [(&str, f64); 8] = [
    ("MAX_SAFE_INTEGER", 9007199254740991.0),
    ("MIN_SAFE_INTEGER", -9007199254740991.0),
    ("EPSILON", f64::EPSILON),
    ("MAX_VALUE", f64::MAX),
    ("MIN_VALUE", 5e-324),
    ("POSITIVE_INFINITY", f64::INFINITY),
    ("NEGATIVE_INFINITY", f64::NEG_INFINITY),
    ("NaN", f64::NAN),
];

/// A built-in constructor whose prototype object the runtime provides (T13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    Object,
    String,
    Number,
    Boolean,
    Error(ErrorKind),
    Function,
    Array,
    Symbol,
    /// `%ArrayIteratorPrototype%` (not a global name).
    ArrayIterator,
    /// T17.
    Promise,
    /// `%GeneratorPrototype%` (T18, not a global name).
    Generator,
    /// T25.
    Map,
    Set,
    /// `%MapIteratorPrototype%`, `%SetIteratorPrototype%` (T25, not global names).
    MapIterator,
    SetIterator,
    /// node's `Timeout` and `Immediate` classes (T25, not global names).
    Timeout,
    Immediate,
    /// `%AsyncGeneratorPrototype%` (T20, not a global name).
    AsyncGenerator,
    /// T20: `ArrayBuffer`, `%TypedArray%` (not a global name) and the typed arrays (kind: an
    /// index into [`TYPED_NAMES`]).
    ArrayBuffer,
    TypedArray,
    Typed(u8),
    /// T27.
    RegExp,
    /// T26.
    Date,
    /// T40. Not in [`Builtin::ALL`] (whose order is the runtime's global layout): its prototype and
    /// constructor globals exist only in a module that can make a BigInt.
    BigInt,
    /// T45. Not in [`Builtin::ALL`] either: their globals exist only when [`Module::stacks`].
    /// (T48: nor are `Typed(9)`/`Typed(10)`, the BigInt arrays: only with [`Module::big_typed`].)
    DisposableStack,
    AsyncDisposableStack,
}

/// The typed array constructors (T20), by kind; [`TYPED_SIZES`]: their element sizes. T48: kinds
/// 9 and 10, the BigInt arrays, exist only in a module with [`Module::big_typed`] ([`TYPED_KINDS`]
/// kinds otherwise).
pub const TYPED_NAMES: [&str; 11] = [
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
];
pub const TYPED_SIZES: [u32; 11] = [1, 1, 1, 2, 2, 4, 4, 4, 8, 8, 8];
/// The typed array kinds without the BigInt ones (T48).
pub const TYPED_KINDS: u8 = 9;
/// The first BigInt kind (`BigInt64Array`; `BigUint64Array` is the next).
pub const TYPED_BIGINT64: u8 = 9;

impl Builtin {
    /// All of them; a builtin's index here is its prototype's runtime slot.
    pub const ALL: [Builtin; 38] = [
        Builtin::Object,
        Builtin::String,
        Builtin::Number,
        Builtin::Boolean,
        Builtin::Error(ErrorKind::Error),
        Builtin::Error(ErrorKind::TypeError),
        Builtin::Error(ErrorKind::RangeError),
        Builtin::Error(ErrorKind::ReferenceError),
        Builtin::Error(ErrorKind::SyntaxError),
        Builtin::Error(ErrorKind::EvalError),
        Builtin::Error(ErrorKind::UriError),
        Builtin::Function,
        Builtin::Array,
        Builtin::Symbol,
        Builtin::ArrayIterator,
        Builtin::Promise,
        Builtin::Generator,
        Builtin::Map,
        Builtin::Set,
        Builtin::MapIterator,
        Builtin::SetIterator,
        Builtin::Error(ErrorKind::AggregateError),
        Builtin::Timeout,
        Builtin::Immediate,
        Builtin::AsyncGenerator,
        Builtin::ArrayBuffer,
        Builtin::TypedArray,
        Builtin::Typed(0),
        Builtin::Typed(1),
        Builtin::Typed(2),
        Builtin::Typed(3),
        Builtin::Typed(4),
        Builtin::Typed(5),
        Builtin::Typed(6),
        Builtin::Typed(7),
        Builtin::Typed(8),
        Builtin::RegExp,
        Builtin::Date,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Builtin::Object => "Object",
            Builtin::String => "String",
            Builtin::Number => "Number",
            Builtin::Boolean => "Boolean",
            Builtin::Error(k) => k.name(),
            Builtin::Function => "Function",
            Builtin::Array => "Array",
            Builtin::Symbol => "Symbol",
            Builtin::ArrayIterator => "%ArrayIteratorPrototype%",
            Builtin::Promise => "Promise",
            Builtin::Generator => "%GeneratorPrototype%",
            Builtin::Map => "Map",
            Builtin::Set => "Set",
            Builtin::MapIterator => "%MapIteratorPrototype%",
            Builtin::SetIterator => "%SetIteratorPrototype%",
            Builtin::Timeout => "%Timeout%",
            Builtin::Immediate => "%Immediate%",
            Builtin::AsyncGenerator => "%AsyncGeneratorPrototype%",
            Builtin::ArrayBuffer => "ArrayBuffer",
            Builtin::TypedArray => "%TypedArray%",
            Builtin::Typed(k) => TYPED_NAMES[k as usize],
            Builtin::RegExp => "RegExp",
            Builtin::Date => "Date",
            Builtin::BigInt => "BigInt",
            Builtin::DisposableStack => "DisposableStack",
            Builtin::AsyncDisposableStack => "AsyncDisposableStack",
        }
    }

    /// The built-in whose prototype methods the prelude names `$$<name>_prototype_…` (T21).
    pub fn parse_prelude(s: &str) -> Option<Builtin> {
        match s {
            "MapIterator" => Some(Builtin::MapIterator),
            "TypedArray" => Some(Builtin::TypedArray),
            "SetIterator" => Some(Builtin::SetIterator),
            _ => Builtin::parse(s),
        }
    }

    pub fn parse(s: &str) -> Option<Builtin> {
        if s == "BigInt" {
            return Some(Builtin::BigInt);
        }
        if s == "SuppressedError" {
            return Some(Builtin::Error(ErrorKind::SuppressedError));
        }
        if s == "DisposableStack" {
            return Some(Builtin::DisposableStack);
        }
        if s == "AsyncDisposableStack" {
            return Some(Builtin::AsyncDisposableStack);
        }
        if s == "BigInt64Array" || s == "BigUint64Array" {
            return Some(Builtin::Typed(if s == "BigInt64Array" { 9 } else { 10 }));
        }
        Builtin::ALL.into_iter().find(|b| b.name() == s)
    }

    /// The `length` of the built-in constructors that can be values (T25), `None` for the others.
    pub fn ctor_length(self) -> Option<u32> {
        match self {
            Builtin::Object
            | Builtin::String
            | Builtin::Number
            | Builtin::Boolean
            | Builtin::Array => Some(1),
            Builtin::Map | Builtin::Set => Some(0),
            Builtin::ArrayBuffer => Some(1),
            Builtin::Typed(_) => Some(3),
            // T121: `%TypedArray%`.
            Builtin::TypedArray => Some(0),
            Builtin::RegExp => Some(2),
            Builtin::Date => Some(7),
            Builtin::BigInt => Some(1),
            Builtin::DisposableStack | Builtin::AsyncDisposableStack => Some(0),
            _ => None,
        }
    }

    pub fn index(self) -> u32 {
        Builtin::ALL.iter().position(|b| *b == self).unwrap() as u32
    }
}

/// What a private name denotes (T16, T31).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivateKind {
    /// A field: writable.
    Field,
    /// A method: read-only.
    Method,
    /// A getter and/or setter.
    Accessor,
}

/// A runtime operation on JS values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Intrinsic {
    /// JS `a + b` on two JS values (concatenation if either is a string).
    JsAdd,
    /// Concatenation of two JS strings.
    Concat,
    /// JS ToString of a JS value (a string).
    ToString,
    /// JS Number::toString of an `f64` (a string).
    NumberToString,
    /// T76: QuoteJSONString of a JS value known to be a string (a string).
    JsonQuote,
    /// T76: copy string `src` into string `dst` (made by `StrNew`) at `i` (args: dst, i: `i32`, src).
    StrCopy,
    /// T78: the value of a variable that may hold a string buffer (`$strbuf`, made by
    /// `StrBufAppend`): a buffer's string (made once per append run), any other value as it is.
    StrBufGet,
    /// T78: (v, s) -> the buffer holding `v`'s string followed by string `s`: `v` itself if it is
    /// a buffer (appended in place, amortized O(1)), else a new buffer with room to grow. `v` is a
    /// string or a buffer; anything else gives `v + s` (a string).
    StrBufAppend,
    /// T78: the length (`f64`) of what `StrBufGet` gives, a string, without making it.
    StrBufLen,
    /// T78: (alts, classes, s, from: `i32`, flags: `i32` (1 `i`, 2 `y`), ascii, wide, caps) -> `i32`:
    /// regexp.js's `$$reAlts` search natively: the end of the first match of a fixed alternation in
    /// string `s` from `from` (only at `from` with `y`), with its start and end in `caps` (`$i32s`);
    /// -1 if none. `ascii`/`wide` (`$i32s`, or `null`): the first-unit filter of `$$reFirstSet`.
    ReAltsFind,
    /// T114: (prog, s, from: `i32`, mode: `i32`, caps) -> `i32`: runs the regex program `prog` (a string of
    /// [`Module::regexes`]) on string `s`: mode 0 searches from `from`, mode 1 tries `from` only; the end of the
    /// match, its captures in `caps` (`$i32s`, as regexp.js's `$$reRun` leaves them), or -1.
    ReExec,
    /// T114: the program [`Module::regexes`]`[k]` as a string constant, `null` with [`Module::regex_vm`]: the
    /// third argument of `$$RegExpCreate`.
    RegexId(u32),
    /// T132: is `o[key]` (args: o, key: a string) the function whose code is `code`, by method-call site `site`'s
    /// cache (refilled from `o`'s prototype chain on a miss)? `Bool`.
    MethodCheck { site: u32, code: FuncId },
    /// T132: the function in method-call site `k`'s cache (no args).
    MethodFn(u32),
    /// T132: has method-call site `k` given up (its receivers don't share a keys array)? Inline, no call. `Bool`.
    MethodDead(u32),
    /// T76: write QuoteJSONString of string `s` into string `buf` at `i` (args: buf, i: `i32`, s);
    /// the index after it (`i32`). `buf` must have room for `6 * len + 2` units.
    JsonQuoteInto,
    /// T76: (s, i, mode: `i32`s) -> `i32`: from `i`, the first index of string `s` that isn't JSON
    /// whitespace (mode 0), or that is `"`, `\\` or a control character (mode 1); the length if none.
    JsonScan,
    /// `s.length` of a JS value known to be a string, as an `f64`. Never throws.
    StringLength,
    /// `typeof v` (a string).
    TypeofString,
    /// `console.log` formatting of one primitive: ToString, except `-0` gives `"-0"` (a string).
    Inspect,
    /// Returns its argument, and traps if it's a string containing `%`: the first of several
    /// `console.log` arguments is a format string, which isn't implemented yet (F-item in T10).
    NoFormat,
    /// Append a JS string to stdout (`stderr: false`) or stderr. No result. Commands only.
    ConsoleWrite {
        stderr: bool,
    },
    /// `new E(message, options)` / `E(…)`: a new error object; an own `message` unless the
    /// message is `undefined`, an own `cause` if `options` is an object with one.
    /// `AggregateError` takes `(errors, message, options)` and adds `errors` (an array).
    NewError(ErrorKind),
    /// `{}`: a new plain object (prototype `Object.prototype`).
    NewObject,
    /// `(n: i32) -> jsval` (T109): [`Intrinsic::NewObject`] with room for `n` properties (an object literal's
    /// static property count, as JSC's `StaticPropertyAnalyzer`).
    NewObjectSized,
    /// `v[key]`: property get (a TypeError on `undefined` / `null`).
    GetProp,
    /// `v[key] = value` (no result; TypeErrors as in strict mode).
    SetProp,
    /// `key: value` in an object literal: define an own property of a new object (no result).
    DefineProp,
    /// `delete v[key]` (`bool`).
    DeleteProp,
    /// `key in v` (`bool`; a TypeError if `v` isn't an object).
    HasProp,
    /// `__proto__: p` in an object literal: set the prototype if `p` is an object or `null`.
    SetProto,
    /// `...src` in an object literal: copy `src`'s own enumerable properties into the object.
    CopyProps,
    /// The keys `for (k in v)` visits, as an internal list (a `jsval` that never reaches JS).
    ForInKeys,
    /// Length of a key list (`i32`).
    KeysLen,
    /// Key `i` (`i32`) of a key list (a string).
    KeysGet,
    /// `Object.create(p)`.
    ObjectCreate,
    /// `Object.getPrototypeOf(v)`.
    GetPrototypeOf,
    /// `Object.getOwnPropertySymbols(v)` (T25).
    GetOwnPropertySymbols,
    /// `(v) -> jsval`: an object's `Symbol.toStringTag` (T25): a string, or `undefined`.
    BuiltinTag,
    /// `(v) -> jsval`: ToObject (T30): primitives wrapped, TypeError for `undefined`/`null`.
    ToObject,
    /// `(data) -> jsval`: a new RegExp object holding `data` (T27, see `builtins/regexp.js`).
    RegExpNew,
    /// `(v) -> jsval`: a RegExp's data; `null` for `RegExp.prototype`, `undefined` otherwise (T27).
    RegExpData,
    /// `(f) -> bool`: is `f` the built-in `RegExp.prototype.exec` (T27)?
    RegExpBuiltinExec,
    /// `(r) -> bool` (T76): is `r` a RegExp with only its own `lastIndex` (writable, a number), whose
    /// prototype is `RegExp.prototype` with `exec`, `flags` and the flag getters as built?
    RegExpPlain,
    /// `(digits) -> jsval` (T40): a BigInt constant; the argument is an [`Expr::Str`] of its
    /// decimal digits (a literal's value), made once as a global.
    BigIntConst,
    /// `(v) -> jsval` (T40): ToNumeric: a number or a BigInt (objects through ToPrimitive, hint
    /// number; anything else ToNumber).
    ToNumeric,
    /// A numeric operator on JS values that may be BigInts (T40, only in a module that can make
    /// one): ToNumeric of the operands, then the BigInt operation (a TypeError when a BigInt
    /// meets a number) or the number one. `(a, b) -> jsval` for a binary operator, `(v) -> jsval`
    /// otherwise.
    Numeric(NumericOp),
    /// `(x: f64) -> jsval` (T40): the BigInt of an integral number (the prelude checks).
    BigIntFromNumber,
    /// `(v) -> f64` (T40): a BigInt as a number (correctly rounded).
    BigIntToNumber,
    /// `(v, radix: i32) -> jsval` (T40): a BigInt's digits in `radix` (2–36), a string.
    BigIntToString,
    /// `(data) -> jsval`: a new Date object holding `data` (T26, see `builtins/date.js`).
    DateNew,
    /// `(v) -> jsval`: a Date's data; `undefined` for anything else (T26).
    DateData,
    /// `(data) -> jsval`: a new `DisposableStack` (`false`) or `AsyncDisposableStack` holding `data`
    /// (T45, see `builtins/dispose.js`).
    StackNew(bool),
    /// `(v) -> jsval`: the data of a plain object with private state (a stack's); `undefined` for
    /// anything else (T45).
    StackData,
    /// `() -> jsval`: a new module namespace object (T41, see `builtins/module.js`): empty, with a
    /// null prototype.
    ModuleNew,
    /// `() -> f64`: the system clock in ms since the epoch (T26, `wasi:clocks/system-clock`, commands
    /// only).
    DateNow,
    /// `() -> jsval`: a string from `wasi:cli/environment` (T55, `process`; commands only): 0 the
    /// variables (`name\0value\0…`), 1 the arguments (`a\0b\0…`), 2 the initial directory ("" if
    /// none).
    Environment(u8),
    /// `() -> f64`: `wasi:clocks/monotonic-clock.now` in ns (T55: `performance.now`, `hrtime`).
    MonotonicNow,
    /// `(code: f64)`: `process.exit` (T55): end the program now (no more JS runs: the exit tag is
    /// caught only by `run` and the event loop), flush the output, and fail unless `code` is 0.
    ProcessExit,
    /// `(v, hint) -> jsval`: ToPrimitive (T26); hint 0 default, 1 number, 2 string.
    ToPrimitive,
    /// `(v) -> jsval`: the primitive of a wrapper object, anything else unchanged (T30).
    Unwrap,
    /// `(v) -> jsval`: `util.inspect(v)` from depth `d` (T25, `console.log` format strings;
    /// strings quoted).
    InspectAt(u8),
    /// `(v, depth: i32) -> jsval`: `util.inspect(v)` from a depth known at run time (T55:
    /// `util.inspect(v, { depth })`: node's depth `n` starts at `2 - n`).
    InspectDepth,
    /// `(v) -> bool`: node's `hasBuiltInToString(o)` (T25): `o.toString` is missing or comes from
    /// a built-in prototype.
    /// IsConstructor(v) (`bool`, T35): a function with a construct entry.
    IsConstructor,
    BuiltinToString,
    /// `Object.hasOwn(v, key)` (`bool`).
    HasOwn,
    /// `v instanceof B` (`bool`).
    InstanceOf(Builtin),
    /// `B.prototype`.
    Prototype(Builtin),
    /// A new cell holding its argument (of the given type, `F64` or `JsVal`).
    NewCell(Type),
    /// A new `JsVal` cell that isn't initialized yet (a `let` in its TDZ).
    NewCellUninit,
    /// The value in a cell of the given type.
    CellGet(Type),
    /// Store into a cell of the given type (no result).
    CellSet(Type),
    /// Is the (`JsVal`) cell still uninitialized? (`bool`)
    CellIsUninit,
    /// Entry `i` of a function object's environment.
    EnvGet(u32),
    /// Argument `i` of an argument array (`undefined` past its end).
    ArgGet(u32),
    /// `arr.push(v)` on a new array (literals). No result.
    ArrayPush,
    /// `...v` in an array literal: append what iterating `v` gives (args: array, v, the
    /// TypeError message if `v` isn't iterable). No result.
    ArraySpread,
    /// The arguments from index `i` on of an argument array, as a new array (rest parameters).
    ArgsRest(u32),
    /// The elements from index `i` on of an array, as a new array (T63: a rest parameter beside
    /// `arguments`, from the array of all arguments).
    ArrayRest(u32),
    /// Start iterating `v` (args: v, the TypeError message if it isn't iterable): an internal
    /// iteration state (a `jsval` that never reaches JS).
    IterOpen,
    /// Advance an iteration: `false` when it's done.
    IterStep,
    /// The value the last `IterStep` produced.
    IterValue,
    /// Close an iteration that isn't done (the iterator's `return`); args: state, `throwing`
    /// (`bool`: errors from `return` are ignored). No result.
    IterClose,
    /// The rest of an iteration, as a new array.
    IterRest,
    /// `for await` (T20): start iterating `v` (args: v, the TypeError message if it isn't
    /// iterable): its `Symbol.asyncIterator` iterator, else the `IterOpen` state of its sync one.
    AsyncIterOpen,
    /// Is the state from `AsyncIterOpen` an async iterator (else a sync `IterOpen` state)?
    IsAsyncIter,
    /// The async iterator's `next()` result (to await).
    AsyncIterNext,
    /// Check an awaited iterator result is an object (TypeError); returns it.
    AsyncIterResult,
    /// Closing an async iterator: args: state, `throwing` (`bool`: errors are ignored, a Wasm
    /// null result). Its `return()` result to await, or a hole when it has no `return`.
    AsyncIterReturn,
    /// Does a sync `IterOpen` state have a `return` method (it isn't done)?
    IterHasReturn,
    /// `f.apply(this, args)` without the lookup: args: callee, this, array-like, what (the
    /// TypeError text if it isn't a function).
    CallList,
    /// `{...rest}` in a pattern: a new object with `src`'s own enumerable properties except the
    /// keys in the array `excluded`.
    ObjectRest,
    /// Destructuring `v` (args: v, message prefix): a TypeError "<prefix>undefined." / "…null."
    /// if it's `undefined` / `null`. No result.
    CheckDestructure,
    /// `Symbol(description)`.
    NewSymbol,
    /// `Symbol.iterator`.
    SymbolIterator,
    /// `() -> jsval`: another well-known symbol, `Symbol.<WELL_KNOWN_SYMBOLS[k]>` (T25).
    WellKnownSymbol(u8),
    /// `Array.isArray(v)` (`bool`).
    IsArray,
    /// IsPromise(v) (`bool`, T50: the prelude's `__intrinsic_is_promise`).
    IsPromise,
    /// `Object.keys(v)`.
    ObjectKeys,
    /// `Object.getOwnPropertyNames(v)` (T28).
    GetOwnPropertyNames,
    /// `Object.values(v)`.
    ObjectValues,
    /// `Object.entries(v)`.
    ObjectEntries,
    /// `f(...args)` for `new` (T16): args: callee, array-like, new target, what (the TypeError
    /// text if it isn't a constructor).
    ConstructList,
    /// A new object whose prototype is `new_target.prototype` (`Object.prototype` if that isn't an
    /// object): a base class constructor's `this`.
    CreateThis,
    /// `(new_target, n: i32) -> jsval` (T109): [`Intrinsic::CreateThis`] with room for `n` properties (the
    /// constructor's `this.k = …` count).
    CreateThisSized,
    /// `(new_target) -> jsval` (T109): [`Intrinsic::CreateThis`] with room for the keys of
    /// [`Module::layouts`]`[l]`, marked as having that layout (its shared hash index).
    CreateThisLayout(u32),
    /// `(o, key: string constant, value)` (T109): `o.key = value` in the constructor of layout `layout`,
    /// where `key` is the layout's key `slot`: a store at that entry when `o` still has the layout,
    /// else `SetProp`.
    LayoutSet { layout: u32, slot: u32 },
    /// `() -> jsval` (T109): `{}` with room for the keys of [`Module::layouts`]`[l]` and that layout (an object
    /// literal's keys).
    NewObjectLayout(u32),
    /// `(o, key: string constant, value)` (T109): an object literal's `key: value` (`DefineProp`), `key` the
    /// layout's key `slot`.
    LayoutDefine { layout: u32, slot: u32 },
    /// Check a class heritage value (node's TypeErrors): the class prototype's parent.
    ClassHeritage,
    /// args: class, heritage (`undefined`: none), prototype parent (from `ClassHeritage`, or
    /// `Object.prototype`): the class's new `prototype` object; the class inherits from the heritage.
    ClassSetup,
    /// `v instanceof f` for any value `f` (`bool`).
    InstanceOfValue,
    /// A method: a non-enumerable data property (args: object, key, value). No result.
    DefineMethod,
    /// A field: an enumerable data property replacing any existing one (args: object, key,
    /// value). No result.
    DefineField,
    /// A getter (`setter: false`) or setter of an object literal (`enumerable`) or a class. Args:
    /// object, key, function. No result.
    DefineAccessor {
        setter: bool,
        enumerable: bool,
    },
    /// `super[key]`: args: home object, key, `this`.
    SuperGet,
    /// `super[key] = v` (no result): args: home object, key, value, `this`.
    SuperSet,
    /// The uninitialized marker (a Wasm null): a derived constructor's `this` before `super()`.
    Hole,
    /// Is the value the uninitialized marker? (`bool`)
    IsHole,
    /// What a constructor returning `value` gives (args: value, `this`).
    CtorReturn {
        derived: bool,
    },
    /// `Object.defineProperty(o, key, desc)`.
    DefineProperty,
    /// `Object.getOwnPropertyDescriptor(o, key)`.
    GetOwnPropertyDescriptor,
    /// `Object.preventExtensions` (0), `seal` (1), `freeze` (2).
    Integrity(u8),
    /// `Object.isExtensible` (0), `isSealed` (1), `isFrozen` (2) (`bool`).
    TestIntegrity(u8),
    /// A native error constructor as a value (`class E extends Error`).
    ErrorCtor(ErrorKind),
    /// The `Promise` constructor as a value (T17).
    PromiseCtor,
    /// The `queueMicrotask` function as a value (T17).
    QueueMicrotask,
    /// A timer function as a value (T19, commands only): [`TIMER_SET_TIMEOUT`] …
    Timer(u8),
    /// `o.#x`: args: object, private symbol, the TypeError message if `o` doesn't have it.
    PrivateGet,
    /// `o.#x = v` (no result): args: object, private symbol, value, the TypeError message if `o`
    /// doesn't have it.
    PrivateSet,
    /// `#x in o` (`bool`): args: private symbol, object.
    PrivateHas,
    /// Add a private member to a new object (no result): args: object, private symbol, value (an
    /// accessor: a [`Intrinsic::NewAccessor`]), the TypeError message if it's already there.
    PrivateAdd {
        kind: PrivateKind,
    },
    /// ToPropertyKey (a string or a symbol): a computed class field key, converted once when the
    /// class is evaluated (T31).
    ToPropertyKey,
    /// `(o) -> jsval`: an object as node shows it in TypeErrors (`#<Foo>`, `[object Array]`; T36).
    ObjectDesc,
    /// A getter/setter pair (args: getter, setter, each a function or `undefined`) as a value for
    /// [`Intrinsic::PrivateAdd`] with [`PrivateKind::Accessor`].
    NewAccessor,
    /// How a generator was resumed after the `Suspend` just before (`i32`): [`RESUME_NEXT`],
    /// [`RESUME_THROW`] or [`RESUME_RETURN`]. Only in generators, before the suspension transform.
    ResumeMode,
    /// From the suspension transform (T18) on: a new generator object (prototype
    /// `%GeneratorPrototype%`) whose frame has `slots` slots and which runs `resume`, a function
    /// `(gen, mode: i32, value) -> jsval` (see [`suspend`]).
    NewGenerator {
        resume: FuncId,
        slots: u32,
    },
    /// From the suspension transform on (T19): the frame of an `async function` call, a generator
    /// object that never reaches JS, with `slots` slots for the locals plus one more for the
    /// promise the call returns (a new pending one).
    NewAsync {
        resume: FuncId,
        slots: u32,
    },
    /// From the suspension transform on (T20): an async generator object; its frame has one more
    /// slot, the queue of `next`/`throw`/`return` requests.
    NewAsyncGenerator {
        resume: FuncId,
        slots: u32,
    },
    /// Run an async function's frame (args: the frame) up to its first `await` or its end, and
    /// return its promise. Later steps run from promise jobs; the body's result resolves the
    /// promise and an exception rejects it.
    AsyncStart,
    /// Slot `i` of a generator's frame (args: the generator).
    GenLoad(u32),
    /// Store into slot `i` of a generator's frame (args: the generator, the value). No result.
    GenStore(u32),
    /// The point a generator is suspended at (0: its start) (`i32`; args: the generator).
    GenPoint,
    /// Record that the generator suspends at a point (args: the generator, the point, an `i32`);
    /// the resume function then returns the value to yield (`raw`: an iterator result object
    /// passed out as is, `yield*`; `awaiting`: an `await` in an async generator, T20). No result.
    GenSuspend {
        raw: bool,
        awaiting: bool,
    },
    /// `yield* v`: start iterating `v` (a TypeError like node's if it isn't iterable): an internal
    /// iteration state, as `IterOpen` gives.
    DelegateOpen,
    /// One step of `yield*` (args: state, mode (`i32`, as `ResumeMode`), received value): the
    /// inner iterator's result object of `next(received)` / `throw(received)` / `return(received)`
    /// (TypeErrors as in the spec), or the uninitialized marker (`Hole`) for a `return` when the
    /// inner iterator has no `return` method.
    DelegateStep,
    /// The high (`F64Hi`) or low 32 bits of an `f64` (`i32`, T21).
    F64Hi,
    F64Lo,
    /// The `f64` with these high and low 32 bits.
    F64FromWords,
    /// `(o, k: jsval) -> f64` (T107): element `k` of an array in number mode, unboxed; for anything else
    /// (another value, a hole, a generic array) the NaN with the hole's bits ([`Intrinsic::F64IsHole`]).
    /// Never runs JS code.
    ElemNum,
    /// `(x: f64) -> bool` (T107): `x` has the bits [`Intrinsic::ElemNum`] returns for "not an element in
    /// number mode" (no JS number has them).
    F64IsHole,
    /// `(v: jsval) -> f64` (T108): `v`'s value if it's a number (an i31 or a `$number`), else the bits of
    /// [`Intrinsic::F64IsHole`]. Never runs JS code.
    NumOrHole,
    /// 32 random bits from the host (`wasi:random/insecure`, commands only) (`i32`).
    RandomSeed,
    /// Call [`Module::imports`]`[i]` (T20 slice 5) with the arguments in an array (`jsval`):
    /// each converted to its WIT type like an export's result, the WIT result given back as a JS
    /// value (a top-level `result` as `{ tag, val }`: the caller unwraps it). Converting the
    /// arguments can run user code (`valueOf`, `toString`). An `async func` (slice 5b) gives a
    /// promise instead, settled when the subtask returns (through [`Import::unwrap`]).
    CallImport(u32),
    /// `(promise)` (T67): cancel the pending `async func` import call whose promise this is, if
    /// any: a call that already returned settles now, a cancelled one's promise never settles.
    /// Only a command waiting for non-blocking imports has such calls; elsewhere a no-op.
    ImportCancel,
    /// Code unit `i` (`i32`, in range) of a JS value known to be a string (`i32`, T21).
    StrUnit,
    /// The code units `[start, end)` (`i32`s, in range) of a string, as a new string.
    StrSub,
    /// A new string of `n` (`i32`) zero code units, to be filled with `StrSet` before it escapes.
    StrNew,
    /// Set code unit `i` of a string made by `StrNew` (args: string, i, unit: `i32`s). No result.
    StrSet,
    /// A new `$i32s` array of `n` (`i32`) zeros (T27): prelude-internal storage (the RegExp
    /// engine's program and backtrack stack), never given to user code.
    I32New,
    /// Element `i` (`i32`, in range) of an `$i32s` array (T27).
    I32Get,
    /// Set element `i` of an `$i32s` array (args: array, i, value: `i32`s). No result (T27).
    I32Set,
    /// The length of an `$i32s` array (`i32`, T27).
    I32Len,
    /// JS `a < b` on two JS values (`bool`, IsLessThan with LeftFirst): ToPrimitive (hint number)
    /// of both, then strings compare by code units, anything else as numbers (NaN: false).
    /// `swap`: compare `b < a` (for `>`/`<=`), still converting `a` first. `or_equal`: `<=`/`>=`
    /// (not `b < a`, NaN still false).
    JsLess {
        swap: bool,
        or_equal: bool,
    },
    /// A Wasm trap: a built-in case the prelude doesn't implement yet. No result.
    Trap,
    /// `Map`/`Set` storage for the prelude (T25).
    Map(MapOp),
    /// Typed arrays and `ArrayBuffer`s (T20), for the prelude.
    Typed(TypedOp),
    /// A boundary stream's ends (T20 slice 6b), for the prelude's reader and pump.
    Stream(StreamOp),
    /// `(args: an array of every argument) -> jsval`: a function's `arguments` object (T25,
    /// unmapped: strict mode).
    MakeArguments,
    /// `(f, key, prefix: a string) -> f`: SetFunctionName at run time (F59, a computed key): `name`
    /// becomes `prefix` + the key (a symbol: `[description]`).
    SetFunctionName,
    /// `() -> jsval`: a built-in constructor as a value (T25; see [`Module::builtin_ctors`]).
    BuiltinCtor(Builtin),
    /// `() -> jsval`: `Math` or `JSON` as a value (T25; see [`Module::namespace_methods`]).
    Namespace(Namespace),
}

/// [`Intrinsic::Map`] operations. `m` is a `$map` and `it` a `$mapiter` (the prelude checks
/// them with `Kind`/`IterKind` first: anything else traps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapOp {
    /// `() -> jsval`: a new empty map ([`MAP_KIND_MAP`] …), prototype `Map.prototype` / ….
    New(u8),
    /// `(v) -> i32`: the kind of a `$map`, or -1.
    Kind,
    /// `(m, key) -> jsval`: the value, or `undefined`.
    Get,
    /// `(m, key) -> bool`.
    Has,
    /// `(m, key, value)`: add or update (a `-0` key is stored as `+0`).
    Set,
    /// `(m, key) -> bool`: was it there?
    Delete,
    /// `(m)`.
    Clear,
    /// `(m) -> i32`: the number of entries.
    Size,
    /// `(m) -> jsval`: a new iterator ([`MAP_ITER_KEYS`] …), prototype `%MapIteratorPrototype%`
    /// or `%SetIteratorPrototype%`.
    Iter(u8),
    /// `(v) -> i32`: `mode + 3 * map kind` of a `$mapiter`, or -1.
    IterKind,
    /// `(it) -> i32`: advance to the next entry and give its index, or -1 at the end (for good).
    Step,
    /// `(it, index) -> jsval`: the key / value of the entry `Step` gave.
    Key,
    Value,
}

/// [`Intrinsic::Typed`] operations (T20). `t` is a typed array, `b` an `ArrayBuffer` (the
/// prelude checks them with `Kind`/`IsBuffer` first: anything else traps); `kind` indexes
/// [`TYPED_NAMES`]; lengths, offsets and indices are `i32`s in range (the prelude checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StreamOp {
    /// `(h, id) -> jsval`: read from the readable end `h` of stream type `id` (the backend's
    /// payload id): a promise of `[chunk, ended]` (`chunk`: shaped like `list<T>`, `undefined`
    /// if nothing was read; `ended`: the writer dropped its end, don't read again).
    Read,
    /// `(chunk, w, id) -> jsval`: write `chunk` (converted like a `list<T>`) to the writable end
    /// `w`: a promise of `true` once it's all written, `false` if the reader dropped its end.
    Write,
    /// `(h, id)`: drop the readable end.
    DropReadable,
    /// `(w, id)`: drop the writable end (the stream ends); the call stops waiting for it.
    DropWritable,
    /// `(h, id) -> jsval`: [`StreamOp::Read`] blocking until the read ends (T59): `[chunk, ended]`.
    ReadSync,
    /// `(chunk, w, id) -> jsval`: [`StreamOp::Write`] blocking until it's all written (T59):
    /// `true`, or `false` if the reader dropped its end.
    WriteSync,
    /// `(h, id)`: cancel the read in progress on the readable end `h`, if any (T66): its promise
    /// settles now. Only a command waiting for non-blocking imports cancels; elsewhere a no-op.
    CancelRead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypedOp {
    /// `(kind, length) -> jsval`: a new zeroed typed array on a new buffer.
    New,
    /// `(kind, b, byte_offset, length) -> jsval`: a typed array on `b`.
    View,
    /// `(v) -> i32`: the kind of a typed array, or -1.
    Kind,
    /// `(t) -> i32`: its length / byte offset.
    Length,
    ByteOffset,
    /// `(t) -> jsval`: its buffer.
    Buffer,
    /// `(v) -> bool`: is it an `ArrayBuffer`?
    IsBuffer,
    /// `(byte_length) -> jsval`: a new zeroed `ArrayBuffer`.
    BufferNew,
    /// `(b) -> i32`.
    BufferLength,
    /// `(b, start, end) -> jsval`: a new `ArrayBuffer` with a copy of bytes `start..end`.
    BufferSlice,
    /// `(dst, dst_index, src, src_index, count)`: copy elements between typed arrays of the same
    /// kind (overlap allowed).
    Copy,
    // T47, only in a module with [`Module::rab`] (the backend's `$buffer`/`$typed` have the
    // fields they use only then).
    /// `(kind, b, byte_offset) -> jsval`: a length-tracking typed array on `b`.
    ViewTracking,
    /// `(t) -> bool`: is it length-tracking?
    Tracking,
    /// `(t) -> i32`: IsTypedArrayOutOfBounds (its buffer is detached, or doesn't reach its end).
    OutOfBounds,
    /// `(byte_length, max) -> jsval`: a new zeroed resizable `ArrayBuffer`; `max` is a number.
    BufferNewResizable,
    /// `(b) -> jsval`: its `maxByteLength`, `undefined` if it isn't resizable.
    BufferMax,
    /// `(b) -> bool`: is it detached?
    BufferDetached,
    /// `(b, byte_length)`: resize it in place (new bytes are zero).
    BufferResize,
    /// `(b, byte_length, resizable) -> jsval`: a new buffer with `b`'s bytes (cut or zero-filled to
    /// `byte_length`; resizable with `b`'s max if `resizable` and `b` is), then `b` is detached.
    BufferTransfer,
    /// T78: `(t, start, end, mask) -> jsval`: the string of units `byte & mask` of elements
    /// `start..end` (in range) of a typed array of bytes (`Buffer`'s `latin1`/`ascii` decoding).
    Latin1,
    /// T80: `(s) -> i32`: the UTF-8 length of string `s` (an unpaired surrogate is U+FFFD: 3 bytes).
    Utf8Length,
    /// T80: `(t, s, offset, limit) -> i32`: encode string `s` as UTF-8 into a typed array of bytes
    /// from element `offset`, at most `limit` bytes (in range), never a partial character; the
    /// bytes written. Unpaired surrogates are U+FFFD.
    Utf8Write,
    /// T80: `(t, start, end) -> jsval`: the string of elements `start..end` (in range) of a typed
    /// array of bytes decoded as UTF-8 (WHATWG: U+FFFD per maximal invalid subpart).
    Utf8Slice,
    /// T121: `(c) -> i32`: the kind of typed-array constructor value `c`, or of the first one on its
    /// [[Prototype]] chain (a subclass), else -1 (`%TypedArray%.from`/`of` with `c` as `this`).
    CtorKind,
}

pub const MAP_KIND_MAP: u8 = 0;
pub const MAP_KIND_SET: u8 = 1;
pub const MAP_ITER_KEYS: u8 = 0;
pub const MAP_ITER_VALUES: u8 = 1;
pub const MAP_ITER_ENTRIES: u8 = 2;

/// [`Intrinsic::Timer`] functions.
pub const TIMER_SET_TIMEOUT: u8 = 0;
pub const TIMER_SET_INTERVAL: u8 = 1;
pub const TIMER_CLEAR_TIMEOUT: u8 = 2;
pub const TIMER_CLEAR_INTERVAL: u8 = 3;
/// T25: `setImmediate`, `clearImmediate`, `process.nextTick`.
pub const TIMER_SET_IMMEDIATE: u8 = 4;
pub const TIMER_CLEAR_IMMEDIATE: u8 = 5;
pub const TIMER_NEXT_TICK: u8 = 6;
/// Their JS names, by kind (the globals; `nextTick` is `process.nextTick`).
pub const TIMER_NAMES: [&str; 7] = [
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "clearInterval",
    "setImmediate",
    "clearImmediate",
    "nextTick",
];

/// [`Intrinsic::ResumeMode`] values.
pub const RESUME_NEXT: i32 = 0;
pub const RESUME_THROW: i32 = 1;
pub const RESUME_RETURN: i32 = 2;

impl Intrinsic {
    /// Parameter types and result type (`None`: no result).
    pub fn signature(self) -> (&'static [Type], Option<Type>) {
        use Type::*;
        match self {
            Intrinsic::JsAdd | Intrinsic::Concat => (&[JsVal, JsVal], Some(JsVal)),
            Intrinsic::ToString
            | Intrinsic::TypeofString
            | Intrinsic::Inspect
            | Intrinsic::NoFormat
            | Intrinsic::ForInKeys
            | Intrinsic::ObjectCreate
            | Intrinsic::GetPrototypeOf
            | Intrinsic::GetOwnPropertySymbols
            | Intrinsic::BuiltinTag
            | Intrinsic::ToObject
            | Intrinsic::RegExpNew
            | Intrinsic::RegExpData
            | Intrinsic::DateNew
            | Intrinsic::DateData
            | Intrinsic::StackNew(_)
            | Intrinsic::StackData
            | Intrinsic::Unwrap
            | Intrinsic::InspectAt(_) => (&[JsVal], Some(JsVal)),
            Intrinsic::InspectDepth => (&[JsVal, I32], Some(JsVal)),
            Intrinsic::BuiltinToString
            | Intrinsic::IsConstructor
            | Intrinsic::RegExpBuiltinExec
            | Intrinsic::RegExpPlain => (&[JsVal], Some(Bool)),
            Intrinsic::BigIntConst | Intrinsic::ToNumeric => (&[JsVal], Some(JsVal)),
            Intrinsic::Numeric(NumericOp::Binary(_)) => (&[JsVal, JsVal], Some(JsVal)),
            Intrinsic::Numeric(_) => (&[JsVal], Some(JsVal)),
            Intrinsic::BigIntFromNumber => (&[F64], Some(JsVal)),
            Intrinsic::BigIntToNumber => (&[JsVal], Some(F64)),
            Intrinsic::BigIntToString => (&[JsVal, I32], Some(JsVal)),
            Intrinsic::NewError(ErrorKind::AggregateError | ErrorKind::SuppressedError) => {
                (&[JsVal, JsVal, JsVal], Some(JsVal))
            }
            Intrinsic::NewError(_) | Intrinsic::GetProp => (&[JsVal, JsVal], Some(JsVal)),
            Intrinsic::NewObject | Intrinsic::Prototype(_) => (&[], Some(JsVal)),
            Intrinsic::NewObjectSized => (&[I32], Some(JsVal)),
            Intrinsic::CreateThisSized => (&[JsVal, I32], Some(JsVal)),
            Intrinsic::CreateThisLayout(_) => (&[JsVal], Some(JsVal)),
            Intrinsic::LayoutSet { .. } | Intrinsic::LayoutDefine { .. } => (&[JsVal, JsVal, JsVal], None),
            Intrinsic::NewObjectLayout(_) => (&[], Some(JsVal)),
            Intrinsic::SetProp | Intrinsic::DefineProp => (&[JsVal, JsVal, JsVal], None),
            Intrinsic::DeleteProp | Intrinsic::HasProp | Intrinsic::HasOwn => {
                (&[JsVal, JsVal], Some(Bool))
            }
            Intrinsic::SetProto | Intrinsic::CopyProps => (&[JsVal, JsVal], None),
            Intrinsic::KeysLen => (&[JsVal], Some(I32)),
            Intrinsic::KeysGet => (&[JsVal, I32], Some(JsVal)),
            Intrinsic::InstanceOf(_) => (&[JsVal], Some(Bool)),
            Intrinsic::NumberToString => (&[F64], Some(JsVal)),
            Intrinsic::JsonQuote => (&[JsVal], Some(JsVal)),
            Intrinsic::StrCopy => (&[JsVal, I32, JsVal], None),
            Intrinsic::StrBufGet => (&[JsVal], Some(JsVal)),
            Intrinsic::StrBufAppend => (&[JsVal, JsVal], Some(JsVal)),
            Intrinsic::StrBufLen => (&[JsVal], Some(F64)),
            Intrinsic::ReAltsFind => (&[JsVal, JsVal, JsVal, I32, I32, JsVal, JsVal, JsVal], Some(I32)),
            Intrinsic::ReExec => (&[JsVal, JsVal, I32, I32, JsVal], Some(I32)),
            Intrinsic::RegexId(_) => (&[], Some(JsVal)),
            Intrinsic::MethodCheck { .. } => (&[JsVal, JsVal], Some(Bool)),
            Intrinsic::MethodFn(_) => (&[], Some(JsVal)),
            Intrinsic::MethodDead(_) => (&[], Some(Bool)),
            Intrinsic::JsonQuoteInto => (&[JsVal, I32, JsVal], Some(I32)),
            Intrinsic::JsonScan => (&[JsVal, I32, I32], Some(I32)),
            Intrinsic::StringLength => (&[JsVal], Some(F64)),
            Intrinsic::ConsoleWrite { .. } => (&[JsVal], None),
            Intrinsic::NewCell(F64) => (&[F64], Some(JsVal)),
            Intrinsic::NewCell(_) => (&[JsVal], Some(JsVal)),
            Intrinsic::NewCellUninit => (&[], Some(JsVal)),
            Intrinsic::CellGet(t) => (&[JsVal], Some(t)),
            Intrinsic::CellSet(F64) => (&[JsVal, F64], None),
            Intrinsic::CellSet(_) => (&[JsVal, JsVal], None),
            Intrinsic::CellIsUninit => (&[JsVal], Some(Bool)),
            Intrinsic::EnvGet(_) | Intrinsic::ArgGet(_) => (&[JsVal], Some(JsVal)),
            Intrinsic::ArrayPush => (&[JsVal, JsVal], None),
            Intrinsic::ArraySpread => (&[JsVal, JsVal, JsVal], None),
            Intrinsic::ArgsRest(_)
            | Intrinsic::ArrayRest(_)
            | Intrinsic::IterValue
            | Intrinsic::IterRest
            | Intrinsic::NewSymbol
            | Intrinsic::ObjectKeys
            | Intrinsic::ObjectValues
            | Intrinsic::ObjectEntries
            | Intrinsic::GetOwnPropertyNames => (&[JsVal], Some(JsVal)),
            Intrinsic::IterOpen | Intrinsic::ObjectRest => (&[JsVal, JsVal], Some(JsVal)),
            Intrinsic::IterStep | Intrinsic::IsArray | Intrinsic::IsPromise => {
                (&[JsVal], Some(Bool))
            }
            Intrinsic::IterClose => (&[JsVal, Bool], None),
            Intrinsic::AsyncIterOpen => (&[JsVal, JsVal], Some(JsVal)),
            Intrinsic::AsyncIterNext | Intrinsic::AsyncIterResult => (&[JsVal], Some(JsVal)),
            Intrinsic::AsyncIterReturn => (&[JsVal, Bool], Some(JsVal)),
            Intrinsic::IsAsyncIter | Intrinsic::IterHasReturn => (&[JsVal], Some(Bool)),
            Intrinsic::CallList => (&[JsVal, JsVal, JsVal, JsVal], Some(JsVal)),
            Intrinsic::CheckDestructure => (&[JsVal, JsVal], None),
            Intrinsic::SymbolIterator | Intrinsic::WellKnownSymbol(_) => (&[], Some(JsVal)),
            Intrinsic::ConstructList => (&[JsVal, JsVal, JsVal, JsVal], Some(JsVal)),
            Intrinsic::CreateThis | Intrinsic::ClassHeritage | Intrinsic::Integrity(_) => {
                (&[JsVal], Some(JsVal))
            }
            Intrinsic::ClassSetup | Intrinsic::SuperGet | Intrinsic::DefineProperty => {
                (&[JsVal, JsVal, JsVal], Some(JsVal))
            }
            Intrinsic::InstanceOfValue => (&[JsVal, JsVal], Some(Bool)),
            Intrinsic::DefineMethod | Intrinsic::DefineField | Intrinsic::DefineAccessor { .. } => {
                (&[JsVal, JsVal, JsVal], None)
            }
            Intrinsic::Hole
            | Intrinsic::ErrorCtor(_)
            | Intrinsic::PromiseCtor
            | Intrinsic::QueueMicrotask
            | Intrinsic::Timer(_) => (&[], Some(JsVal)),
            Intrinsic::PrivateGet => (&[JsVal, JsVal, JsVal], Some(JsVal)),
            Intrinsic::PrivateSet | Intrinsic::PrivateAdd { .. } | Intrinsic::SuperSet => {
                (&[JsVal, JsVal, JsVal, JsVal], None)
            }
            Intrinsic::PrivateHas => (&[JsVal, JsVal], Some(Bool)),
            Intrinsic::NewAccessor => (&[JsVal, JsVal], Some(JsVal)),
            Intrinsic::ToPropertyKey | Intrinsic::ObjectDesc => (&[JsVal], Some(JsVal)),
            Intrinsic::IsHole | Intrinsic::TestIntegrity(_) => (&[JsVal], Some(Bool)),
            Intrinsic::CtorReturn { .. } | Intrinsic::GetOwnPropertyDescriptor => {
                (&[JsVal, JsVal], Some(JsVal))
            }
            Intrinsic::ResumeMode => (&[], Some(I32)),
            Intrinsic::NewGenerator { .. }
            | Intrinsic::NewAsync { .. }
            | Intrinsic::NewAsyncGenerator { .. } => (&[], Some(JsVal)),
            Intrinsic::AsyncStart => (&[JsVal], Some(JsVal)),
            Intrinsic::GenLoad(_) => (&[JsVal], Some(JsVal)),
            Intrinsic::GenStore(_) => (&[JsVal, JsVal], None),
            Intrinsic::GenPoint => (&[JsVal], Some(I32)),
            Intrinsic::GenSuspend { .. } => (&[JsVal, I32], None),
            Intrinsic::DelegateOpen => (&[JsVal], Some(JsVal)),
            Intrinsic::DelegateStep => (&[JsVal, I32, JsVal], Some(JsVal)),
            Intrinsic::F64Hi | Intrinsic::F64Lo => (&[F64], Some(I32)),
            Intrinsic::F64FromWords => (&[I32, I32], Some(F64)),
            Intrinsic::ElemNum => (&[JsVal, JsVal], Some(F64)),
            Intrinsic::F64IsHole => (&[F64], Some(Bool)),
            Intrinsic::NumOrHole => (&[JsVal], Some(F64)),
            Intrinsic::RandomSeed => (&[], Some(I32)),
            Intrinsic::DateNow => (&[], Some(F64)),
            Intrinsic::Environment(_) => (&[], Some(JsVal)),
            Intrinsic::MonotonicNow => (&[], Some(F64)),
            Intrinsic::ProcessExit => (&[F64], None),
            Intrinsic::ModuleNew => (&[], Some(JsVal)),
            Intrinsic::ToPrimitive => (&[JsVal, I32], Some(JsVal)),
            Intrinsic::CallImport(_) => (&[JsVal], Some(JsVal)),
            Intrinsic::ImportCancel => (&[JsVal], None),
            Intrinsic::StrUnit => (&[JsVal, I32], Some(I32)),
            Intrinsic::StrSub => (&[JsVal, I32, I32], Some(JsVal)),
            Intrinsic::StrNew => (&[I32], Some(JsVal)),
            Intrinsic::StrSet => (&[JsVal, I32, I32], None),
            Intrinsic::I32New => (&[I32], Some(JsVal)),
            Intrinsic::I32Get => (&[JsVal, I32], Some(I32)),
            Intrinsic::I32Set => (&[JsVal, I32, I32], None),
            Intrinsic::I32Len => (&[JsVal], Some(I32)),
            Intrinsic::JsLess { .. } => (&[JsVal, JsVal], Some(Bool)),
            Intrinsic::Trap => (&[], None),
            Intrinsic::MakeArguments => (&[JsVal], Some(JsVal)),
            Intrinsic::SetFunctionName => (&[JsVal, JsVal, JsVal], Some(JsVal)),
            Intrinsic::BuiltinCtor(_) | Intrinsic::Namespace(_) => (&[], Some(JsVal)),
            Intrinsic::Stream(op) => match op {
                StreamOp::Read | StreamOp::ReadSync => (&[I32, I32], Some(JsVal)),
                StreamOp::Write | StreamOp::WriteSync => (&[JsVal, I32, I32], Some(JsVal)),
                StreamOp::DropReadable | StreamOp::DropWritable | StreamOp::CancelRead => {
                    (&[I32, I32], None)
                }
            },
            Intrinsic::Map(op) => match op {
                MapOp::New(_) => (&[], Some(JsVal)),
                MapOp::Kind | MapOp::Size | MapOp::IterKind | MapOp::Step => (&[JsVal], Some(I32)),
                MapOp::Get => (&[JsVal, JsVal], Some(JsVal)),
                MapOp::Has | MapOp::Delete => (&[JsVal, JsVal], Some(Bool)),
                MapOp::Set => (&[JsVal, JsVal, JsVal], None),
                MapOp::Clear => (&[JsVal], None),
                MapOp::Iter(_) => (&[JsVal], Some(JsVal)),
                MapOp::Key | MapOp::Value => (&[JsVal, I32], Some(JsVal)),
            },
            Intrinsic::Typed(op) => match op {
                TypedOp::New => (&[I32, I32], Some(JsVal)),
                TypedOp::View => (&[I32, JsVal, I32, I32], Some(JsVal)),
                TypedOp::Kind | TypedOp::Length | TypedOp::ByteOffset | TypedOp::BufferLength | TypedOp::CtorKind => {
                    (&[JsVal], Some(I32))
                }
                TypedOp::Buffer => (&[JsVal], Some(JsVal)),
                TypedOp::IsBuffer => (&[JsVal], Some(Bool)),
                TypedOp::BufferNew => (&[I32], Some(JsVal)),
                TypedOp::BufferSlice => (&[JsVal, I32, I32], Some(JsVal)),
                TypedOp::Copy => (&[JsVal, I32, JsVal, I32, I32], None),
                TypedOp::ViewTracking => (&[I32, JsVal, I32], Some(JsVal)),
                TypedOp::OutOfBounds => (&[JsVal], Some(I32)),
                TypedOp::Tracking => (&[JsVal], Some(Bool)),
                TypedOp::BufferNewResizable => (&[I32, JsVal], Some(JsVal)),
                TypedOp::BufferMax => (&[JsVal], Some(JsVal)),
                TypedOp::BufferDetached => (&[JsVal], Some(Bool)),
                TypedOp::BufferResize => (&[JsVal, I32], None),
                TypedOp::BufferTransfer => (&[JsVal, I32, I32], Some(JsVal)),
                TypedOp::Latin1 => (&[JsVal, I32, I32, I32], Some(JsVal)),
                TypedOp::Utf8Length => (&[JsVal], Some(I32)),
                TypedOp::Utf8Write => (&[JsVal, JsVal, I32, I32], Some(I32)),
                TypedOp::Utf8Slice => (&[JsVal, I32, I32], Some(JsVal)),
            },
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Intrinsic::JsAdd => "js_add",
            Intrinsic::Concat => "concat",
            Intrinsic::ToString => "to_string",
            Intrinsic::NumberToString => "number_to_string",
            Intrinsic::JsonQuote => "json_quote",
            Intrinsic::StrCopy => "str_copy",
            Intrinsic::StrBufGet => "str_buf_get",
            Intrinsic::StrBufAppend => "str_buf_append",
            Intrinsic::StrBufLen => "str_buf_len",
            Intrinsic::ReAltsFind => "re_alts_find",
            Intrinsic::ReExec => "re_exec",
            Intrinsic::RegexId(_) => "regex_id",
            Intrinsic::MethodCheck { .. } => "method_check",
            Intrinsic::MethodFn(_) => "method_fn",
            Intrinsic::MethodDead(_) => "method_dead",
            Intrinsic::JsonQuoteInto => "json_quote_into",
            Intrinsic::JsonScan => "json_scan",
            Intrinsic::StringLength => "string_length",
            Intrinsic::TypeofString => "typeof",
            Intrinsic::Inspect => "inspect",
            Intrinsic::NoFormat => "no_format",
            Intrinsic::ConsoleWrite { stderr: false } => "write_stdout",
            Intrinsic::ConsoleWrite { stderr: true } => "write_stderr",
            Intrinsic::NewError(_) => "new_error",
            Intrinsic::NewObject => "new_object",
            Intrinsic::NewObjectSized => "new_object_sized",
            Intrinsic::GetProp => "get",
            Intrinsic::SetProp => "set",
            Intrinsic::DefineProp => "define",
            Intrinsic::DeleteProp => "delete",
            Intrinsic::HasProp => "has",
            Intrinsic::SetProto => "set_proto",
            Intrinsic::CopyProps => "copy_props",
            Intrinsic::ForInKeys => "for_in_keys",
            Intrinsic::KeysLen => "keys_len",
            Intrinsic::KeysGet => "keys_get",
            Intrinsic::ObjectCreate => "object_create",
            Intrinsic::GetPrototypeOf => "get_prototype_of",
            Intrinsic::GetOwnPropertySymbols => "get_own_property_symbols",
            Intrinsic::BuiltinTag => "builtin_tag",
            Intrinsic::ToObject => "to_object",
            Intrinsic::RegExpNew => "regexp_new",
            Intrinsic::RegExpData => "regexp_data",
            Intrinsic::DateNew => "date_new",
            Intrinsic::DateData => "date_data",
            Intrinsic::StackNew(false) => "stack_new",
            Intrinsic::StackNew(true) => "async_stack_new",
            Intrinsic::StackData => "stack_data",
            Intrinsic::ModuleNew => "module_new",
            Intrinsic::ToPrimitive => "to_primitive",
            Intrinsic::RegExpBuiltinExec => "regexp_builtin_exec",
            Intrinsic::RegExpPlain => "regexp_plain",
            Intrinsic::BigIntConst => "bigint",
            Intrinsic::ToNumeric => "to_numeric",
            Intrinsic::Numeric(_) => "numeric",
            Intrinsic::BigIntFromNumber => "bigint_from_number",
            Intrinsic::BigIntToNumber => "bigint_to_number",
            Intrinsic::BigIntToString => "bigint_to_string",
            Intrinsic::Unwrap => "unwrap",
            Intrinsic::InspectAt(_) => "inspect_at",
            Intrinsic::InspectDepth => "inspect_depth",
            Intrinsic::BuiltinToString => "builtin_to_string",
            Intrinsic::IsConstructor => "is_constructor",
            Intrinsic::HasOwn => "has_own",
            Intrinsic::InstanceOf(_) => "instanceof",
            Intrinsic::Prototype(_) => "prototype",
            Intrinsic::NewCell(_) => "new_cell",
            Intrinsic::NewCellUninit => "new_cell_uninit",
            Intrinsic::CellGet(_) => "cell_get",
            Intrinsic::CellSet(_) => "cell_set",
            Intrinsic::CellIsUninit => "cell_is_uninit",
            Intrinsic::EnvGet(_) => "env_get",
            Intrinsic::ArgGet(_) => "arg_get",
            Intrinsic::ArrayPush => "array_push",
            Intrinsic::ArraySpread => "array_spread",
            Intrinsic::ArgsRest(_) => "args_rest",
            Intrinsic::ArrayRest(_) => "array_rest",
            Intrinsic::IterOpen => "iter_open",
            Intrinsic::IterStep => "iter_step",
            Intrinsic::IterValue => "iter_value",
            Intrinsic::IterClose => "iter_close",
            Intrinsic::AsyncIterOpen => "async_iter_open",
            Intrinsic::IsAsyncIter => "is_async_iter",
            Intrinsic::AsyncIterNext => "async_iter_next",
            Intrinsic::AsyncIterResult => "async_iter_result",
            Intrinsic::AsyncIterReturn => "async_iter_return",
            Intrinsic::IterHasReturn => "iter_has_return",
            Intrinsic::IterRest => "iter_rest",
            Intrinsic::CallList => "call_list",
            Intrinsic::ObjectRest => "object_rest",
            Intrinsic::CheckDestructure => "check_destructure",
            Intrinsic::NewSymbol => "new_symbol",
            Intrinsic::SymbolIterator => "symbol_iterator",
            Intrinsic::WellKnownSymbol(_) => "well_known_symbol",
            Intrinsic::IsArray => "is_array",
            Intrinsic::IsPromise => "is_promise",
            Intrinsic::ObjectKeys => "object_keys",
            Intrinsic::ObjectValues => "object_values",
            Intrinsic::ObjectEntries => "object_entries",
            Intrinsic::GetOwnPropertyNames => "get_own_property_names",
            Intrinsic::ConstructList => "construct_list",
            Intrinsic::CreateThis => "create_this",
            Intrinsic::CreateThisSized => "create_this_sized",
            Intrinsic::CreateThisLayout(_) => "create_this_layout",
            Intrinsic::LayoutSet { .. } => "layout_set",
            Intrinsic::NewObjectLayout(_) => "new_object_layout",
            Intrinsic::LayoutDefine { .. } => "layout_define",
            Intrinsic::ClassHeritage => "class_heritage",
            Intrinsic::ClassSetup => "class_setup",
            Intrinsic::InstanceOfValue => "instanceof_value",
            Intrinsic::DefineMethod => "define_method",
            Intrinsic::DefineField => "define_field",
            Intrinsic::DefineAccessor { setter: false, .. } => "define_getter",
            Intrinsic::DefineAccessor { setter: true, .. } => "define_setter",
            Intrinsic::SuperGet => "super_get",
            Intrinsic::SuperSet => "super_set",
            Intrinsic::Hole => "hole",
            Intrinsic::IsHole => "is_hole",
            Intrinsic::CtorReturn { derived: false } => "base_ctor_return",
            Intrinsic::CtorReturn { derived: true } => "derived_ctor_return",
            Intrinsic::DefineProperty => "define_property",
            Intrinsic::GetOwnPropertyDescriptor => "get_own_property_descriptor",
            Intrinsic::Integrity(0) => "prevent_extensions",
            Intrinsic::Integrity(1) => "seal",
            Intrinsic::Integrity(_) => "freeze",
            Intrinsic::TestIntegrity(0) => "is_extensible",
            Intrinsic::TestIntegrity(1) => "is_sealed",
            Intrinsic::TestIntegrity(_) => "is_frozen",
            Intrinsic::ErrorCtor(_) => "error_constructor",
            Intrinsic::PromiseCtor => "promise_constructor",
            Intrinsic::QueueMicrotask => "queue_microtask",
            Intrinsic::Timer(k) => TIMER_NAMES[k as usize],
            Intrinsic::PrivateGet => "private_get",
            Intrinsic::PrivateSet => "private_set",
            Intrinsic::PrivateHas => "private_has",
            Intrinsic::PrivateAdd { kind } => match kind {
                PrivateKind::Field => "private_add_field",
                PrivateKind::Method => "private_add_method",
                PrivateKind::Accessor => "private_add_accessor",
            },
            Intrinsic::NewAccessor => "new_accessor",
            Intrinsic::ToPropertyKey => "to_property_key",
            Intrinsic::ObjectDesc => "object_desc",
            Intrinsic::ResumeMode => "resume_mode",
            Intrinsic::NewGenerator { .. } => "new_generator",
            Intrinsic::NewAsync { .. } => "new_async",
            Intrinsic::NewAsyncGenerator { .. } => "new_async_generator",
            Intrinsic::AsyncStart => "async_start",
            Intrinsic::GenLoad(_) => "gen_load",
            Intrinsic::GenStore(_) => "gen_store",
            Intrinsic::GenPoint => "gen_point",
            Intrinsic::GenSuspend { awaiting: true, .. } => "gen_suspend_await",
            Intrinsic::GenSuspend { raw: false, .. } => "gen_suspend",
            Intrinsic::GenSuspend { raw: true, .. } => "gen_suspend_raw",
            Intrinsic::DelegateOpen => "delegate_open",
            Intrinsic::DelegateStep => "delegate_step",
            Intrinsic::F64Hi => "f64_hi",
            Intrinsic::F64Lo => "f64_lo",
            Intrinsic::F64FromWords => "f64_from_words",
            Intrinsic::ElemNum => "elem_num",
            Intrinsic::F64IsHole => "f64_is_hole",
            Intrinsic::NumOrHole => "num_or_hole",
            Intrinsic::RandomSeed => "random_seed",
            Intrinsic::DateNow => "date_now",
            Intrinsic::Environment(_) => "environment",
            Intrinsic::MonotonicNow => "monotonic_now",
            Intrinsic::ProcessExit => "process_exit",
            Intrinsic::CallImport(_) => "call_import",
            Intrinsic::ImportCancel => "import_cancel",
            Intrinsic::StrUnit => "str_unit",
            Intrinsic::StrSub => "str_sub",
            Intrinsic::StrNew => "str_new",
            Intrinsic::StrSet => "str_set",
            Intrinsic::I32New => "i32_new",
            Intrinsic::I32Get => "i32_get",
            Intrinsic::I32Set => "i32_set",
            Intrinsic::I32Len => "i32_len",
            Intrinsic::JsLess { .. } => "js_less",
            Intrinsic::Trap => "trap",
            Intrinsic::MakeArguments => "make_arguments",
            Intrinsic::SetFunctionName => "set_function_name",
            Intrinsic::BuiltinCtor(_) => "builtin_ctor",
            Intrinsic::Namespace(_) => "namespace",
            Intrinsic::Map(op) => match op {
                MapOp::New(_) => "map_new",
                MapOp::Kind => "map_kind",
                MapOp::Get => "map_get",
                MapOp::Has => "map_has",
                MapOp::Set => "map_set",
                MapOp::Delete => "map_delete",
                MapOp::Clear => "map_clear",
                MapOp::Size => "map_size",
                MapOp::Iter(_) => "map_iter",
                MapOp::IterKind => "map_iter_kind",
                MapOp::Step => "map_step",
                MapOp::Key => "map_key",
                MapOp::Value => "map_value",
            },
            Intrinsic::Stream(op) => match op {
                StreamOp::Read => "stream_read",
                StreamOp::Write => "stream_write",
                StreamOp::ReadSync => "stream_read_sync",
                StreamOp::WriteSync => "stream_write_sync",
                StreamOp::DropReadable => "stream_drop_readable",
                StreamOp::DropWritable => "stream_drop_writable",
                StreamOp::CancelRead => "stream_cancel_read",
            },
            Intrinsic::Typed(op) => match op {
                TypedOp::New => "typed_new",
                TypedOp::View => "typed_view",
                TypedOp::Kind => "typed_kind",
                TypedOp::CtorKind => "typed_ctor_kind",
                TypedOp::Length => "typed_length",
                TypedOp::ByteOffset => "typed_byte_offset",
                TypedOp::Buffer => "typed_buffer",
                TypedOp::IsBuffer => "is_array_buffer",
                TypedOp::BufferNew => "buffer_new",
                TypedOp::BufferLength => "buffer_length",
                TypedOp::BufferSlice => "buffer_slice",
                TypedOp::Copy => "typed_copy",
                TypedOp::ViewTracking => "typed_view_tracking",
                TypedOp::OutOfBounds => "typed_out_of_bounds",
                TypedOp::Tracking => "typed_tracking",
                TypedOp::BufferNewResizable => "buffer_new_resizable",
                TypedOp::BufferMax => "buffer_max",
                TypedOp::BufferDetached => "buffer_detached",
                TypedOp::BufferResize => "buffer_resize",
                TypedOp::BufferTransfer => "buffer_transfer",
                TypedOp::Latin1 => "typed_latin1",
                TypedOp::Utf8Length => "typed_utf8_length",
                TypedOp::Utf8Write => "typed_utf8_write",
                TypedOp::Utf8Slice => "typed_utf8_slice",
            },
        }
    }

    /// Can it throw a JS value? (Traps don't count.)
    pub fn may_throw(self) -> bool {
        matches!(
            self,
            Intrinsic::GetProp
                | Intrinsic::SetProp
                | Intrinsic::DeleteProp
                | Intrinsic::HasProp
                | Intrinsic::ObjectCreate
                | Intrinsic::GetPrototypeOf
                | Intrinsic::HasOwn
                | Intrinsic::ArraySpread
                | Intrinsic::IterOpen
                | Intrinsic::IterStep
                | Intrinsic::IterValue
                | Intrinsic::IterClose
                | Intrinsic::IterRest
                | Intrinsic::AsyncIterOpen
                | Intrinsic::AsyncIterNext
                | Intrinsic::AsyncIterResult
                | Intrinsic::AsyncIterReturn
                | Intrinsic::IterHasReturn
                | Intrinsic::CallList
                | Intrinsic::ObjectRest
                | Intrinsic::CheckDestructure
                | Intrinsic::NewSymbol
                | Intrinsic::ObjectKeys
                | Intrinsic::ObjectValues
                | Intrinsic::ObjectEntries
                | Intrinsic::GetOwnPropertyNames
                | Intrinsic::ConstructList
                | Intrinsic::CreateThis
                | Intrinsic::CreateThisSized
                | Intrinsic::CreateThisLayout(_)
                | Intrinsic::LayoutSet { .. }
                | Intrinsic::ClassHeritage
                | Intrinsic::InstanceOfValue
                | Intrinsic::SuperGet
                | Intrinsic::SuperSet
                | Intrinsic::CtorReturn { .. }
                | Intrinsic::DefineProperty
                | Intrinsic::GetOwnPropertyDescriptor
                | Intrinsic::Integrity(_)
                | Intrinsic::TestIntegrity(_)
                | Intrinsic::PrivateGet
                | Intrinsic::PrivateSet
                | Intrinsic::PrivateHas
                | Intrinsic::ToPropertyKey
                | Intrinsic::PrivateAdd { .. }
                | Intrinsic::DelegateOpen
                | Intrinsic::DelegateStep
                | Intrinsic::CallImport(_)
                | Intrinsic::ImportCancel
                | Intrinsic::Stream(StreamOp::Write)
                | Intrinsic::ToNumeric
                | Intrinsic::Numeric(_)
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FnFlags {
    pub is_async: bool,
    pub is_generator: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    /// Source name, or a synthetic name such as `tmp` (printing adds the index, so names needn't be unique).
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    /// The first `params` entries of `locals` are the parameters.
    pub params: u32,
    pub locals: Vec<Local>,
    pub result: Option<Type>,
    pub flags: FnFlags,
    pub body: Vec<Stmt>,
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Const {
    F64(f64),
    I32(i32),
    Bool(bool),
    /// `jsval`.
    Undefined,
    /// `jsval`.
    Null,
}

impl Const {
    pub fn ty(&self) -> Type {
        match self {
            Const::F64(_) => Type::F64,
            Const::I32(_) => Type::I32,
            Const::Bool(_) => Type::Bool,
            Const::Undefined | Const::Null => Type::JsVal,
        }
    }
}

/// A result of JS `typeof`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeofTag {
    Undefined,
    Object,
    Boolean,
    Number,
    String,
    Symbol,
    Function,
    BigInt,
}

impl TypeofTag {
    pub const ALL: [TypeofTag; 8] = [
        TypeofTag::Undefined,
        TypeofTag::Object,
        TypeofTag::Boolean,
        TypeofTag::Number,
        TypeofTag::String,
        TypeofTag::Symbol,
        TypeofTag::Function,
        TypeofTag::BigInt,
    ];

    /// The string `typeof` returns.
    pub fn as_str(self) -> &'static str {
        match self {
            TypeofTag::Undefined => "undefined",
            TypeofTag::Object => "object",
            TypeofTag::Boolean => "boolean",
            TypeofTag::Number => "number",
            TypeofTag::String => "string",
            TypeofTag::Symbol => "symbol",
            TypeofTag::Function => "function",
            TypeofTag::BigInt => "bigint",
        }
    }

    pub fn parse(s: &str) -> Option<TypeofTag> {
        TypeofTag::ALL.into_iter().find(|t| t.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    /// `F64`: negate.
    Neg,
    /// `F64`: absolute value (used for number truthiness: `abs(x) > 0` is false for ±0 and NaN).
    Abs,
    /// `Bool`: logical not.
    Not,
    /// `I32`: bitwise not.
    BitNot,
    /// `F64` (T21): square root, rounding (IEEE, as Wasm's `f64.floor` …), `Fround`: to the nearest
    /// `f32` and back.
    Sqrt,
    Floor,
    Ceil,
    Trunc,
    Fround,
    /// `I32` (T21): count leading zeros.
    Clz,
}

/// [`Intrinsic::Numeric`]'s operators (T40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumericOp {
    /// `Sub`, `Mul`, `Div`, `Rem`, `Pow`, `And`, `Or`, `Xor`, `Shl`, `Shr`, `ShrU` (`+` is
    /// [`Intrinsic::JsAdd`]).
    Binary(BinOp),
    /// Unary `-` and `~`.
    Neg,
    BitNot,
    /// `v + 1` and `v - 1` of `++`/`--` (the operand already ToNumeric'd for a postfix value).
    Inc,
    Dec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    /// `F64`: JS `%` (truncated remainder, sign of dividend). `I32`: signed remainder.
    Rem,
    /// `**` (T24, `f64` only): Number::exponentiate.
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// `I32` only from here on.
    And,
    Or,
    Xor,
    Shl,
    /// Arithmetic (signed) shift right: JS `>>`.
    Shr,
    /// Logical shift right: JS `>>>`.
    ShrU,
    /// `JsVal` only: JS `==`.
    LooseEq,
    /// `JsVal` only: JS `!=`.
    LooseNe,
    /// `F64` only (T21): Wasm's `f64.min`/`max` (NaN if either is NaN, `-0 < +0`, as JS
    /// `Math.min`/`max` of two numbers) and `copysign` (`lhs` with the sign of `rhs`).
    Min,
    Max,
    CopySign,
}

impl BinOp {
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::LooseEq
                | BinOp::LooseNe
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SuspendKind {
    Await,
    Yield,
    /// `yield*`: the operand is the inner iterator's result object, passed out as is.
    YieldDelegate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expr(Expr),
    Set(LocalId, Expr),
    SetGlobal(GlobalId, Expr),
    Block {
        label: LabelId,
        body: Vec<Stmt>,
    },
    Loop {
        label: LabelId,
        body: Vec<Stmt>,
    },
    If {
        cond: Expr,
        then: Vec<Stmt>,
        els: Vec<Stmt>,
    },
    Break(LabelId),
    Continue(LabelId),
    Return(Option<Expr>),
    Try {
        body: Vec<Stmt>,
        catch_local: Option<LocalId>,
        catch_body: Vec<Stmt>,
    },
    Throw(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Const(Const),
    Local(LocalId),
    Global(GlobalId),
    Tee(LocalId, Box<Expr>),
    Unary {
        op: UnOp,
        ty: Type,
        arg: Box<Expr>,
    },
    Binary {
        op: BinOp,
        ty: Type,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Convert {
        from: Type,
        to: Type,
        arg: Box<Expr>,
    },
    Cond {
        cond: Box<Expr>,
        then: Box<Expr>,
        els: Box<Expr>,
        ty: Type,
    },
    Call {
        func: FuncId,
        args: Vec<Expr>,
    },
    Suspend {
        kind: SuspendKind,
        arg: Box<Expr>,
    },
    TypeofIs {
        arg: Box<Expr>,
        tag: TypeofTag,
    },
    Str(Vec<u16>),
    Intrinsic {
        op: Intrinsic,
        args: Vec<Expr>,
    },
    Seq {
        body: Vec<Stmt>,
        value: Box<Expr>,
    },
    Closure(Box<Closure>),
    /// An array literal: elements (`jsval`), `None` for a hole (T15).
    Array(Vec<Option<Expr>>),
    CallRef {
        callee: Box<Expr>,
        this: Box<Expr>,
        args: Vec<Expr>,
        /// How the callee is shown in the TypeError when it isn't a function (node's text).
        what: String,
    },
    /// `new callee(...args)` (T16).
    New {
        callee: Box<Expr>,
        /// `new.target` (`None`: the callee), for `super(…)`.
        new_target: Option<Box<Expr>>,
        args: Vec<Expr>,
        /// How the callee is shown in the TypeError when it isn't a constructor (node's text).
        what: String,
    },
}

/// A new function object (T14).
#[derive(Debug, Clone, PartialEq)]
pub struct Closure {
    /// What calls through the value run: a function `(self, this, args: jsval) -> jsval`
    /// (the function object, `this`, the argument array).
    pub code: FuncId,
    /// The `name` property.
    pub name: String,
    /// The `length` property.
    pub length: u32,
    /// What kind of function object (constructor or not).
    pub kind: FnKind,
    /// The environment: captured cells and values (`jsval`s), read back with `Intrinsic::EnvGet`.
    pub env: Vec<Expr>,
    /// One object per module, made on first use (a top-level function declaration): `env` is empty.
    pub shared: bool,
    /// Its source text, for `Function.prototype.toString` (T25; only with [`Module::to_string`],
    /// never for the prelude's functions: those print as native code).
    pub source: Option<String>,
}

/// What a [`Closure`] makes (T16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnKind {
    /// Not a constructor: an arrow function, a method, an accessor.
    Plain,
    /// A `function`: an own `prototype` object, `new` makes an object and calls the code.
    Ordinary,
    /// A `function` that uses `new.target` (T38) or stores `this` properties (T109: `this` made with room
    /// for them): like [`FnKind::Ordinary`], but `new` runs `construct` (the `$code` shape
    /// `(self, new_target, args) -> jsval`), which passes it.
    OrdinaryWith { construct: FuncId },
    /// A class: calling it throws (its code does), `new` runs `construct`, a function
    /// `(self, new_target, args) -> jsval` (the `$code` shape). Its `prototype` comes from
    /// [`Intrinsic::ClassSetup`].
    Class { construct: FuncId },
    /// A generator function (T18): not a constructor.
    Generator,
    /// An async function (T19): not a constructor.
    Async,
    /// An async generator function (T20): not a constructor.
    AsyncGenerator,
}

impl Expr {
    pub fn f64(v: f64) -> Expr {
        Expr::Const(Const::F64(v))
    }
    pub fn i32(v: i32) -> Expr {
        Expr::Const(Const::I32(v))
    }
    pub fn bool(v: bool) -> Expr {
        Expr::Const(Const::Bool(v))
    }
    pub fn local(l: LocalId) -> Expr {
        Expr::Local(l)
    }
    pub fn undefined() -> Expr {
        Expr::Const(Const::Undefined)
    }
    pub fn str(s: &str) -> Expr {
        Expr::Str(s.encode_utf16().collect())
    }
    pub fn intrinsic(op: Intrinsic, args: Vec<Expr>) -> Expr {
        Expr::Intrinsic { op, args }
    }
    pub fn convert(from: Type, to: Type, arg: Expr) -> Expr {
        Expr::Convert {
            from,
            to,
            arg: Box::new(arg),
        }
    }
    pub fn unary(op: UnOp, ty: Type, arg: Expr) -> Expr {
        Expr::Unary {
            op,
            ty,
            arg: Box::new(arg),
        }
    }
    pub fn binary(op: BinOp, ty: Type, lhs: Expr, rhs: Expr) -> Expr {
        Expr::Binary {
            op,
            ty,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    /// Result type, or `None` for a call to a function without a result.
    /// Assumes well-formed IR (ids in range).
    pub fn ty(&self, func: &Function, module: &Module) -> Option<Type> {
        Some(match self {
            Expr::Const(c) => c.ty(),
            Expr::Local(l) | Expr::Tee(l, _) => func.local(*l).ty,
            Expr::Global(g) => module.globals[g.0 as usize].ty,
            Expr::Unary { ty, .. } => *ty,
            Expr::Binary { op, ty, .. } => {
                if op.is_comparison() {
                    Type::Bool
                } else {
                    *ty
                }
            }
            Expr::Convert { to, .. } => *to,
            Expr::Cond { ty, .. } => *ty,
            Expr::Call { func: f, .. } => return module.functions[f.0 as usize].result,
            Expr::Suspend { .. } => Type::JsVal,
            Expr::TypeofIs { .. } => Type::Bool,
            Expr::Str(_) => Type::JsVal,
            Expr::Intrinsic { op, .. } => return op.signature().1,
            Expr::Seq { value, .. } => return value.ty(func, module),
            Expr::Closure(_) | Expr::CallRef { .. } | Expr::Array(_) | Expr::New { .. } => {
                Type::JsVal
            }
        })
    }
}

impl Function {
    /// A function with the given params and no extra locals or body yet.
    pub fn new(name: impl Into<String>, params: &[(&str, Type)], result: Option<Type>) -> Function {
        Function {
            name: name.into(),
            params: params.len() as u32,
            locals: params
                .iter()
                .map(|(n, ty)| Local {
                    name: n.to_string(),
                    ty: *ty,
                })
                .collect(),
            result,
            flags: FnFlags::default(),
            body: Vec::new(),
            pos: None,
        }
    }

    pub fn param(&self, i: u32) -> LocalId {
        assert!(i < self.params, "param {i} out of range");
        LocalId(i)
    }

    pub fn add_local(&mut self, name: impl Into<String>, ty: Type) -> LocalId {
        self.locals.push(Local {
            name: name.into(),
            ty,
        });
        LocalId(self.locals.len() as u32 - 1)
    }

    pub fn local(&self, l: LocalId) -> &Local {
        &self.locals[l.0 as usize]
    }
}

impl Module {
    pub fn add_function(&mut self, f: Function) -> FuncId {
        self.functions.push(f);
        FuncId(self.functions.len() as u32 - 1)
    }

    pub fn function(&self, f: FuncId) -> &Function {
        &self.functions[f.0 as usize]
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Type::F64 => "f64",
            Type::I32 => "i32",
            Type::Bool => "bool",
            Type::JsVal => "jsval",
        })
    }
}

#[cfg(test)]
mod tests;

/// Call `f` on every expression in `list`, outer ones first.
pub fn visit_exprs(list: &[Stmt], f: &mut dyn FnMut(&Expr)) {
    fn expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
        f(e);
        match e {
            Expr::Const(_) | Expr::Local(_) | Expr::Global(_) | Expr::Str(_) => {}
            Expr::Tee(_, a)
            | Expr::Unary { arg: a, .. }
            | Expr::Convert { arg: a, .. }
            | Expr::TypeofIs { arg: a, .. }
            | Expr::Suspend { arg: a, .. } => expr(a, f),
            Expr::Binary { lhs, rhs, .. } => {
                expr(lhs, f);
                expr(rhs, f);
            }
            Expr::Cond {
                cond, then, els, ..
            } => {
                expr(cond, f);
                expr(then, f);
                expr(els, f);
            }
            Expr::Call { args, .. } | Expr::Intrinsic { args, .. } => {
                args.iter().for_each(|a| expr(a, f))
            }
            Expr::Seq { body, value } => {
                visit_exprs(body, f);
                expr(value, f);
            }
            Expr::Closure(c) => c.env.iter().for_each(|a| expr(a, f)),
            Expr::Array(elems) => elems.iter().flatten().for_each(|a| expr(a, f)),
            Expr::CallRef {
                callee, this, args, ..
            } => {
                expr(callee, f);
                expr(this, f);
                args.iter().for_each(|a| expr(a, f));
            }
            Expr::New {
                callee,
                new_target,
                args,
                ..
            } => {
                expr(callee, f);
                new_target.iter().for_each(|a| expr(a, f));
                args.iter().for_each(|a| expr(a, f));
            }
        }
    }
    for s in list {
        match s {
            Stmt::Expr(e)
            | Stmt::Set(_, e)
            | Stmt::SetGlobal(_, e)
            | Stmt::Return(Some(e))
            | Stmt::Throw(e) => expr(e, f),
            Stmt::Block { body, .. } | Stmt::Loop { body, .. } => visit_exprs(body, f),
            Stmt::If { cond, then, els } => {
                expr(cond, f);
                visit_exprs(then, f);
                visit_exprs(els, f);
            }
            Stmt::Try {
                body, catch_body, ..
            } => {
                visit_exprs(body, f);
                visit_exprs(catch_body, f);
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(None) => {}
        }
    }
}
