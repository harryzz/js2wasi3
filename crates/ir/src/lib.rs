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
//! - `Suspend { kind, arg }` *(placeholder, T18/T19)*: `await` / `yield` point.

use std::fmt;

mod print;
mod throws;
mod validate;

pub use throws::may_throw;
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
    /// The command entry point (a `wasi:cli/run` command runs it), for a module without exports.
    /// Takes no params and has no result.
    pub main: Option<FuncId>,
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
    /// Types at the component boundary. `func` takes and returns `f64` for [`AbiType::F64`]
    /// and a JS string (`JsVal`) for [`AbiType::String`].
    pub sig: AbiSig,
}

/// A value type at the component boundary (a WIT type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbiType {
    F64,
    String,
}

impl AbiType {
    /// The IR type the exported function uses for it.
    pub fn ir_type(self) -> Type {
        match self {
            AbiType::F64 => Type::F64,
            AbiType::String => Type::JsVal,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AbiSig {
    pub params: Vec<AbiType>,
    pub result: Option<AbiType>,
    /// The function may throw a JS value: the WIT result is `result<T, js-error>` (T12).
    pub throws: bool,
}

impl AbiSig {
    /// All `f64` (the T07/T08 boundary), for a function with `params` parameters.
    pub fn numbers(params: u32, result: bool) -> AbiSig {
        AbiSig {
            params: vec![AbiType::F64; params as usize],
            result: result.then_some(AbiType::F64),
            throws: false,
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
}

impl ErrorKind {
    pub const ALL: [ErrorKind; 7] = [
        ErrorKind::Error,
        ErrorKind::TypeError,
        ErrorKind::RangeError,
        ErrorKind::ReferenceError,
        ErrorKind::SyntaxError,
        ErrorKind::EvalError,
        ErrorKind::UriError,
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
        }
    }

    pub fn parse(s: &str) -> Option<ErrorKind> {
        ErrorKind::ALL.into_iter().find(|k| k.name() == s)
    }
}

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
}

impl Builtin {
    /// All of them; a builtin's index here is its prototype's runtime slot.
    pub const ALL: [Builtin; 16] = [
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
        }
    }

    pub fn parse(s: &str) -> Option<Builtin> {
        Builtin::ALL.into_iter().find(|b| b.name() == s)
    }

    pub fn index(self) -> u32 {
        Builtin::ALL.iter().position(|b| *b == self).unwrap() as u32
    }
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
    ConsoleWrite { stderr: bool },
    /// `new E(message, options)` / `E(…)`: a new error object; an own `message` unless the
    /// message is `undefined`, an own `cause` if `options` is an object with one.
    NewError(ErrorKind),
    /// `{}`: a new plain object (prototype `Object.prototype`).
    NewObject,
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
    /// `Array.isArray(v)` (`bool`).
    IsArray,
    /// `Object.keys(v)`.
    ObjectKeys,
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
    DefineAccessor { setter: bool, enumerable: bool },
    /// `super[key]`: args: home object, key, `this`.
    SuperGet,
    /// `super[key] = v` (no result): args: home object, key, value, `this`.
    SuperSet,
    /// The uninitialized marker (a Wasm null): a derived constructor's `this` before `super()`.
    Hole,
    /// Is the value the uninitialized marker? (`bool`)
    IsHole,
    /// What a constructor returning `value` gives (args: value, `this`).
    CtorReturn { derived: bool },
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
    /// `o.#x`: args: object, private symbol, the TypeError message if `o` doesn't have it.
    PrivateGet,
    /// `o.#x = v` (no result): args: object, private symbol, value, the TypeError message if `o`
    /// doesn't have it.
    PrivateSet,
    /// `#x in o` (`bool`): args: private symbol, object.
    PrivateHas,
    /// Add a private field (or method: read-only) to a new object (no result): args: object, private
    /// symbol, value, the TypeError message if it's already there.
    PrivateAdd { method: bool },
}

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
            | Intrinsic::GetPrototypeOf => (&[JsVal], Some(JsVal)),
            Intrinsic::NewError(_) | Intrinsic::GetProp => (&[JsVal, JsVal], Some(JsVal)),
            Intrinsic::NewObject | Intrinsic::Prototype(_) => (&[], Some(JsVal)),
            Intrinsic::SetProp | Intrinsic::DefineProp => (&[JsVal, JsVal, JsVal], None),
            Intrinsic::DeleteProp | Intrinsic::HasProp | Intrinsic::HasOwn => {
                (&[JsVal, JsVal], Some(Bool))
            }
            Intrinsic::SetProto | Intrinsic::CopyProps => (&[JsVal, JsVal], None),
            Intrinsic::KeysLen => (&[JsVal], Some(I32)),
            Intrinsic::KeysGet => (&[JsVal, I32], Some(JsVal)),
            Intrinsic::InstanceOf(_) => (&[JsVal], Some(Bool)),
            Intrinsic::NumberToString => (&[F64], Some(JsVal)),
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
            | Intrinsic::IterValue
            | Intrinsic::IterRest
            | Intrinsic::NewSymbol
            | Intrinsic::ObjectKeys
            | Intrinsic::ObjectValues
            | Intrinsic::ObjectEntries => (&[JsVal], Some(JsVal)),
            Intrinsic::IterOpen | Intrinsic::ObjectRest => (&[JsVal, JsVal], Some(JsVal)),
            Intrinsic::IterStep | Intrinsic::IsArray => (&[JsVal], Some(Bool)),
            Intrinsic::IterClose => (&[JsVal, Bool], None),
            Intrinsic::CallList => (&[JsVal, JsVal, JsVal, JsVal], Some(JsVal)),
            Intrinsic::CheckDestructure => (&[JsVal, JsVal], None),
            Intrinsic::SymbolIterator => (&[], Some(JsVal)),
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
            | Intrinsic::QueueMicrotask => (&[], Some(JsVal)),
            Intrinsic::PrivateGet => (&[JsVal, JsVal, JsVal], Some(JsVal)),
            Intrinsic::PrivateSet | Intrinsic::PrivateAdd { .. } | Intrinsic::SuperSet => {
                (&[JsVal, JsVal, JsVal, JsVal], None)
            }
            Intrinsic::PrivateHas => (&[JsVal, JsVal], Some(Bool)),
            Intrinsic::IsHole | Intrinsic::TestIntegrity(_) => (&[JsVal], Some(Bool)),
            Intrinsic::CtorReturn { .. } | Intrinsic::GetOwnPropertyDescriptor => {
                (&[JsVal, JsVal], Some(JsVal))
            }
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Intrinsic::JsAdd => "js_add",
            Intrinsic::Concat => "concat",
            Intrinsic::ToString => "to_string",
            Intrinsic::NumberToString => "number_to_string",
            Intrinsic::StringLength => "string_length",
            Intrinsic::TypeofString => "typeof",
            Intrinsic::Inspect => "inspect",
            Intrinsic::NoFormat => "no_format",
            Intrinsic::ConsoleWrite { stderr: false } => "write_stdout",
            Intrinsic::ConsoleWrite { stderr: true } => "write_stderr",
            Intrinsic::NewError(_) => "new_error",
            Intrinsic::NewObject => "new_object",
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
            Intrinsic::IterOpen => "iter_open",
            Intrinsic::IterStep => "iter_step",
            Intrinsic::IterValue => "iter_value",
            Intrinsic::IterClose => "iter_close",
            Intrinsic::IterRest => "iter_rest",
            Intrinsic::CallList => "call_list",
            Intrinsic::ObjectRest => "object_rest",
            Intrinsic::CheckDestructure => "check_destructure",
            Intrinsic::NewSymbol => "new_symbol",
            Intrinsic::SymbolIterator => "symbol_iterator",
            Intrinsic::IsArray => "is_array",
            Intrinsic::ObjectKeys => "object_keys",
            Intrinsic::ObjectValues => "object_values",
            Intrinsic::ObjectEntries => "object_entries",
            Intrinsic::ConstructList => "construct_list",
            Intrinsic::CreateThis => "create_this",
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
            Intrinsic::PrivateGet => "private_get",
            Intrinsic::PrivateSet => "private_set",
            Intrinsic::PrivateHas => "private_has",
            Intrinsic::PrivateAdd { method: false } => "private_add_field",
            Intrinsic::PrivateAdd { method: true } => "private_add_method",
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
                | Intrinsic::CallList
                | Intrinsic::ObjectRest
                | Intrinsic::CheckDestructure
                | Intrinsic::NewSymbol
                | Intrinsic::ObjectKeys
                | Intrinsic::ObjectValues
                | Intrinsic::ObjectEntries
                | Intrinsic::ConstructList
                | Intrinsic::CreateThis
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
                | Intrinsic::PrivateAdd { .. }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    /// `F64`: JS `%` (truncated remainder, sign of dividend). `I32`: signed remainder.
    Rem,
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
}

/// What a [`Closure`] makes (T16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnKind {
    /// Not a constructor: an arrow function, a method, an accessor.
    Plain,
    /// A `function`: an own `prototype` object, `new` makes an object and calls the code.
    Ordinary,
    /// A class: calling it throws (its code does), `new` runs `construct`, a function
    /// `(self, new_target, args) -> jsval` (the `$code` shape). Its `prototype` comes from
    /// [`Intrinsic::ClassSetup`].
    Class { construct: FuncId },
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
