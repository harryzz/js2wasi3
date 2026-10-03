//! Objects (T13, design D1: dictionary mode, no hidden classes yet).
//!
//! # Layout
//! `$object { class: i32, proto: mut (ref null $object), keys: mut (ref $keys), vals: mut (ref $vals),
//! attrs: mut (ref $bytes), count: mut i32, index: mut (ref null $i32s) }`:
//! - Properties are entries `0..count` in insertion order: `keys[e]`, `vals[e]`, `attrs[e]` (bit 0: not
//!   enumerable). A deleted entry has a null key. Keys are strings (no symbols yet).
//! - Up to [`SMALL`] entries a lookup scans them. Beyond that an open-addressing hash index
//!   (FNV-1a over the UTF-16 units, linear probing) maps slots to `entry + 1` (0: empty). Deleted
//!   entries keep their slots until the arrays are compacted ([`Helper::Grow`]).
//! - `class`: [`CLASS_PLAIN`] or [`CLASS_ERROR`] (made by an error constructor).
//!
//! # Built-in prototypes
//! One per [`Builtin`], created on first use (globals from [`runtime::G_PROTOS`]) with their real
//! property names, all non-enumerable. Methods can't be values before functions exist (T14/T21),
//! so they hold the `G_BUILTIN` sentinel: `in` and shadowing see them, `for-in` skips them, and
//! reading one traps (`builtin_not_implemented`) instead of silently giving `undefined`.
//! `Error.prototype` has `name: "Error"`, `message: ""`; each native error prototype inherits
//! from it and has its own `name`.
//!
//! # Semantics
//! Get/set/delete/`in` follow the spec for data properties (no accessors, everything writable and
//! configurable) with strict-mode TypeErrors and node's messages. Primitives look properties up
//! on their prototype; strings also have `length` and index properties (read-only).
//! ToPrimitive uses the built-in `valueOf`/`toString` (`Error.prototype.toString`, or
//! `[object Object]` / `[object Error]`); a replaced one traps (calling values: T14).

use crate::runtime::JsTest;
use js2wasi3_ir::{Builtin, ErrorKind};
use wasm_encoder::{BlockType, Function, InstructionSink, ValType};

use crate::runtime::{
    self, Cx, G_BUILTIN, G_NULL, G_PROTOS, G_UNDEFINED, T_ARRAY, T_BYTES, T_I32S, T_KEYS, T_OBJECT,
    T_STRING, T_VALS, TAG, jsval, object, rt, string, string_ref,
};
use crate::{array, func, numstr, props, wrapper};

pub const F_CLASS: u32 = 0;
pub const F_PROTO: u32 = 1;
pub const F_KEYS: u32 = 2;
pub const F_VALS: u32 = 3;
pub const F_ATTRS: u32 = 4;
pub const F_COUNT: u32 = 5;
const F_INDEX: u32 = 6;

pub const CLASS_PLAIN: i32 = 0;
pub const CLASS_ERROR: i32 = 1;
/// A function's `arguments` object (T25; 2 and 3 are `Map`/`Set`, see `crate::mapset`).
pub const CLASS_ARGUMENTS: i32 = 4;
/// `Math` and `JSON` as values (T25): printed `Object [Math] {}`.
pub const CLASS_MATH: i32 = 5;
pub const CLASS_JSON: i32 = 6;
/// An async generator object (T20, a `$generator`, see `crate::asyncgen`).
pub const CLASS_ASYNC_GEN: i32 = 7;
/// A primitive wrapper object (T30, a `$wrapper`, see `crate::wrapper`).
pub const CLASS_WRAPPER: i32 = 8;
/// `Reflect` as a value (T36), like [`CLASS_MATH`].
pub const CLASS_REFLECT: i32 = 9;
/// A RegExp (T27): its data (`builtins/regexp.js`) is under the private `G_SYM_STATE` key.
pub const CLASS_REGEXP: i32 = 10;
/// A Date (T26): its data `[time value]` (`builtins/date.js`) is under the private `G_SYM_STATE` key.
pub const CLASS_DATE: i32 = 11;
/// A module namespace object (T41, `builtins/module.js`): its getters are shown as data
/// properties, writes fail with node's messages.
pub const CLASS_MODULE: i32 = 12;
/// A `DisposableStack` (14) or `AsyncDisposableStack` (15) (T45): its data (`builtins/dispose.js`) is
/// under the private `G_SYM_STATE` key.
pub const CLASS_STACK: i32 = 14;
pub const CLASS_ASYNC_STACK: i32 = 15;
/// The bits of `class` that hold the class (the others are flags, T16).
pub const CLASS_MASK: i32 = 0xff;
/// `class` flag: not extensible (`Object.preventExtensions`, `seal`, `freeze`).
pub const NO_EXTEND: i32 = 0x100;
/// `class` flag of an array: its elements are not configurable (`seal`, `freeze`).
pub const SEALED: i32 = 0x200;
/// `class` flag of an array: its elements and `length` are read-only too (`freeze`).
pub const FROZEN: i32 = 0x400;
/// `class` flag of an array: `length` is read-only (`defineProperty(a, "length", { writable: false })`,
/// T32).
pub const LENGTH_READONLY: i32 = 0x800;

/// Attribute bit: not enumerable.
pub const HIDDEN: i32 = 1;
/// Attribute bit: not writable (T16).
pub const READONLY: i32 = 2;
/// Attribute bit: not configurable (T16).
pub const FIXED: i32 = 4;
/// Attribute bit: an accessor property; the value slot holds an `$accessor` (T16).
pub const ACCESSOR: i32 = 8;
/// Attribute bit: a private member (`#x`, T16); its key is a private symbol. Invisible to
/// everything but the private-name helpers (not frozen, not listed).
pub const PRIVATE: i32 = 16;

/// Objects with more entries than this get a hash index.
const SMALL: i32 = 8;

/// Built-in methods (sentinels) of each prototype, besides `constructor`.
fn methods(b: Builtin) -> &'static [&'static str] {
    match b {
        Builtin::Object => &[
            "toString",
            "toLocaleString",
            "valueOf",
            "hasOwnProperty",
            "isPrototypeOf",
            "propertyIsEnumerable",
            "__proto__",
            "__defineGetter__",
            "__defineSetter__",
            "__lookupGetter__",
            "__lookupSetter__",
        ],
        Builtin::String => &[
            "length",
            "anchor",
            "at",
            "big",
            "blink",
            "bold",
            "charAt",
            "charCodeAt",
            "codePointAt",
            "concat",
            "endsWith",
            "fontcolor",
            "fontsize",
            "fixed",
            "includes",
            "indexOf",
            "isWellFormed",
            "italics",
            "lastIndexOf",
            "link",
            "localeCompare",
            "match",
            "matchAll",
            "normalize",
            "padEnd",
            "padStart",
            "repeat",
            "replace",
            "replaceAll",
            "search",
            "slice",
            "small",
            "split",
            "strike",
            "sub",
            "substr",
            "substring",
            "sup",
            "startsWith",
            "toString",
            "toWellFormed",
            "trim",
            "trimStart",
            "trimLeft",
            "trimEnd",
            "trimRight",
            "toLocaleLowerCase",
            "toLocaleUpperCase",
            "toLowerCase",
            "toUpperCase",
            "valueOf",
        ],
        Builtin::Number => &[
            "toExponential",
            "toFixed",
            "toPrecision",
            "toString",
            "valueOf",
            "toLocaleString",
        ],
        Builtin::Boolean => &["toString", "valueOf"],
        // `toString` is a real function (T16).
        Builtin::Error(ErrorKind::Error) => &["toString"],
        Builtin::Error(_) => &[],
        // `call` and `apply` are replaced by real functions (`crate::func`).
        Builtin::Function => &["apply", "bind", "call", "toString"],
        // `join`, `pop`, `push`, `toString` are real functions (`crate::array`).
        Builtin::Array => &[
            "at",
            "concat",
            "copyWithin",
            "fill",
            "find",
            "findIndex",
            "findLast",
            "findLastIndex",
            "lastIndexOf",
            "pop",
            "push",
            "reverse",
            "shift",
            "unshift",
            "slice",
            "sort",
            "splice",
            "includes",
            "indexOf",
            "join",
            "keys",
            "entries",
            "values",
            "forEach",
            "filter",
            "flat",
            "flatMap",
            "map",
            "every",
            "some",
            "reduce",
            "reduceRight",
            "toLocaleString",
            "toString",
            "toReversed",
            "toSorted",
            "toSpliced",
            "with",
        ],
        Builtin::Symbol => &["toString", "valueOf", "description"],
        Builtin::ArrayIterator | Builtin::Promise | Builtin::Generator => &[],
        // T25. `size` is a getter: no sentinel (a data sentinel would keep its attributes).
        Builtin::Map => &[
            "get", "set", "has", "delete", "clear", "entries", "forEach", "keys", "values",
        ],
        Builtin::Set => &[
            "has", "add", "delete", "clear", "entries", "forEach", "values", "keys",
        ],
        Builtin::MapIterator | Builtin::SetIterator => &["next"],
        // Native (T25, `crate::timers`).
        Builtin::Timeout | Builtin::Immediate | Builtin::AsyncGenerator => &[],
        // T20: from the prelude and `crate::typed`.
        Builtin::ArrayBuffer | Builtin::TypedArray | Builtin::Typed(_) => &[],
        // T27, T26: all from the prelude.
        Builtin::RegExp | Builtin::Date => &[],
        // T45: all from the prelude.
        Builtin::DisposableStack | Builtin::AsyncDisposableStack => &[],
        // T40: the prelude's methods replace them.
        Builtin::BigInt => &["toString", "toLocaleString", "valueOf"],
    }
}

/// The accessors of `RegExp.prototype` (T27, getters from the prelude).
const REGEXP_GETTERS: &[&str] = &[
    "source",
    "flags",
    "hasIndices",
    "global",
    "ignoreCase",
    "multiline",
    "dotAll",
    "unicode",
    "unicodeSets",
    "sticky",
];

fn parent(b: Builtin) -> Option<Builtin> {
    match b {
        Builtin::Object => None,
        Builtin::Error(k) if k != ErrorKind::Error => Some(Builtin::Error(ErrorKind::Error)),
        Builtin::Typed(_) => Some(Builtin::TypedArray),
        _ => Some(Builtin::Object),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(s: (ref $string)) -> i32`: FNV-1a.
    Hash,
    /// `(o: (ref $object), k: (ref $string)) -> i32`: own entry index or -1 (dictionary part).
    FindOwn,
    /// `(o: (ref $object), k: (ref $string)) -> eqref`: the own property's value (an array's
    /// elements and `length` included), Wasm null if there's none.
    OwnGet,
    /// `(o: (ref null $object), k) -> jsval`: the value on the prototype chain, Wasm null if absent.
    Lookup,
    /// `(o: (ref null $object), k) -> (ref null $object)`: the object on the chain that has `k`.
    Holder,
    /// `(o: (ref $object), k: (ref $string), hash: i32) -> i32`: [`Helper::FindOwn`] with `k`'s hash
    /// given (T74: a constant key's, computed at build time).
    FindOwnNamed,
    /// `(o: (ref null $object), k, hash: i32) -> jsval`: [`Helper::Lookup`] for a key that is neither an
    /// array index nor `length` (so no array own-get), with its hash given (T74).
    LookupNamed,
    /// `(v: jsval, k: (ref $string), hash: i32, site: i32) -> jsval`: `v.k` for a constant key that is
    /// neither an array index nor `length` (T74): an object first tries the entry its site's inline cache
    /// remembers (`G_IC[site]`, valid when that entry's key is `k` itself), then
    /// [`Helper::FindOwnNamed`] (remembered) and [`Helper::LookupNamed`] on the prototype; other values
    /// [`Helper::Get`].
    GetNamed,
    /// `(site: i32, e: i32)`: remember entry `e` for site `site` (T74), growing `G_IC`.
    IcStore,
    /// `(v: jsval) -> jsval`: `v.length` (T74): an array's or a string's directly, else [`Helper::Get`].
    GetLength,
    /// `(o, k, v: jsval, attrs: i32)`: set an own property (attributes only for a new one).
    Define,
    /// `(o, k, v, attrs, hash)`: a new own property `k` known to be absent, neither an array index nor
    /// `length`, with its hash (T74).
    AppendNamed,
    /// `(v, k: (ref $string), value, hash: i32)`: `v.k = value` for a constant key that is neither an
    /// array index, `length` nor `__proto__` (T74): objects through `ordinary_set_named`, others [`Helper::Set`].
    SetNamed,
    /// `(o: jsval (an object), k, v, hash)`: [`Helper::DefineProp`] for a constant key that is neither an
    /// array index nor `length` (T74; object literals).
    DefineNamed,
    /// `(o, k)`: delete an own property.
    Remove,
    /// `(o)`: make room for one more entry (compacting deleted ones).
    Grow,
    /// `(o, cap: i32)`: rebuild the hash index with `cap` (a power of 2) slots.
    Reindex,
    /// `(class: i32, proto: (ref null $object)) -> (ref $object)`: a new empty object.
    Alloc,
    /// `() -> (ref $object)`: a built-in prototype.
    Proto(Builtin),
    /// `(s: (ref $string)) -> i64`: the array index `s` denotes (canonical, < 2^32 - 1), or -1.
    ArrayIndex,
    /// `(v: jsval) -> (ref $string)`: ToPropertyKey.
    ToKey,
    /// `(v, key: jsval) -> jsval`: `v[key]`.
    Get,
    /// `(v, key, value: jsval)`: `v[key] = value`.
    Set,
    /// `(o, key, value: jsval)`: an own enumerable property of object `o` (literals).
    DefineProp,
    /// `(v, key) -> i32`: `delete v[key]`.
    Delete,
    /// `(key, v) -> i32`: `key in v`.
    Has,
    /// `(v, key) -> i32`: `Object.hasOwn(v, key)`.
    HasOwn,
    /// `() -> jsval`: `{}`.
    NewObject,
    /// `(o, p: jsval)`: `__proto__: p` in a literal.
    SetProto,
    /// `(target, src: jsval)`: `...src` in a literal.
    CopyProps,
    /// `(o: (ref $object)) -> (ref $keys)`: own enumerable keys in enumeration order.
    OwnKeys,
    /// `$own_names(o)`: like `OwnKeys`, non-enumerable keys too, and an array's `length`
    /// (`Object.getOwnPropertyNames`, T28).
    OwnNames,
    /// `(v) -> jsval`: the keys of `for (k in v)`.
    ForInKeys,
    /// `(p) -> jsval`: `Object.create(p)`.
    Create,
    /// `(v) -> jsval`: `Object.getPrototypeOf(v)`.
    GetPrototypeOf,
    /// `(v) -> i32`: `v instanceof B`.
    InstanceOf(Builtin),
    /// `(message, options: jsval) -> jsval`: `new E(message, options)`.
    NewError(ErrorKind),
    /// `(receiver) -> jsval`: an error's `stack` on its first read (T63): the header and a frame,
    /// then an own data property of the receiver.
    ErrorStack,
    /// `(o: (ref $object), hint: i32) -> jsval`: ToPrimitive (hint 0 default, 1 number, 2 string).
    ToPrimitive,
    /// `(o: (ref $object)) -> f64`: ToNumber of an object.
    ObjectToNumber,
    /// The code (a `$code`) of `Error.prototype.toString` (T16).
    ErrorToString,
    /// `(message: (ref $string))`: throw a new error of this kind.
    Throw(ErrorKind),
    /// `()`: trap: a built-in method was read.
    BuiltinTrap,
    /// `()`: trap: `console.log` of an object (F24).
    InspectTrap,
    /// `()`: trap: ToPrimitive reached a built-in method that isn't implemented (T21).
    UserPrimitiveTrap,
    /// `(src, excluded: jsval (an array of keys)) -> jsval`: `{...rest}` in a pattern (T15).
    ObjectRest,
    /// `(v, prefix: (ref $string))`: TypeError "<prefix>undefined." / "…null." for those.
    CheckDestructure,
    /// `(v) -> jsval`: `Object.keys` (0), `Object.values` (1), `Object.entries` (2).
    Keys(u8),
    /// `(o: (ref $object)) -> i32`: the number of live enumerable symbol keys.
    SymbolKeys,
    /// `(o: (ref $object), all: i32) -> jsval`: an array of the own symbol keys in insertion order
    /// (`all`: also non-enumerable ones; never private ones), T25.
    OwnSymbols,
    /// `(v) -> jsval`: `Object.getOwnPropertySymbols(v)` (T25).
    GetOwnSymbols,
    /// `(o: (ref $object)) -> jsval`: its `Symbol.toStringTag` (T25): `o[Symbol.toStringTag]` if
    /// the code names `toStringTag` and it's a string, else the built-in tag from its type (`Map`,
    /// `Promise`, `Generator`, `Array Iterator`, `Math`, …) or `undefined`.
    BuiltinTag,
    /// `(v) -> jsval`: [`Helper::BuiltinTag`] of an object, `undefined` for anything else.
    TagOf,
    /// `(v) -> i32`: node's `hasBuiltInToString` (T25): not an object, no `toString`, or one found
    /// on a built-in prototype (not an own one).
    BuiltinToString,
    /// `(data) -> jsval`: a new RegExp holding `data` (T27), with an own `lastIndex` of 0.
    RegExpNew,
    /// `(v) -> jsval`: a RegExp's data; `null` for `RegExp.prototype`; `undefined` otherwise.
    RegExpData,
    /// `(data) -> jsval`: a new Date holding `data` (T26).
    DateNew,
    /// `(v) -> jsval`: a Date's data; `undefined` otherwise (T26).
    DateData,
    /// `(data) -> jsval` (T45): a new `DisposableStack` (`false`) or `AsyncDisposableStack`.
    StackNew(bool),
    /// `(v) -> jsval` (T45): a stack's data, `undefined` for anything else.
    StackData,
    /// `() -> jsval`: a new module namespace object (T41): empty, null prototype.
    ModuleNew,
    /// `(v, hint: i32) -> jsval`: ToPrimitive of any value (T26): objects through [`Helper::ToPrimitive`].
    ToPrimitiveValue,
    /// `(args: jsval (an `$array`)) -> jsval`: an unmapped `arguments` object (T25): the indices,
    /// `length` and `[Symbol.iterator]` (`Array.prototype.values`), both hidden.
    MakeArguments,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Hash => "string_hash",
            Helper::FindOwn => "find_own",
            Helper::OwnGet => "own_get",
            Helper::Lookup => "lookup",
            Helper::Holder => "holder",
            Helper::FindOwnNamed => "find_own_named",
            Helper::LookupNamed => "lookup_named",
            Helper::GetNamed => "get_named",
            Helper::IcStore => "ic_store",
            Helper::GetLength => "get_length",
            Helper::Define => "define",
            Helper::AppendNamed => "append_named",
            Helper::DefineNamed => "define_named",
            Helper::SetNamed => "set_named",
            Helper::Remove => "remove",
            Helper::Grow => "grow",
            Helper::Reindex => "reindex",
            Helper::Alloc => "alloc_object",
            Helper::Proto(b) => match b {
                Builtin::Object => "Object.prototype",
                Builtin::String => "String.prototype",
                Builtin::Number => "Number.prototype",
                Builtin::Boolean => "Boolean.prototype",
                Builtin::Error(ErrorKind::Error) => "Error.prototype",
                Builtin::Error(ErrorKind::TypeError) => "TypeError.prototype",
                Builtin::Error(ErrorKind::RangeError) => "RangeError.prototype",
                Builtin::Error(ErrorKind::ReferenceError) => "ReferenceError.prototype",
                Builtin::Error(ErrorKind::SyntaxError) => "SyntaxError.prototype",
                Builtin::Error(ErrorKind::EvalError) => "EvalError.prototype",
                Builtin::Error(ErrorKind::UriError) => "URIError.prototype",
                Builtin::Error(ErrorKind::AggregateError) => "AggregateError.prototype",
                Builtin::Error(ErrorKind::SuppressedError) => "SuppressedError.prototype",
                Builtin::Function => "Function.prototype",
                Builtin::Array => "Array.prototype",
                Builtin::Symbol => "Symbol.prototype",
                Builtin::ArrayIterator => "%ArrayIteratorPrototype%",
                Builtin::Promise => "Promise.prototype",
                Builtin::Generator => "%GeneratorPrototype%",
                Builtin::Map => "Map.prototype",
                Builtin::Set => "Set.prototype",
                Builtin::MapIterator => "%MapIteratorPrototype%",
                Builtin::SetIterator => "%SetIteratorPrototype%",
                Builtin::Timeout => "Timeout.prototype",
                Builtin::Immediate => "Immediate.prototype",
                Builtin::AsyncGenerator => "%AsyncGeneratorPrototype%",
                Builtin::ArrayBuffer => "ArrayBuffer.prototype",
                Builtin::TypedArray => "%TypedArray%.prototype",
                Builtin::Typed(_) => "typed_array_prototype",
                Builtin::RegExp => "RegExp.prototype",
                Builtin::Date => "Date.prototype",
                Builtin::DisposableStack => "DisposableStack.prototype",
                Builtin::AsyncDisposableStack => "AsyncDisposableStack.prototype",
                Builtin::BigInt => "BigInt.prototype",
            },
            Helper::ArrayIndex => "array_index",
            Helper::ToKey => "to_property_key",
            Helper::Get => "get",
            Helper::Set => "set",
            Helper::DefineProp => "define_property",
            Helper::Delete => "delete",
            Helper::Has => "has",
            Helper::HasOwn => "has_own",
            Helper::NewObject => "new_object",
            Helper::SetProto => "set_proto",
            Helper::CopyProps => "copy_props",
            Helper::OwnKeys => "own_keys",
            Helper::OwnNames => "own_names",
            Helper::ForInKeys => "for_in_keys",
            Helper::Create => "object_create",
            Helper::GetPrototypeOf => "get_prototype_of",
            Helper::InstanceOf(_) => "instanceof",
            Helper::NewError(_) => "new_error",
            Helper::ErrorStack => "error_stack",
            Helper::ToPrimitive => "to_primitive",
            Helper::ObjectToNumber => "object_to_number",
            Helper::ErrorToString => "Error.prototype.toString",
            Helper::Throw(_) => "throw_error",
            Helper::BuiltinTrap => "builtin_not_implemented",
            Helper::InspectTrap => "inspect_object_not_implemented",
            Helper::UserPrimitiveTrap => "user_to_primitive_not_implemented",
            Helper::ObjectRest => "object_rest",
            Helper::CheckDestructure => "check_destructure",
            Helper::Keys(0) => "Object.keys",
            Helper::Keys(1) => "Object.values",
            Helper::Keys(3) => "Object.getOwnPropertyNames",
            Helper::Keys(_) => "Object.entries",
            Helper::SymbolKeys => "symbol_keys",
            Helper::OwnSymbols => "own_symbols",
            Helper::GetOwnSymbols => "Object.getOwnPropertySymbols",
            Helper::BuiltinTag => "builtin_tag",
            Helper::TagOf => "tag_of",
            Helper::BuiltinToString => "has_builtin_to_string",
            Helper::MakeArguments => "make_arguments",
            Helper::RegExpNew => "regexp_new",
            Helper::RegExpData => "regexp_data",
            Helper::DateNew => "date_new",
            Helper::ModuleNew => "module_new",
            Helper::DateData => "date_data",
            Helper::StackNew(false) => "stack_new",
            Helper::StackNew(true) => "async_stack_new",
            Helper::StackData => "stack_data",
            Helper::ToPrimitiveValue => "to_primitive_value",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::{F64, I32, I64};
        let v = jsval();
        let s = string_ref();
        let o = rt(false, T_OBJECT);
        let on = rt(true, T_OBJECT);
        match self {
            Helper::Hash => (vec![s], vec![I32]),
            Helper::FindOwn => (vec![o, s], vec![I32]),
            Helper::OwnGet => (vec![o, s], vec![v]),
            Helper::Lookup => (vec![on, s], vec![v]),
            Helper::Holder => (vec![on, s], vec![on]),
            Helper::FindOwnNamed => (vec![o, s, I32], vec![I32]),
            Helper::LookupNamed => (vec![on, s, I32], vec![v]),
            Helper::GetNamed => (vec![v, s, I32, I32], vec![v]),
            Helper::IcStore => (vec![I32, I32], vec![]),
            Helper::GetLength => (vec![v], vec![v]),
            Helper::Define => (vec![o, s, v, I32], vec![]),
            Helper::AppendNamed => (vec![o, s, v, I32, I32], vec![]),
            Helper::DefineNamed => (vec![v, s, v, I32], vec![]),
            Helper::SetNamed => (vec![v, s, v, I32], vec![]),
            Helper::Remove => (vec![o, s], vec![]),
            Helper::Grow => (vec![o], vec![]),
            Helper::Reindex => (vec![o, I32], vec![]),
            Helper::Alloc => (vec![I32, on], vec![o]),
            Helper::Proto(_) => (vec![], vec![o]),
            Helper::ArrayIndex => (vec![s], vec![I64]),
            Helper::ToKey => (vec![v], vec![s]),
            Helper::Get => (vec![v, v], vec![v]),
            Helper::Set | Helper::DefineProp => (vec![v, v, v], vec![]),
            Helper::Delete | Helper::Has | Helper::HasOwn => (vec![v, v], vec![I32]),
            Helper::NewObject => (vec![], vec![v]),
            Helper::SetProto | Helper::CopyProps => (vec![v, v], vec![]),
            Helper::OwnKeys | Helper::OwnNames => (vec![o], vec![rt(false, T_KEYS)]),
            Helper::ForInKeys | Helper::Create | Helper::GetPrototypeOf => (vec![v], vec![v]),
            Helper::InstanceOf(_) => (vec![v], vec![I32]),
            Helper::NewError(ErrorKind::AggregateError | ErrorKind::SuppressedError) => {
                (vec![v, v, v], vec![v])
            }
            Helper::NewError(_) => (vec![v, v], vec![v]),
            Helper::ErrorStack => (vec![v], vec![v]),
            Helper::ToPrimitive => (vec![o, I32], vec![v]),
            Helper::ObjectToNumber => (vec![o], vec![F64]),
            Helper::ErrorToString => (vec![v, v, v], vec![v]),
            Helper::Throw(_) => (vec![s], vec![]),
            Helper::BuiltinTrap | Helper::InspectTrap | Helper::UserPrimitiveTrap => {
                (vec![], vec![])
            }
            Helper::ObjectRest => (vec![v, v], vec![v]),
            Helper::CheckDestructure => (vec![v, s], vec![]),
            Helper::Keys(_) => (vec![v], vec![v]),
            Helper::SymbolKeys => (vec![o], vec![I32]),
            Helper::OwnSymbols => (vec![o, I32], vec![v]),
            Helper::GetOwnSymbols | Helper::TagOf => (vec![v], vec![v]),
            Helper::BuiltinTag => (vec![o], vec![v]),
            Helper::BuiltinToString => (vec![v], vec![I32]),
            Helper::MakeArguments
            | Helper::RegExpNew
            | Helper::RegExpData
            | Helper::DateNew
            | Helper::DateData
            | Helper::StackNew(_)
            | Helper::StackData => (vec![v], vec![v]),
            Helper::ModuleNew => (vec![], vec![v]),
            Helper::ToPrimitiveValue => (vec![v, I32], vec![v]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Hash => hash(),
            Helper::FindOwn => find_own(cx, false),
            Helper::FindOwnNamed => find_own(cx, true),
            Helper::LookupNamed => lookup_named(cx),
            Helper::GetNamed => get_named(cx),
            Helper::IcStore => ic_store(),
            Helper::GetLength => get_length(cx),
            Helper::OwnGet => own_get(cx),
            Helper::Lookup => lookup(cx, false),
            Helper::Holder => lookup(cx, true),
            Helper::Define => define(cx, false),
            Helper::AppendNamed => define(cx, true),
            Helper::DefineNamed => define_named(cx),
            Helper::SetNamed => set_named(cx),
            Helper::Remove => remove(cx),
            Helper::Grow => grow(cx),
            Helper::Reindex => reindex(cx),
            Helper::Alloc => alloc(),
            Helper::Proto(b) => proto(cx, b),
            Helper::ArrayIndex => array_index(),
            Helper::ToKey => to_key(cx),
            Helper::Get => get(cx),
            Helper::Set => set(cx),
            Helper::DefineProp => {
                let to_key = h(cx, Helper::ToKey);
                let define = h(cx, Helper::Define);
                let define_index = cx.helper(runtime::Helper::Arr(array::Helper::DefineIndex));
                let mut f = Function::new([]);
                let mut s = f.instructions();
                // An array and a small non-negative integer key: the element directly (T34).
                s.local_get(0).ref_test_non_null(array::array());
                s.local_get(1).ref_test_non_null(wasm_encoder::HeapType::I31);
                s.i32_and();
                s.if_(BlockType::Empty);
                s.local_get(1)
                    .ref_cast_non_null(wasm_encoder::HeapType::I31)
                    .i31_get_s()
                    .i32_const(0)
                    .i32_ge_s();
                s.if_(BlockType::Empty);
                s.local_get(0).ref_cast_non_null(array::array());
                s.local_get(1)
                    .ref_cast_non_null(wasm_encoder::HeapType::I31)
                    .i31_get_s();
                s.local_get(2).call(define_index).return_();
                s.end();
                s.end();
                as_obj(&mut s, 0);
                s.local_get(1)
                    .call(to_key)
                    .local_get(2)
                    .i32_const(0)
                    .call(define);
                s.end();
                f
            }
            Helper::Delete => delete(cx),
            Helper::Has => has(cx),
            Helper::HasOwn => has_own(cx),
            Helper::NewObject => new_object(cx),
            Helper::SetProto => set_proto(),
            Helper::CopyProps => copy_props(cx),
            Helper::OwnKeys => own_keys(cx, false),
            Helper::OwnNames => own_keys(cx, true),
            Helper::ForInKeys => for_in_keys(cx),
            Helper::Create => create(cx),
            Helper::GetPrototypeOf => get_prototype_of(cx),
            Helper::InstanceOf(b) => instance_of(cx, b),
            Helper::NewError(k) => new_error(cx, k),
            Helper::ErrorStack => error_stack(cx),
            Helper::ToPrimitive => to_primitive(cx),
            Helper::ObjectToNumber => object_to_number(cx),
            Helper::ErrorToString => error_to_string(cx),
            Helper::Throw(k) => throw(cx, k),
            Helper::BuiltinTrap | Helper::InspectTrap | Helper::UserPrimitiveTrap => {
                let mut f = Function::new([]);
                f.instructions().unreachable().end();
                f
            }
            Helper::ObjectRest => object_rest(cx),
            Helper::RegExpNew => regexp_new(cx),
            Helper::RegExpData => regexp_data(cx),
            Helper::DateNew => state_new(cx, CLASS_DATE, Builtin::Date),
            Helper::StackNew(false) => state_new(cx, CLASS_STACK, Builtin::DisposableStack),
            Helper::StackNew(true) => state_new(cx, CLASS_ASYNC_STACK, Builtin::AsyncDisposableStack),
            // Either class (`class | 1`).
            Helper::StackData => state_data(cx, CLASS_ASYNC_STACK),
            Helper::ModuleNew => module_new(cx),
            Helper::DateData => state_data(cx, CLASS_DATE),
            Helper::ToPrimitiveValue => to_primitive_value(cx),
            Helper::CheckDestructure => check_destructure(cx),
            Helper::Keys(k) => keys(cx, k),
            Helper::SymbolKeys => symbol_keys(),
            Helper::OwnSymbols => own_symbols(cx),
            Helper::BuiltinTag => builtin_tag(cx),
            Helper::BuiltinToString => {
                let holder = h(cx, Helper::Holder);
                let key = cx.str("toString");
                const V: u32 = 0;
                const HD: u32 = 1;
                let mut f = Function::new([(1, rt(true, T_OBJECT))]);
                let mut s = f.instructions();
                s.test_object(V).i32_eqz();
                s.if_(BlockType::Empty);
                s.i32_const(1).return_();
                s.end();
                s.local_get(V).ref_cast_non_null(object()).global_get(key);
                s.call(holder).local_tee(HD).ref_is_null();
                s.if_(BlockType::Empty);
                s.i32_const(1).return_();
                s.end();
                s.local_get(HD).local_get(V).ref_eq();
                s.if_(BlockType::Empty);
                s.i32_const(0).return_();
                s.end();
                for b in Builtin::ALL {
                    s.local_get(HD).global_get(runtime::proto_global(b)).ref_eq();
                    s.if_(BlockType::Empty);
                    s.i32_const(1).return_();
                    s.end();
                }
                s.i32_const(0);
                s.end();
                f
            }
            Helper::TagOf => {
                let tag = h(cx, Helper::BuiltinTag);
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.test_object(0);
                s.if_(BlockType::Result(jsval()));
                as_obj(&mut s, 0);
                s.call(tag);
                s.else_();
                s.global_get(G_UNDEFINED);
                s.end();
                s.end();
                f
            }
            Helper::GetOwnSymbols => {
                let own = h(cx, Helper::OwnSymbols);
                let new_array = rh(cx, runtime::Helper::Arr(array::Helper::New));
                let is_nullish = rh(cx, runtime::Helper::IsNullish);
                let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
                let msg = cx.str("Cannot convert undefined or null to object");
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.test_object(0);
                s.if_(BlockType::Empty);
                as_obj(&mut s, 0);
                s.i32_const(1).call(own).return_();
                s.end();
                s.local_get(0).call(is_nullish);
                s.if_(BlockType::Empty);
                s.global_get(msg).call(throw);
                s.end();
                s.i32_const(0)
                    .array_new_default(T_VALS)
                    .i32_const(0)
                    .call(new_array);
                s.end();
                f
            }
            Helper::MakeArguments => make_arguments(cx),
        }
    }
}

fn h(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Obj(h))
}

fn rh(cx: &mut dyn Cx, h: runtime::Helper) -> u32 {
    cx.helper(h)
}

/// Push "local `v` (a jsval) is an error object".
pub fn is_error(s: &mut InstructionSink, v: u32) {
    s.test_object(v);
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(v)
        .ref_cast_non_null(object())
        .struct_get(T_OBJECT, F_CLASS)
        .i32_const(CLASS_MASK)
        .i32_and()
        .i32_const(CLASS_ERROR)
        .i32_eq();
    s.else_();
    s.i32_const(0);
    s.end();
}

/// Push local `v` (a jsval) cast to `(ref $object)`.
pub fn as_obj(s: &mut InstructionSink, v: u32) {
    s.local_get(v).ref_cast_non_null(object());
}

/// `for (i = 0; i < n; i++) body`, with `i` and `n` locals already holding values
/// (`i` is set to 0 here). `body` may `br 1` to exit or `br 0` after incrementing itself.
pub fn count_loop(
    s: &mut InstructionSink,
    i: u32,
    n: u32,
    body: impl FnOnce(&mut InstructionSink),
) {
    s.i32_const(0).local_set(i);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(i).local_get(n).i32_ge_s().br_if(1);
    body(s);
    s.local_get(i).i32_const(1).i32_add().local_set(i);
    s.br(0).end().end();
}

fn hash() -> Function {
    const S: u32 = 0;
    const H: u32 = 1;
    const I: u32 = 2;
    const N: u32 = 3;
    let mut f = Function::new([(3, ValType::I32)]);
    let mut s = f.instructions();
    s.i32_const(0x811c9dc5_u32 as i32).local_set(H);
    s.local_get(S).array_len().local_set(N);
    count_loop(&mut s, I, N, |s| {
        s.local_get(H)
            .local_get(S)
            .local_get(I)
            .array_get_u(T_STRING)
            .i32_xor()
            .i32_const(16777619)
            .i32_mul()
            .local_set(H);
    });
    s.local_get(H);
    s.end();
    f
}

/// `define_named(o, k, v, hash)`: CreateDataProperty on object `o` (see [`Helper::DefineNamed`]).
fn define_named(cx: &mut dyn Cx) -> Function {
    let find = h(cx, Helper::FindOwnNamed);
    let append = h(cx, Helper::AppendNamed);
    const O: u32 = 0;
    const K: u32 = 1;
    const V: u32 = 2;
    const H: u32 = 3;
    const E: u32 = 4;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    as_obj(&mut s, O);
    s.local_get(K).local_get(H).call(find).local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    // An existing property: now a data property with this value (the other attributes stay).
    as_obj(&mut s, O);
    s.struct_get(T_OBJECT, F_VALS).local_get(E).local_get(V).array_set(T_VALS);
    as_obj(&mut s, O);
    s.struct_get(T_OBJECT, F_ATTRS).local_get(E);
    as_obj(&mut s, O);
    s.struct_get(T_OBJECT, F_ATTRS)
        .local_get(E)
        .array_get_u(T_BYTES)
        .i32_const(!ACCESSOR)
        .i32_and();
    s.array_set(T_BYTES);
    s.return_();
    s.end();
    as_obj(&mut s, O);
    s.local_get(K).local_get(V).i32_const(0).local_get(H).call(append);
    s.end();
    f
}

/// `set_named(v, k, value, hash)`: `v.k = value` (see [`Helper::SetNamed`]).
fn set_named(cx: &mut dyn Cx) -> Function {
    let ordinary = rh(cx, runtime::Helper::Prop(props::Helper::SetNamed));
    let set = h(cx, Helper::Set);
    const V: u32 = 0;
    const K: u32 = 1;
    const X: u32 = 2;
    const H: u32 = 3;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.test_object(V);
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.local_get(K).local_get(X).local_get(H).call(ordinary);
    s.else_();
    s.local_get(V).local_get(K).local_get(X).call(set);
    s.end();
    s.end();
    f
}

/// The runtime's key hash ([`Helper::Hash`], FNV-1a over the UTF-16 units), at build time (T74).
pub fn key_hash(units: &[u16]) -> i32 {
    let mut h: u32 = 0x811c9dc5;
    for &u in units {
        h = (h ^ u as u32).wrapping_mul(16777619);
    }
    h as i32
}

/// Can `v[k]` with this constant key take [`Helper::GetNamed`] (T74)? Not for an array index (elements,
/// typed arrays, string characters) or `length` (arrays and strings have it outside the dictionary).
pub fn named_key(units: &[u16]) -> bool {
    let is_index = !units.is_empty()
        && units.iter().all(|&u| (b'0' as u16..=b'9' as u16).contains(&u))
        && (units.len() == 1 || units[0] != b'0' as u16)
        && units.len() <= 10
        && String::from_utf16_lossy(units).parse::<u64>().is_ok_and(|n| n < u32::MAX as u64);
    !is_index && units != "length".encode_utf16().collect::<Vec<_>>().as_slice()
}

/// Does the key in local `kk` (a `(ref null $string)`, null: a deleted entry) equal `k`? Leaves an
/// i32. Identity first, then the lengths, before calling `string_eq` (T74).
fn key_matches(s: &mut InstructionSink, kk: u32, k: u32, str_eq: u32) {
    s.local_get(kk).ref_is_null();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(0);
    s.else_();
    s.local_get(kk).local_get(k).ref_eq();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(1);
    s.else_();
    s.local_get(kk).array_len().local_get(k).array_len().i32_ne();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(0);
    s.else_();
    // Same length: the first and the last unit decide most mismatches (`value_min`/`value_max`).
    // (Empty keys go to `string_eq`: a symbol, a `$string` subtype, equals only itself.)
    s.local_get(k).array_len().i32_eqz();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(kk).ref_as_non_null().local_get(k).call(str_eq);
    s.else_();
    s.local_get(kk).i32_const(0).array_get_u(T_STRING);
    s.local_get(k).i32_const(0).array_get_u(T_STRING);
    s.i32_ne();
    s.local_get(kk).local_get(kk).array_len().i32_const(1).i32_sub().array_get_u(T_STRING);
    s.local_get(k).local_get(k).array_len().i32_const(1).i32_sub().array_get_u(T_STRING);
    s.i32_ne();
    s.i32_or();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(0);
    s.else_();
    s.local_get(kk).ref_as_non_null().local_get(k).call(str_eq);
    s.end();
    s.end();
    s.end();
    s.end();
    s.end();
}

/// `find_own` (`(o, k)`), or with `hashed` `find_own_named` (`(o, k, hash)`: the hash given).
fn find_own(cx: &mut dyn Cx, hashed: bool) -> Function {
    let str_eq = rh(cx, runtime::Helper::StrEq);
    let hash = h(cx, Helper::Hash);
    const O: u32 = 0;
    const K: u32 = 1;
    // With `hashed`, param 2 is the hash and the locals start one later.
    let p = hashed as u32;
    let hv = 2;
    let e = 2 + p;
    let n = 3 + p;
    let mask = 4 + p;
    let i = 5 + p;
    let kk = 6 + p;
    let idx = 7 + p;
    let mut f = Function::new([
        (4, ValType::I32),
        (1, rt(true, T_STRING)),
        (1, rt(true, T_I32S)),
    ]);
    let mut s = f.instructions();
    // Is entry `e`'s key `k`? (leaves i32)
    let matches = |s: &mut InstructionSink| {
        s.local_get(O)
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(e)
            .array_get(T_KEYS)
            .local_set(kk);
        key_matches(s, kk, K, str_eq);
    };
    s.local_get(O)
        .struct_get(T_OBJECT, F_INDEX)
        .local_tee(idx)
        .ref_is_null();
    s.if_(BlockType::Empty);
    {
        s.local_get(O).struct_get(T_OBJECT, F_COUNT).local_set(n);
        count_loop(&mut s, e, n, |s| {
            matches(s);
            s.if_(BlockType::Empty);
            s.local_get(e).return_();
            s.end();
        });
        s.i32_const(-1).return_();
    }
    s.end();
    s.local_get(idx)
        .array_len()
        .i32_const(1)
        .i32_sub()
        .local_set(mask);
    if hashed {
        s.local_get(hv);
    } else {
        s.local_get(K).call(hash);
    }
    s.local_get(mask).i32_and().local_set(i);
    s.loop_(BlockType::Empty);
    s.local_get(idx).local_get(i).array_get(T_I32S).local_tee(e);
    s.i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(-1).return_();
    s.end();
    s.local_get(e).i32_const(1).i32_sub().local_set(e);
    matches(&mut s);
    s.if_(BlockType::Empty);
    s.local_get(e).return_();
    s.end();
    s.local_get(i)
        .i32_const(1)
        .i32_add()
        .local_get(mask)
        .i32_and()
        .local_set(i);
    s.br(0);
    s.end();
    s.unreachable();
    s.end();
    f
}

/// `lookup_named(o, k, hash)`: the prototype chain walk of [`lookup`] for a key that no array
/// element or `length` can have, so each level is a dictionary lookup with the given hash.
fn lookup_named(cx: &mut dyn Cx) -> Function {
    let find = h(cx, Helper::FindOwnNamed);
    const O: u32 = 0;
    const K: u32 = 1;
    const H: u32 = 2;
    const E: u32 = 3;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(O).ref_is_null().br_if(1);
    s.local_get(O)
        .ref_as_non_null()
        .local_get(K)
        .local_get(H)
        .call(find)
        .local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .struct_get(T_OBJECT, F_VALS)
        .local_get(E)
        .array_get(T_VALS)
        .return_();
    s.end();
    s.local_get(O).struct_get(T_OBJECT, F_PROTO).local_set(O);
    s.br(0).end().end();
    s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
    s.end();
    f
}

/// `get_named(v, k, hash, site)`: `v.k` (see [`Helper::GetNamed`]); the same steps as [`get`] for such a
/// key. A cached entry index is only a guess: it's used when that entry's key is `k` (keys are unique
/// within an object, and a deleted entry's key is null).
fn get_named(cx: &mut dyn Cx) -> Function {
    let find = h(cx, Helper::FindOwnNamed);
    let lookup = h(cx, Helper::LookupNamed);
    let store = h(cx, Helper::IcStore);
    let get = h(cx, Helper::Get);
    let sproto = h(cx, Helper::Proto(Builtin::String));
    let value = rh(cx, runtime::Helper::Prop(props::Helper::Value));
    const V: u32 = 0;
    const K: u32 = 1;
    const H: u32 = 2;
    const SITE: u32 = 3;
    const O: u32 = 4;
    const IC: u32 = 5;
    const E: u32 = 6;
    const R: u32 = 7;
    let mut f = Function::new([
        (1, rt(true, T_OBJECT)),
        (1, rt(true, T_I32S)),
        (1, ValType::I32),
        (1, jsval()),
    ]);
    let mut s = f.instructions();
    s.test_object(V);
    s.if_(BlockType::Result(jsval()));
    {
        as_obj(&mut s, V);
        s.local_set(O);
        // The cached entry.
        s.global_get(runtime::G_IC).local_tee(IC).ref_is_null().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(SITE).local_get(IC).array_len().i32_lt_u();
        s.if_(BlockType::Empty);
        s.local_get(IC).local_get(SITE).array_get(T_I32S).local_tee(E);
        s.local_get(O).struct_get(T_OBJECT, F_COUNT).i32_lt_u();
        s.if_(BlockType::Empty);
        s.local_get(O)
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(E)
            .array_get(T_KEYS)
            .local_get(K)
            .ref_eq();
        s.if_(BlockType::Empty);
        s.local_get(O)
            .struct_get(T_OBJECT, F_VALS)
            .local_get(E)
            .array_get(T_VALS)
            .local_tee(R);
        // A plain value as is; an accessor or a built-in sentinel through `property_value`.
        s.ref_test_non_null(wasm_encoder::HeapType::Concrete(runtime::T_ACCESSOR));
        s.local_get(R).global_get(runtime::G_BUILTIN).ref_eq();
        s.i32_or();
        s.if_(BlockType::Empty);
        s.local_get(R).local_get(V).call(value).return_();
        s.end();
        s.local_get(R).return_();
        s.end();
        s.end();
        s.end();
        s.end();
        // An own property: remember it.
        s.local_get(O)
            .ref_as_non_null()
            .local_get(K)
            .local_get(H)
            .call(find)
            .local_tee(E);
        s.i32_const(0).i32_ge_s();
        s.if_(BlockType::Result(jsval()));
        s.local_get(SITE).local_get(E).call(store);
        s.local_get(O)
            .struct_get(T_OBJECT, F_VALS)
            .local_get(E)
            .array_get(T_VALS);
        s.else_();
        s.local_get(O).struct_get(T_OBJECT, F_PROTO);
        s.local_get(K).local_get(H).call(lookup);
        s.end();
        s.local_get(V);
        s.call(value);
    }
    s.else_();
    // A string's methods: `String.prototype` (a constant key is never `length` or an index).
    runtime::is_string(&mut s, V);
    s.if_(BlockType::Result(jsval()));
    s.call(sproto);
    s.local_get(K).local_get(H).call(lookup);
    s.local_get(V);
    s.call(value);
    s.else_();
    s.local_get(V).local_get(K).call(get);
    s.end();
    s.end();
    s.end();
    f
}

/// `get_length(v)`: `v.length` (see [`Helper::GetLength`]).
fn get_length(cx: &mut dyn Cx) -> Function {
    let get = h(cx, Helper::Get);
    let box_ = rh(cx, runtime::Helper::Box);
    let length = cx.str("length");
    const V: u32 = 0;
    const N: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    let number = |s: &mut InstructionSink| {
        // An i31 below 2^30 (the runtime's small integers), else a boxed number.
        s.local_tee(N).i32_const(0x4000_0000).i32_lt_u();
        s.if_(BlockType::Result(jsval()));
        s.local_get(N).ref_i31();
        s.else_();
        s.local_get(N).f64_convert_i32_u().call(box_);
        s.end();
    };
    s.local_get(V).ref_test_non_null(array::array());
    s.if_(BlockType::Result(jsval()));
    s.local_get(V)
        .ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, array::F_LENGTH);
    number(&mut s);
    s.else_();
    runtime::is_string(&mut s, V);
    s.if_(BlockType::Result(jsval()));
    s.local_get(V).ref_cast_non_null(string()).array_len();
    number(&mut s);
    s.else_();
    s.local_get(V).global_get(length).call(get);
    s.end();
    s.end();
    s.end();
    f
}

/// `ic_store(site, e)`: `G_IC[site] = e`, growing the array (doubling, at least 1,024) when needed.
fn ic_store() -> Function {
    const SITE: u32 = 0;
    const E: u32 = 1;
    const IC: u32 = 2;
    const N: u32 = 3;
    const NEW: u32 = 4;
    let mut f = Function::new([(1, rt(true, T_I32S)), (1, ValType::I32), (1, rt(true, T_I32S))]);
    let mut s = f.instructions();
    s.global_get(runtime::G_IC).local_set(IC);
    s.local_get(IC).ref_is_null();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(0);
    s.else_();
    s.local_get(IC).array_len();
    s.end();
    s.local_tee(N);
    s.local_get(SITE).i32_le_u();
    s.if_(BlockType::Empty);
    {
        // max(2 * n, site + 1, 1024)
        s.local_get(N).i32_const(1).i32_shl().local_tee(N);
        s.local_get(SITE).i32_const(1).i32_add();
        s.local_get(N).local_get(SITE).i32_const(1).i32_add().i32_gt_u();
        s.select().local_tee(N);
        s.i32_const(1024);
        s.local_get(N).i32_const(1024).i32_gt_u();
        s.select();
        s.array_new_default(T_I32S).local_set(NEW);
        s.local_get(IC).ref_is_null().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(NEW)
            .i32_const(0)
            .local_get(IC)
            .i32_const(0)
            .local_get(IC)
            .array_len()
            .array_copy(T_I32S, T_I32S);
        s.end();
        s.local_get(NEW).local_tee(IC).global_set(runtime::G_IC);
    }
    s.end();
    s.local_get(IC).local_get(SITE).local_get(E).array_set(T_I32S);
    s.end();
    f
}

fn own_get(cx: &mut dyn Cx) -> Function {
    let find = h(cx, Helper::FindOwn);
    let array_own = cx.helper(runtime::Helper::Arr(array::Helper::OwnGet));
    const O: u32 = 0;
    const K: u32 = 1;
    const E: u32 = 2;
    const R: u32 = 3;
    let mut f = Function::new([(1, ValType::I32), (1, jsval())]);
    let mut s = f.instructions();
    s.local_get(O).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    s.local_get(O)
        .ref_cast_non_null(array::array())
        .local_get(K)
        .call(array_own)
        .local_tee(R);
    s.ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(R).return_();
    s.end();
    s.end();
    s.local_get(O).local_get(K).call(find).local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Result(jsval()));
    s.local_get(O)
        .struct_get(T_OBJECT, F_VALS)
        .local_get(E)
        .array_get(T_VALS);
    s.else_();
    s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
    s.end();
    s.end();
    f
}

/// `$lookup` / `$holder`: walk the prototype chain.
fn lookup(cx: &mut dyn Cx, holder: bool) -> Function {
    let own = h(cx, Helper::OwnGet);
    const O: u32 = 0;
    const K: u32 = 1;
    const R: u32 = 2;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(O).ref_is_null().br_if(1);
    s.local_get(O)
        .ref_as_non_null()
        .local_get(K)
        .call(own)
        .local_tee(R);
    s.ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    if holder {
        s.local_get(O).return_();
    } else {
        s.local_get(R).return_();
    }
    s.end();
    s.local_get(O).struct_get(T_OBJECT, F_PROTO).local_set(O);
    s.br(0).end().end();
    if holder {
        s.ref_null(object());
    } else {
        s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
    }
    s.end();
    f
}

/// `define(o, k, v, attrs)`; with `named`, `append_named(o, k, v, attrs, hash)` (T74): `k` is absent and
/// neither an array index nor `length` (the caller checked), so no array part, no search, the hash given.
fn define(cx: &mut dyn Cx, named: bool) -> Function {
    let find = h(cx, Helper::FindOwn);
    let index = h(cx, Helper::ArrayIndex);
    let grow = h(cx, Helper::Grow);
    let reindex = h(cx, Helper::Reindex);
    let hash = h(cx, Helper::Hash);
    const O: u32 = 0;
    const K: u32 = 1;
    const V: u32 = 2;
    const A: u32 = 3;
    // With `named`, param 4 is the hash and the locals start one later.
    const HV: u32 = 4;
    let p = named as u32;
    let (e, mask, i, idx) = (4 + p, 5 + p, 6 + p, 7 + p);
    let array_define = cx.helper(runtime::Helper::Arr(array::Helper::Define { set: false }));
    let mut f = Function::new([(3, ValType::I32), (1, rt(true, T_I32S))]);
    let mut s = f.instructions();
    if !named {
    // An array's index or `length`.
    s.local_get(O).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    s.local_get(O)
        .ref_cast_non_null(array::array())
        .local_get(K)
        .local_get(V)
        .call(array_define);
    s.if_(BlockType::Empty);
    s.return_();
    s.end();
    s.end();
    s.local_get(O).local_get(K).call(find).local_tee(e);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .struct_get(T_OBJECT, F_VALS)
        .local_get(e)
        .local_get(V)
        .array_set(T_VALS);
    // Now a data property (the other attributes stay).
    s.local_get(O).struct_get(T_OBJECT, F_ATTRS).local_get(e);
    s.local_get(O)
        .struct_get(T_OBJECT, F_ATTRS)
        .local_get(e)
        .array_get_u(T_BYTES)
        .i32_const(!ACCESSOR)
        .i32_and();
    s.array_set(T_BYTES);
    s.return_();
    s.end();
    // An index key on `Array.prototype` or `Object.prototype`: array writes must look at the
    // prototype chain from now on (T34).
    s.local_get(O).global_get(G_PROTOS + Builtin::Array.index()).ref_eq();
    s.local_get(O).global_get(G_PROTOS + Builtin::Object.index()).ref_eq();
    s.i32_or();
    s.if_(BlockType::Empty);
    s.local_get(K).call(index).i64_const(0).i64_ge_s();
    s.if_(BlockType::Empty);
    s.i32_const(1).global_set(runtime::G_INDEX_PROTO);
    s.end();
    s.end();
    }
    // A new entry at `count`.
    s.local_get(O).struct_get(T_OBJECT, F_COUNT);
    s.local_get(O).struct_get(T_OBJECT, F_KEYS).array_len();
    s.i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(O).call(grow);
    s.end();
    s.local_get(O).struct_get(T_OBJECT, F_COUNT).local_set(e);
    s.local_get(O)
        .struct_get(T_OBJECT, F_KEYS)
        .local_get(e)
        .local_get(K)
        .array_set(T_KEYS);
    s.local_get(O)
        .struct_get(T_OBJECT, F_VALS)
        .local_get(e)
        .local_get(V)
        .array_set(T_VALS);
    s.local_get(O)
        .struct_get(T_OBJECT, F_ATTRS)
        .local_get(e)
        .local_get(A)
        .array_set(T_BYTES);
    s.local_get(O)
        .local_get(e)
        .i32_const(1)
        .i32_add()
        .struct_set(T_OBJECT, F_COUNT);
    // Index it: start an index past SMALL entries, grow it past half full.
    s.local_get(O)
        .struct_get(T_OBJECT, F_INDEX)
        .local_tee(idx)
        .ref_is_null();
    s.if_(BlockType::Empty);
    s.local_get(e).i32_const(SMALL).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(O).i32_const(64).call(reindex);
    s.end();
    s.return_();
    s.end();
    s.local_get(e)
        .i32_const(1)
        .i32_add()
        .i32_const(1)
        .i32_shl()
        .local_get(idx)
        .array_len()
        .i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .local_get(idx)
        .array_len()
        .i32_const(1)
        .i32_shl()
        .call(reindex);
    s.return_();
    s.end();
    s.local_get(idx)
        .array_len()
        .i32_const(1)
        .i32_sub()
        .local_set(mask);
    if named {
        s.local_get(HV);
    } else {
        s.local_get(K).call(hash);
    }
    s.local_get(mask)
        .i32_and()
        .local_set(i);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(idx)
        .local_get(i)
        .array_get(T_I32S)
        .i32_eqz()
        .br_if(1);
    s.local_get(i)
        .i32_const(1)
        .i32_add()
        .local_get(mask)
        .i32_and()
        .local_set(i);
    s.br(0).end().end();
    s.local_get(idx)
        .local_get(i)
        .local_get(e)
        .i32_const(1)
        .i32_add()
        .array_set(T_I32S);
    s.end();
    f
}

fn remove(cx: &mut dyn Cx) -> Function {
    let find = h(cx, Helper::FindOwn);
    const O: u32 = 0;
    const K: u32 = 1;
    const E: u32 = 2;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(O).local_get(K).call(find).local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .struct_get(T_OBJECT, F_KEYS)
        .local_get(E)
        .ref_null(string())
        .array_set(T_KEYS);
    s.local_get(O)
        .struct_get(T_OBJECT, F_VALS)
        .local_get(E)
        .ref_null(wasm_encoder::RefType::EQREF.heap_type)
        .array_set(T_VALS);
    s.end();
    s.end();
    f
}

/// `$grow(o)`: new arrays of twice the live entries (at least 4), live entries copied in order.
fn grow(cx: &mut dyn Cx) -> Function {
    let reindex = h(cx, Helper::Reindex);
    const O: u32 = 0;
    const I: u32 = 1;
    const N: u32 = 2;
    const LIVE: u32 = 3;
    const J: u32 = 4;
    const KEYS: u32 = 5;
    const VALS: u32 = 6;
    const ATTRS: u32 = 7;
    let mut f = Function::new([
        (4, ValType::I32),
        (1, rt(true, T_KEYS)),
        (1, rt(true, T_VALS)),
        (1, rt(true, T_BYTES)),
    ]);
    let mut s = f.instructions();
    let key = |s: &mut InstructionSink| {
        s.local_get(O)
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(I)
            .array_get(T_KEYS);
    };
    s.local_get(O).struct_get(T_OBJECT, F_COUNT).local_set(N);
    count_loop(&mut s, I, N, |s| {
        key(s);
        s.ref_is_null().i32_eqz();
        s.local_get(LIVE).i32_add().local_set(LIVE);
    });
    // cap = max(4, 2 * (live + 1))
    s.local_get(LIVE)
        .i32_const(1)
        .i32_add()
        .i32_const(1)
        .i32_shl();
    s.local_tee(J);
    s.i32_const(4)
        .local_get(J)
        .i32_const(4)
        .i32_gt_u()
        .select()
        .local_set(J);
    s.local_get(J).array_new_default(T_KEYS).local_set(KEYS);
    s.local_get(J).array_new_default(T_VALS).local_set(VALS);
    s.local_get(J).array_new_default(T_BYTES).local_set(ATTRS);
    s.i32_const(0).local_set(J);
    count_loop(&mut s, I, N, |s| {
        key(s);
        s.ref_is_null().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(KEYS).local_get(J);
        key(s);
        s.array_set(T_KEYS);
        s.local_get(VALS).local_get(J);
        s.local_get(O)
            .struct_get(T_OBJECT, F_VALS)
            .local_get(I)
            .array_get(T_VALS);
        s.array_set(T_VALS);
        s.local_get(ATTRS).local_get(J);
        s.local_get(O)
            .struct_get(T_OBJECT, F_ATTRS)
            .local_get(I)
            .array_get_u(T_BYTES);
        s.array_set(T_BYTES);
        s.local_get(J).i32_const(1).i32_add().local_set(J);
        s.end();
    });
    s.local_get(O)
        .local_get(KEYS)
        .ref_as_non_null()
        .struct_set(T_OBJECT, F_KEYS);
    s.local_get(O)
        .local_get(VALS)
        .ref_as_non_null()
        .struct_set(T_OBJECT, F_VALS);
    s.local_get(O)
        .local_get(ATTRS)
        .ref_as_non_null()
        .struct_set(T_OBJECT, F_ATTRS);
    s.local_get(O).local_get(LIVE).struct_set(T_OBJECT, F_COUNT);
    // The old index points at the old entry numbers.
    s.local_get(O)
        .struct_get(T_OBJECT, F_INDEX)
        .ref_is_null()
        .i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .ref_null(wasm_encoder::HeapType::Concrete(T_I32S))
        .struct_set(T_OBJECT, F_INDEX);
    s.local_get(LIVE).i32_const(SMALL).i32_ge_s();
    s.if_(BlockType::Empty);
    // Smallest power of 2 ≥ 4 * live (at least 64).
    s.local_get(O);
    s.i32_const(64);
    s.i32_const(1)
        .i32_const(32)
        .local_get(LIVE)
        .i32_const(2)
        .i32_shl()
        .i32_const(1)
        .i32_sub()
        .i32_clz()
        .i32_sub()
        .i32_shl();
    s.local_tee(J);
    s.i32_const(64).local_get(J).i32_gt_u().select();
    s.call(reindex);
    s.end();
    s.end();
    s.end();
    f
}

fn reindex(cx: &mut dyn Cx) -> Function {
    let hash = h(cx, Helper::Hash);
    const O: u32 = 0;
    const CAP: u32 = 1;
    const E: u32 = 2;
    const N: u32 = 3;
    const I: u32 = 4;
    const MASK: u32 = 5;
    const KK: u32 = 6;
    const IDX: u32 = 7;
    let mut f = Function::new([
        (4, ValType::I32),
        (1, rt(true, T_STRING)),
        (1, rt(true, T_I32S)),
    ]);
    let mut s = f.instructions();
    s.local_get(CAP).array_new_default(T_I32S).local_set(IDX);
    s.local_get(CAP).i32_const(1).i32_sub().local_set(MASK);
    s.local_get(O).struct_get(T_OBJECT, F_COUNT).local_set(N);
    count_loop(&mut s, E, N, |s| {
        s.local_get(O)
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(E)
            .array_get(T_KEYS)
            .local_tee(KK)
            .ref_is_null()
            .i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(KK)
            .ref_as_non_null()
            .call(hash)
            .local_get(MASK)
            .i32_and()
            .local_set(I);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(IDX)
            .local_get(I)
            .array_get(T_I32S)
            .i32_eqz()
            .br_if(1);
        s.local_get(I)
            .i32_const(1)
            .i32_add()
            .local_get(MASK)
            .i32_and()
            .local_set(I);
        s.br(0).end().end();
        s.local_get(IDX)
            .local_get(I)
            .local_get(E)
            .i32_const(1)
            .i32_add()
            .array_set(T_I32S);
        s.end();
    });
    s.local_get(O).local_get(IDX).struct_set(T_OBJECT, F_INDEX);
    s.end();
    f
}

fn alloc() -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).local_get(1);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(wasm_encoder::HeapType::Concrete(T_I32S));
    s.struct_new(T_OBJECT);
    s.end();
    f
}

/// `$<B>.prototype()`: created on first use, with its properties (see the module docs).
fn proto(cx: &mut dyn Cx, b: Builtin) -> Function {
    let alloc = h(cx, Helper::Alloc);
    let define = h(cx, Helper::Define);
    let parent = parent(b).map(|p| h(cx, Helper::Proto(p)));
    let g = runtime::proto_global(b);
    // (%MapIteratorPrototype% and %SetIteratorPrototype% have no `constructor`, T25.)
    let mut props: Vec<(u32, Option<u32>)> =
        if matches!(b, Builtin::MapIterator | Builtin::SetIterator) {
            vec![]
        } else {
            vec![(cx.str("constructor"), None)]
        };
    for m in methods(b) {
        props.push((cx.str(m), None));
    }
    if let Builtin::Error(k) = b {
        props.push((cx.str("name"), Some(cx.str(k.name()))));
        props.push((cx.str("message"), Some(cx.str(""))));
    }
    if b == Builtin::Function {
        props.push((cx.str("name"), Some(cx.str(""))));
    }
    // Real methods (function objects), replacing their sentinels: (code, key, name, length).
    // (A `None` key is `Symbol.iterator`.)
    let values = runtime::Helper::Iter(crate::iter::Helper::ProtoValues);
    let named: Vec<&str> = ["keys", "entries"]
        .into_iter()
        .filter(|k| cx.names(k))
        .collect();
    let real: Vec<(runtime::Helper, Option<&str>, &str, i32)> = match b {
        Builtin::Function => vec![
            (
                runtime::Helper::Func(func::Helper::Call),
                Some("call"),
                "call",
                1,
            ),
            (
                runtime::Helper::Func(func::Helper::Apply),
                Some("apply"),
                "apply",
                2,
            ),
        ]
        .into_iter()
        // `bind` only when the code names it (T25): it links the bound-function helpers.
        .chain(cx.has_bind().then_some((
            runtime::Helper::Func(func::Helper::Bind),
            Some("bind"),
            "bind",
            1,
        )))
        // `toString` only when the code names it: user functions then carry their source.
        .chain(cx.has_to_string().then_some((
            runtime::Helper::Func(func::Helper::ToString),
            Some("toString"),
            "toString",
            0,
        )))
        .collect(),
        Builtin::Array => array::proto_methods()
            .into_iter()
            .map(|(name, h, len)| (runtime::Helper::Arr(h), Some(name), name, len))
            .chain([
                // (Also `[Symbol.iterator]`: the same function object.)
                (values, Some("values"), "values", 0),
            ])
            // `keys`/`entries` only when the code names them (T25).
            .chain(
                [
                    (crate::iter::Helper::ProtoKeys, "keys"),
                    (crate::iter::Helper::ProtoEntries, "entries"),
                ]
                .into_iter()
                .filter(|(_, k)| named.contains(k))
                .map(|(h, k)| (runtime::Helper::Iter(h), Some(k), k, 0)),
            )
            .collect(),
        Builtin::String => vec![(values, None, "[Symbol.iterator]", 0)],
        // T20: the array iterators work on any array-like.
        // T47: with resizable buffers, their own (ValidateTypedArray).
        Builtin::TypedArray if cx.rab() => [
            (crate::iter::ARRAY_LIKE, "values"),
            (crate::iter::ARRAY_KEYS, "keys"),
            (crate::iter::ARRAY_ENTRIES, "entries"),
        ]
        .into_iter()
        .filter(|(_, k)| *k == "values" || named.contains(k))
        .map(|(kind, k)| {
            let h = runtime::Helper::Iter(crate::iter::Helper::TypedProto(kind));
            (h, Some(k), k, 0)
        })
        .collect(),
        Builtin::TypedArray => [(values, "values")]
            .into_iter()
            .chain(
                [
                    (
                        runtime::Helper::Iter(crate::iter::Helper::ProtoKeys),
                        "keys",
                    ),
                    (
                        runtime::Helper::Iter(crate::iter::Helper::ProtoEntries),
                        "entries",
                    ),
                ]
                .into_iter()
                .filter(|(_, k)| named.contains(k)),
            )
            .map(|(h, k)| (h, Some(k), k, 0))
            .collect(),
        Builtin::Error(ErrorKind::Error) => vec![(
            runtime::Helper::Obj(Helper::ErrorToString),
            Some("toString"),
            "toString",
            0,
        )],
        Builtin::ArrayIterator => vec![
            (
                runtime::Helper::Iter(crate::iter::Helper::Next),
                Some("next"),
                "next",
                0,
            ),
            (
                runtime::Helper::Iter(crate::iter::Helper::This),
                None,
                "[Symbol.iterator]",
                0,
            ),
        ],
        Builtin::Promise => [
            (crate::promise::Helper::Then, "then", 2),
            (crate::promise::Helper::Catch, "catch", 1),
            (crate::promise::Helper::Finally, "finally", 1),
        ]
        .into_iter()
        .map(|(h, name, len)| (runtime::Helper::Prom(h), Some(name), name, len))
        .collect(),
        Builtin::Generator => {
            use crate::generator::Helper as G;
            use js2wasi3_ir::{RESUME_NEXT, RESUME_RETURN, RESUME_THROW};
            vec![
                (
                    runtime::Helper::Gen(G::Method(RESUME_NEXT)),
                    Some("next"),
                    "next",
                    1,
                ),
                (
                    runtime::Helper::Gen(G::Method(RESUME_RETURN)),
                    Some("return"),
                    "return",
                    1,
                ),
                (
                    runtime::Helper::Gen(G::Method(RESUME_THROW)),
                    Some("throw"),
                    "throw",
                    1,
                ),
                (
                    runtime::Helper::Iter(crate::iter::Helper::This),
                    None,
                    "[Symbol.iterator]",
                    0,
                ),
            ]
        }
        Builtin::MapIterator | Builtin::SetIterator => vec![(
            runtime::Helper::Iter(crate::iter::Helper::This),
            None,
            "[Symbol.iterator]",
            0,
        )],
        Builtin::AsyncGenerator => {
            use js2wasi3_ir::{RESUME_NEXT, RESUME_RETURN, RESUME_THROW};
            [
                (RESUME_NEXT, "next"),
                (RESUME_RETURN, "return"),
                (RESUME_THROW, "throw"),
            ]
            .into_iter()
            .map(|(m, name)| {
                let h = runtime::Helper::AsyncGen(crate::asyncgen::Helper::Method(m));
                (h, Some(name), name, 1)
            })
            .collect()
        }
        Builtin::Timeout | Builtin::Immediate => crate::timers::proto_methods(b)
            .into_iter()
            .map(|(h, name, len)| (runtime::Helper::Timer(h), Some(name), name, len))
            .collect(),
        _ => Vec::new(),
    };
    // (keys, name, code, length, getter): one function object defined under each key.
    // (keys, name, code, length, getter, `New` flags)
    let mut real: Vec<(Vec<u32>, u32, u32, i32, bool, i32)> = real
        .into_iter()
        .map(|(h, key, name, len)| {
            let code = cx.helper(h);
            cx.declare_func(code);
            let mut keys = vec![key.map_or(runtime::G_SYM_ITERATOR, |k| cx.str(k))];
            let typed_values = runtime::Helper::Iter(crate::iter::Helper::TypedProto(
                crate::iter::ARRAY_LIKE,
            ));
            if (h == values && matches!(b, Builtin::Array | Builtin::TypedArray)) || h == typed_values {
                keys.push(runtime::G_SYM_ITERATOR);
            }
            let flags = if h == values { func::NEW_VALUES } else { 0 };
            (keys, cx.str(name), code, len, false, flags)
        })
        .collect();
    // Methods written in the prelude (T21). T25: `Map.prototype[Symbol.iterator]` is `entries`,
    // `Set.prototype[Symbol.iterator]` and `keys` are `values`; `size` is a getter. T27: a key
    // `$name` is the well-known symbol `Symbol.name` (`$$RegExp_prototype_$match`).
    // (symbol, name, code, length, attributes, getter)
    let mut sym_methods: Vec<(u32, u32, u32, i32, i32, bool)> = Vec::new();
    for (key, code, len) in cx.builtin_methods(b) {
        cx.declare_func(code);
        if let Some(k) = key
            .strip_prefix('$')
            .and_then(js2wasi3_ir::well_known_symbol)
        {
            let sym = cx.helper(runtime::Helper::WellKnownSymbol(k));
            // T63: `%TypedArray%.prototype[Symbol.toStringTag]` is a getter.
            let getter = b == Builtin::TypedArray && key == "$toStringTag";
            let name = cx.str(&format!("{}[Symbol.{}]", if getter { "get " } else { "" }, &key[1..]));
            // `Date.prototype[Symbol.toPrimitive]` (T26) and `Function.prototype[Symbol.hasInstance]`
            // (T60) are read-only.
            let attrs = if key == "$toPrimitive" || key == "$hasInstance" { HIDDEN | READONLY } else { HIDDEN };
            sym_methods.push((sym, name, code, len as i32, attrs, getter));
            continue;
        }
        let getter = (matches!(b, Builtin::Map | Builtin::Set) && key == "size")
            || (b == Builtin::Symbol && key == "description")
            || (b == Builtin::TypedArray
                && ["length", "byteLength", "byteOffset", "buffer"].contains(&key.as_str()))
            || (b == Builtin::ArrayBuffer
                && ["byteLength", "maxByteLength", "resizable", "detached"].contains(&key.as_str()))
            || (b == Builtin::RegExp && REGEXP_GETTERS.contains(&key.as_str()))
            || (matches!(b, Builtin::DisposableStack | Builtin::AsyncDisposableStack)
                && key == "disposed");
        let name = cx.str(&if getter {
            format!("get {key}")
        } else {
            key.clone()
        });
        let mut keys = vec![cx.str(&key)];
        match (b, key.as_str()) {
            (Builtin::Map, "entries") => keys.push(runtime::G_SYM_ITERATOR),
            (Builtin::Set, "values") => keys.extend([cx.str("keys"), runtime::G_SYM_ITERATOR]),
            // Annex B (T26): the same function.
            (Builtin::Date, "toUTCString") => keys.push(cx.str("toGMTString")),
            _ => {}
        }
        real.push((keys, name, code, len as i32, getter, 0));
    }
    let define_accessor = (real.iter().any(|r| r.4) || sym_methods.iter().any(|m| m.5))
        .then(|| cx.helper(runtime::Helper::Prop(props::Helper::DefineAccessor)));
    let new_function = cx.helper(runtime::Helper::Func(func::Helper::New));
    // `length: 0` (Function.prototype, Array.prototype).
    let length = matches!(b, Builtin::Function | Builtin::Array).then(|| cx.str("length"));
    const O: u32 = 0;
    const M: u32 = 1;
    const PARENT: u32 = 2;
    let mut f = Function::new([(1, rt(true, T_OBJECT)), (1, jsval()), (1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.global_get(g).local_tee(O).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(O).ref_as_non_null().return_();
    s.end();
    if let Some(p) = parent {
        // The parent first, then check again: making it can make this prototype (T61:
        // `Object.prototype`'s `constructor` is a function, whose prototype is
        // `Function.prototype`), which must not be made twice.
        s.call(p).local_set(PARENT);
        s.global_get(g).local_tee(O).ref_is_null().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(O).ref_as_non_null().return_();
        s.end();
    }
    s.i32_const(CLASS_PLAIN);
    match parent {
        Some(_) => s.local_get(PARENT),
        None => s.ref_null(object()),
    };
    if b == Builtin::Function {
        // `Function.prototype` is a function (F47): `$function` fields after the object's.
        let empty = cx.helper(runtime::Helper::Func(func::Helper::Empty));
        cx.declare_func(empty);
        s.i32_const(0).array_new_default(T_KEYS);
        s.i32_const(0).array_new_default(T_VALS);
        s.i32_const(0).array_new_default(T_BYTES);
        s.i32_const(0);
        s.ref_null(wasm_encoder::HeapType::Concrete(T_I32S));
        s.ref_func(empty);
        s.ref_null(func::vals());
        s.ref_null(wasm_encoder::HeapType::Concrete(runtime::T_CODE));
        s.i32_const(func::KIND_FUNCTION);
        s.ref_null(wasm_encoder::HeapType::Concrete(T_STRING));
        s.struct_new(runtime::T_FUNCTION);
    } else {
        s.call(alloc);
    }
    s.local_tee(O).global_set(g);
    for (key, value) in props {
        s.local_get(O).ref_as_non_null().global_get(key);
        match value {
            Some(v) => s.global_get(v),
            None => s.global_get(G_BUILTIN),
        };
        s.i32_const(HIDDEN).call(define);
    }
    if let Some(length) = length {
        s.local_get(O).ref_as_non_null().global_get(length);
        s.i32_const(0).ref_i31();
        s.i32_const(HIDDEN).call(define);
    }
    for (sym, name, code, len, attrs, getter) in sym_methods {
        s.local_get(O).ref_as_non_null();
        s.call(sym)
            .ref_cast_non_null(wasm_encoder::HeapType::Concrete(T_STRING));
        s.ref_func(code);
        s.ref_null(wasm_encoder::HeapType::Concrete(runtime::T_CODE));
        s.ref_null(func::vals());
        s.global_get(name)
            .i32_const(len)
            .i32_const(0)
            .call(new_function);
        match define_accessor.filter(|_| getter) {
            Some(define_accessor) => s.i32_const(0).i32_const(attrs).call(define_accessor),
            None => s.i32_const(attrs).call(define),
        };
    }
    let exec = (b == Builtin::RegExp).then(|| cx.str("exec"));
    for (keys, name, code, len, getter, flags) in real {
        s.ref_func(code);
        s.ref_null(wasm_encoder::HeapType::Concrete(runtime::T_CODE));
        s.ref_null(func::vals());
        s.global_get(name)
            .i32_const(len)
            .i32_const(flags)
            .call(new_function)
            .local_set(M);
        if exec == Some(name) {
            s.local_get(M)
                .ref_cast_non_null(object())
                .global_set(runtime::G_REGEXP_EXEC);
        }
        for key in keys {
            s.local_get(O)
                .ref_as_non_null()
                .global_get(key)
                .local_get(M);
            if let Some(define_accessor) = define_accessor.filter(|_| getter) {
                s.i32_const(0).i32_const(HIDDEN).call(define_accessor);
            } else {
                s.i32_const(HIDDEN).call(define);
            }
        }
    }
    if let Builtin::Error(k) = b {
        // `constructor`: the function object (T16).
        let ctor = cx.helper(runtime::Helper::Func(func::Helper::ErrorCtor(k)));
        s.call(ctor).drop();
    }
    if b == Builtin::Promise {
        let ctor = cx.helper(runtime::Helper::Prom(crate::promise::Helper::Ctor));
        s.call(ctor).drop();
    }
    if let Builtin::Typed(k) = b {
        // T20: `BYTES_PER_ELEMENT` on each typed array prototype.
        let size = js2wasi3_ir::TYPED_SIZES[k as usize] as f64;
        func::define_constants(cx, &mut s, O, &[("BYTES_PER_ELEMENT", size)]);
    }
    if b == Builtin::AsyncGenerator {
        // `[Symbol.asyncIterator]` (`this`; it's `%AsyncIteratorPrototype%`'s, F70-like).
        let sym = cx.helper(runtime::Helper::WellKnownSymbol(
            js2wasi3_ir::well_known_symbol("asyncIterator").expect("well-known"),
        ));
        let code = cx.helper(runtime::Helper::Iter(crate::iter::Helper::This));
        cx.declare_func(code);
        let name = cx.str("[Symbol.asyncIterator]");
        s.local_get(O).ref_as_non_null();
        s.call(sym)
            .ref_cast_non_null(wasm_encoder::HeapType::Concrete(T_STRING));
        s.ref_func(code);
        s.ref_null(wasm_encoder::HeapType::Concrete(runtime::T_CODE));
        s.ref_null(func::vals());
        s.global_get(name)
            .i32_const(0)
            .i32_const(0)
            .call(new_function);
        s.i32_const(HIDDEN).call(define);
    }
    if b == Builtin::Timeout {
        // `Timeout.prototype[Symbol.toPrimitive]`: the timer's id (T25).
        let sym = cx.helper(runtime::Helper::WellKnownSymbol(
            js2wasi3_ir::well_known_symbol("toPrimitive").expect("well-known"),
        ));
        let code = cx.helper(runtime::Helper::Timer(crate::timers::Helper::ToPrimitive));
        cx.declare_func(code);
        let name = cx.str("[Symbol.toPrimitive]");
        s.local_get(O).ref_as_non_null();
        s.call(sym)
            .ref_cast_non_null(wasm_encoder::HeapType::Concrete(T_STRING));
        s.ref_func(code);
        s.ref_null(wasm_encoder::HeapType::Concrete(runtime::T_CODE));
        s.ref_null(func::vals());
        s.global_get(name)
            .i32_const(0)
            .i32_const(0)
            .call(new_function);
        s.i32_const(HIDDEN).call(define);
    }
    if let Some(tag) = builtin_tag_name(b).filter(|_| cx.names("toStringTag")) {
        // `[Symbol.toStringTag]` (T25; only when the code names it).
        define_tag(cx, &mut s, O, tag);
    }
    if cx.has_ctor(b) {
        // `constructor`: the constructor value (T25).
        let ctor = cx.helper(runtime::Helper::Func(func::Helper::BuiltinCtor(b)));
        s.call(ctor).drop();
    }
    s.local_get(O).ref_as_non_null();
    s.end();
    f
}

/// `$array_index(s) -> i64`: 0 or digits without a leading 0, below 2^32 - 1; else -1.
fn array_index() -> Function {
    const S: u32 = 0;
    const I: u32 = 1;
    const N: u32 = 2;
    const C: u32 = 3;
    const R: u32 = 4;
    let mut f = Function::new([(3, ValType::I32), (1, ValType::I64)]);
    let mut s = f.instructions();
    s.local_get(S).array_len().local_tee(N);
    s.i32_eqz();
    s.local_get(N).i32_const(10).i32_gt_u().i32_or();
    s.if_(BlockType::Empty);
    s.i64_const(-1).return_();
    s.end();
    // A leading 0 only for "0" itself.
    s.local_get(S)
        .i32_const(0)
        .array_get_u(T_STRING)
        .i32_const('0' as i32)
        .i32_eq();
    s.local_get(N).i32_const(1).i32_gt_u().i32_and();
    s.if_(BlockType::Empty);
    s.i64_const(-1).return_();
    s.end();
    count_loop(&mut s, I, N, |s| {
        s.local_get(S)
            .local_get(I)
            .array_get_u(T_STRING)
            .i32_const('0' as i32)
            .i32_sub()
            .local_tee(C)
            .i32_const(9)
            .i32_gt_u();
        s.if_(BlockType::Empty);
        s.i64_const(-1).return_();
        s.end();
        s.local_get(R)
            .i64_const(10)
            .i64_mul()
            .local_get(C)
            .i64_extend_i32_u()
            .i64_add()
            .local_set(R);
    });
    s.local_get(R).i64_const(0xffff_ffff).i64_lt_u();
    s.if_(BlockType::Result(ValType::I64));
    s.local_get(R);
    s.else_();
    s.i64_const(-1);
    s.end();
    s.end();
    f
}

/// ToPropertyKey: a symbol stays itself, anything else is ToString.
fn to_key(cx: &mut dyn Cx) -> Function {
    let to_string = rh(cx, runtime::Helper::ToString);
    let to_primitive = h(cx, Helper::ToPrimitive);
    const I: u32 = 1;
    const KEYS: u32 = 2;
    const K: u32 = 3;
    let keys = ValType::Ref(wasm_encoder::RefType {
        nullable: true,
        heap_type: wasm_encoder::HeapType::Concrete(runtime::T_KEYS),
    });
    let key = ValType::Ref(wasm_encoder::RefType {
        nullable: true,
        heap_type: string(),
    });
    let mut f = Function::new([(1, ValType::I32), (1, keys), (1, key)]);
    let mut s = f.instructions();
    // T75: an integer in [0, NUM_KEYS): the key made the first time.
    s.local_get(0)
        .ref_test_non_null(wasm_encoder::HeapType::I31);
    s.if_(BlockType::Empty);
    s.local_get(0)
        .ref_cast_non_null(wasm_encoder::HeapType::I31)
        .i31_get_s()
        .local_tee(I);
    s.i32_const(runtime::NUM_KEYS).i32_lt_u();
    s.if_(BlockType::Empty);
    s.global_get(runtime::G_NUM_KEYS)
        .local_tee(KEYS)
        .ref_is_null();
    s.if_(BlockType::Empty);
    s.i32_const(runtime::NUM_KEYS)
        .array_new_default(runtime::T_KEYS)
        .local_tee(KEYS)
        .global_set(runtime::G_NUM_KEYS);
    s.end();
    s.local_get(KEYS)
        .local_get(I)
        .array_get(runtime::T_KEYS)
        .local_tee(K);
    s.br_on_non_null(2);
    s.local_get(KEYS).local_get(I);
    s.local_get(0).call(to_string).local_tee(K);
    s.array_set(runtime::T_KEYS);
    s.local_get(K).ref_as_non_null().return_();
    s.end();
    s.end();
    // An object: its ToPrimitive (hint string), which may be a symbol.
    s.test_object(0);
    s.if_(BlockType::Empty);
    s.local_get(0)
        .ref_cast_non_null(runtime::object())
        .i32_const(2)
        .call(to_primitive)
        .local_set(0);
    s.end();
    s.local_get(0).ref_test_non_null(runtime::symbol());
    s.if_(BlockType::Result(string_ref()));
    s.local_get(0).ref_cast_non_null(string());
    s.else_();
    s.local_get(0).call(to_string);
    s.end();
    s.end();
    f
}

/// Push "string local `st` (a jsval that is a string) has own key `k` (a `(ref $string)` local)":
/// `length` or an index below its length.
fn string_has(cx: &mut dyn Cx, s: &mut InstructionSink, st: u32, k: u32) {
    let str_eq = rh(cx, runtime::Helper::StrEq);
    let index = h(cx, Helper::ArrayIndex);
    let length = cx.str("length");
    s.local_get(k)
        .ref_as_non_null()
        .global_get(length)
        .call(str_eq);
    s.local_get(k).ref_as_non_null().call(index);
    s.local_get(st)
        .ref_cast_non_null(string())
        .array_len()
        .i64_extend_i32_u()
        .i64_lt_u();
    s.i32_or();
}

/// Push the prototype of primitive local `v` (a string, number or boolean).
fn primitive_proto(cx: &mut dyn Cx, s: &mut InstructionSink, v: u32) {
    let is_number = rh(cx, runtime::Helper::IsNumber);
    let (sp, np, bp, yp) = (
        h(cx, Helper::Proto(Builtin::String)),
        h(cx, Helper::Proto(Builtin::Number)),
        h(cx, Helper::Proto(Builtin::Boolean)),
        h(cx, Helper::Proto(Builtin::Symbol)),
    );
    let big = cx.bigint();
    if big {
        // T40.
        let gp = h(cx, Helper::Proto(Builtin::BigInt));
        crate::bigint::is_bigint(s, v);
        s.if_(BlockType::Result(rt(false, T_OBJECT)));
        s.call(gp);
        s.else_();
    }
    s.local_get(v).ref_test_non_null(runtime::symbol());
    s.if_(BlockType::Result(rt(false, T_OBJECT)));
    s.call(yp);
    s.else_();
    runtime::is_string(s, v);
    s.if_(BlockType::Result(rt(false, T_OBJECT)));
    s.call(sp);
    s.else_();
    s.local_get(v).call(is_number);
    s.if_(BlockType::Result(rt(false, T_OBJECT)));
    s.call(np);
    s.else_();
    s.call(bp);
    s.end();
    s.end();
    s.end();
    if big {
        s.end();
    }
}

fn get(cx: &mut dyn Cx) -> Function {
    let to_key = h(cx, Helper::ToKey);
    let lookup = h(cx, Helper::Lookup);
    let is_nullish = rh(cx, runtime::Helper::IsNullish);
    let throw_read = rh(cx, runtime::Helper::ThrowRead);
    let str_eq = rh(cx, runtime::Helper::StrEq);
    let index = h(cx, Helper::ArrayIndex);
    let box_ = rh(cx, runtime::Helper::Box);
    let value = rh(cx, runtime::Helper::Prop(props::Helper::Value));
    let length = cx.str("length");
    const V: u32 = 0;
    const KEY: u32 = 1;
    const K: u32 = 2;
    const R: u32 = 3;
    const I: u32 = 4;
    let mut f = Function::new([
        (1, rt(true, T_STRING)),
        (1, jsval()),
        (1, ValType::I64),
        (1, ValType::I32),
    ]);
    let mut s = f.instructions();
    const J: u32 = 5;
    // Fast path: an array element at a small integer index.
    s.local_get(V).ref_test_non_null(array::array());
    s.local_get(KEY)
        .ref_test_non_null(wasm_encoder::HeapType::I31);
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(KEY)
        .ref_cast_non_null(wasm_encoder::HeapType::I31)
        .i31_get_s()
        .local_tee(J);
    s.local_get(V)
        .ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, array::F_LENGTH);
    s.i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(V)
        .ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, array::F_ELEMS)
        .local_get(J)
        .array_get(T_VALS)
        .local_tee(R);
    s.ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    // An accessor element (T32): its getter.
    s.local_get(R)
        .ref_test_non_null(wasm_encoder::HeapType::Concrete(runtime::T_ACCESSOR));
    s.if_(BlockType::Empty);
    s.local_get(R).local_get(V).call(value).return_();
    s.end();
    s.local_get(R).return_();
    s.end();
    s.end();
    s.end();
    if cx.names("$typed") {
        // A typed array (T20): an index key (an `i31` or an array-index string) reads the
        // element, or `undefined` out of range; never the prototype chain.
        let elem = rh(cx, runtime::Helper::Typed(crate::typed::Helper::Get));
        let length = crate::typed::Length::new(cx);
        let t = |s: &mut InstructionSink| {
            s.local_get(V).ref_cast_non_null(crate::typed::typed());
        };
        s.local_get(V).ref_test_non_null(crate::typed::typed());
        s.if_(BlockType::Empty);
        {
            s.local_get(KEY)
                .ref_test_non_null(wasm_encoder::HeapType::I31);
            s.if_(BlockType::Result(ValType::I64));
            s.local_get(KEY)
                .ref_cast_non_null(wasm_encoder::HeapType::I31)
                .i31_get_s()
                .i64_extend_i32_s();
            s.else_();
            s.test_string(KEY);
            s.if_(BlockType::Result(ValType::I64));
            s.local_get(KEY).ref_cast_non_null(string()).call(index);
            s.else_();
            s.i64_const(-2);
            s.end();
            s.end();
            s.local_tee(I);
            s.i64_const(-2).i64_ne();
            s.if_(BlockType::Empty);
            s.local_get(I).i64_const(0).i64_ge_s();
            t(&mut s);
            length.emit(&mut s);
            s.i64_extend_i32_u()
                .local_get(I)
                .i64_gt_s();
            s.i32_and();
            s.if_(BlockType::Empty);
            t(&mut s);
            s.local_get(I).i32_wrap_i64().call(elem).return_();
            s.end();
            // An index out of range, or a negative integer key.
            s.local_get(I).i64_const(-1).i64_ne();
            s.local_get(KEY)
                .ref_test_non_null(wasm_encoder::HeapType::I31);
            s.i32_or();
            s.if_(BlockType::Empty);
            s.global_get(runtime::G_UNDEFINED).return_();
            s.end();
            s.end();
        }
        s.end();
    }
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.local_get(V);
    key_text(cx, &mut s, KEY, to_key);
    s.call(throw_read);
    s.unreachable();
    s.end();
    s.local_get(KEY).call(to_key).local_set(K);
    s.test_object(V);
    s.if_(BlockType::Result(jsval()));
    as_obj(&mut s, V);
    s.local_get(K).ref_as_non_null().call(lookup);
    s.else_();
    runtime::is_string(&mut s, V);
    s.if_(BlockType::Empty);
    {
        s.local_get(K)
            .ref_as_non_null()
            .global_get(length)
            .call(str_eq);
        s.if_(BlockType::Empty);
        s.local_get(V)
            .ref_cast_non_null(string())
            .array_len()
            .f64_convert_i32_u()
            .call(box_)
            .return_();
        s.end();
        s.local_get(K).ref_as_non_null().call(index).local_tee(I);
        s.local_get(V)
            .ref_cast_non_null(string())
            .array_len()
            .i64_extend_i32_u()
            .i64_lt_u();
        s.if_(BlockType::Empty);
        s.local_get(V)
            .ref_cast_non_null(string())
            .local_get(I)
            .i32_wrap_i64()
            .array_get_u(T_STRING);
        s.array_new_fixed(T_STRING, 1).return_();
        s.end();
    }
    s.end();
    primitive_proto(cx, &mut s, V);
    s.local_get(K).ref_as_non_null().call(lookup);
    s.end();
    s.local_get(V);
    s.call(value);
    s.end();
    f
}

/// The key in local `key` as the TypeError for a `null`/`undefined` base names it (T52): an
/// object isn't converted (GetValue/PutValue throw before ToPropertyKey), V8 names it without side
/// effects (`#<Object>`); other keys by ToPropertyKey, which can't run user code for them.
/// A symbol is `Symbol(desc)`. (V8 leaves some objects out entirely, arrays and objects with their
/// own `toString` among them; here every object is `#<Class>`.)
fn key_text(cx: &mut dyn Cx, s: &mut InstructionSink, key: u32, to_key: u32) {
    let desc = rh(cx, runtime::Helper::Prop(props::Helper::Desc));
    let inspect = rh(cx, runtime::Helper::Inspect);
    s.test_object(key);
    s.if_(BlockType::Result(rt(false, T_STRING)));
    s.local_get(key).ref_cast_non_null(object()).call(desc);
    s.else_();
    s.local_get(key).ref_test_non_null(runtime::symbol());
    s.if_(BlockType::Result(rt(false, T_STRING)));
    s.local_get(key).call(inspect);
    s.else_();
    s.local_get(key).call(to_key).ref_as_non_null();
    s.end();
    s.end();
}

fn set(cx: &mut dyn Cx) -> Function {
    let to_key = h(cx, Helper::ToKey);
    let prop_set = rh(cx, runtime::Helper::Prop(props::Helper::Set));
    let holder = h(cx, Helper::Holder);
    let set_proto = h(cx, Helper::SetProto);
    let obj_proto = h(cx, Helper::Proto(Builtin::Object));
    let is_nullish = rh(cx, runtime::Helper::IsNullish);
    let to_string = rh(cx, runtime::Helper::ToString);
    let typeof_string = rh(cx, runtime::Helper::TypeofString);
    let concat = rh(cx, runtime::Helper::Concat);
    let str_eq = rh(cx, runtime::Helper::StrEq);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let proto_key = cx.str("__proto__");
    let set_of = cx.str("Cannot set properties of ");
    let setting = cx.str(" (setting '");
    let close = cx.str("')");
    let read_only = cx.str("Cannot assign to read only property '");
    let of_string = cx.str("' of string '");
    let create = cx.str("Cannot create property '");
    let on = cx.str("' on ");
    let space_quote = cx.str(" '");
    let quote = cx.str("'");
    const V: u32 = 0;
    const KEY: u32 = 1;
    const VAL: u32 = 2;
    const K: u32 = 3;
    let set_index = cx.helper(runtime::Helper::Arr(array::Helper::SetIndex));
    const X: u32 = 4;
    let mut f = Function::new([(1, rt(true, T_STRING)), (1, ValType::F64)]);
    let mut s = f.instructions();
    let k = |s: &mut InstructionSink| {
        s.local_get(K).ref_as_non_null();
    };
    if cx.names("$typed") {
        // A typed array element (T20): the value is converted first; out of range, dropped.
        let elem = rh(cx, runtime::Helper::Typed(crate::typed::Helper::Set));
        let to_number = rh(cx, runtime::Helper::ToNumber);
        let length = crate::typed::Length::new(cx);
        s.local_get(V).ref_test_non_null(crate::typed::typed());
        s.local_get(KEY)
            .ref_test_non_null(wasm_encoder::HeapType::I31);
        s.i32_and();
        s.if_(BlockType::Empty);
        {
            let key = |s: &mut InstructionSink| {
                s.local_get(KEY)
                    .ref_cast_non_null(wasm_encoder::HeapType::I31)
                    .i31_get_s();
            };
            if cx.big_typed() {
                // T48: a BigInt array: ToBigInt (the prelude's), then the store.
                let to_bigint = cx.to_bigint().expect("`$$toBigInt` with the BigInt arrays");
                let set_big = rh(cx, runtime::Helper::Typed(crate::typed::Helper::SetBig));
                s.local_get(V)
                    .ref_cast_non_null(crate::typed::typed())
                    .struct_get(runtime::T_TYPED, crate::typed::F_KIND)
                    .i32_const(js2wasi3_ir::TYPED_BIGINT64 as i32)
                    .i32_ge_u();
                s.if_(BlockType::Empty);
                s.local_get(VAL).call(to_bigint).local_set(VAL);
                key(&mut s);
                s.local_get(V).ref_cast_non_null(crate::typed::typed());
                length.emit(&mut s);
                s.i32_lt_u();
                s.if_(BlockType::Empty);
                s.local_get(V).ref_cast_non_null(crate::typed::typed());
                key(&mut s);
                s.local_get(VAL).call(set_big);
                s.end();
                s.return_();
                s.end();
            }
            s.local_get(VAL).call(to_number).local_set(X);
            key(&mut s);
            s.local_get(V).ref_cast_non_null(crate::typed::typed());
            length.emit(&mut s);
            s.i32_lt_u();
            s.if_(BlockType::Empty);
            s.local_get(V).ref_cast_non_null(crate::typed::typed());
            key(&mut s);
            s.local_get(X).call(elem);
            s.end();
            s.return_();
        }
        s.end();
    }
    // Fast path: an array element at a non-negative small integer index.
    s.local_get(V).ref_test_non_null(array::array());
    s.local_get(KEY)
        .ref_test_non_null(wasm_encoder::HeapType::I31);
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(KEY)
        .ref_cast_non_null(wasm_encoder::HeapType::I31)
        .i31_get_s()
        .i32_const(0)
        .i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(V).ref_cast_non_null(array::array());
    s.local_get(KEY)
        .ref_cast_non_null(wasm_encoder::HeapType::I31)
        .i31_get_s();
    s.local_get(VAL).call(set_index).return_();
    s.end();
    s.end();
    // Concatenate the string globals / pushes in `parts`, then throw.
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.global_get(set_of);
    s.local_get(V).call(to_string).call(concat);
    s.global_get(setting).call(concat);
    key_text(cx, &mut s, KEY, to_key);
    s.call(concat);
    s.global_get(close).call(concat);
    s.call(throw);
    s.end();
    s.local_get(KEY).call(to_key).local_set(K);
    s.test_object(V);
    s.if_(BlockType::Empty);
    {
        // `o.__proto__ = p` runs Object.prototype's `__proto__` setter.
        k(&mut s);
        s.global_get(proto_key).call(str_eq);
        s.if_(BlockType::Empty);
        as_obj(&mut s, V);
        k(&mut s);
        s.call(holder).call(obj_proto).ref_eq();
        s.if_(BlockType::Empty);
        s.local_get(V).local_get(VAL).call(set_proto).return_();
        s.end();
        s.end();
        as_obj(&mut s, V);
        k(&mut s);
        s.local_get(VAL).call(prop_set).return_();
    }
    s.end();
    runtime::is_string(&mut s, V);
    s.if_(BlockType::Empty);
    string_has(cx, &mut s, V, K);
    s.if_(BlockType::Empty);
    s.global_get(read_only);
    k(&mut s);
    s.call(concat);
    s.global_get(of_string).call(concat);
    s.local_get(V).ref_cast_non_null(string()).call(concat);
    s.global_get(quote).call(concat);
    s.call(throw);
    s.end();
    s.end();
    s.global_get(create);
    k(&mut s);
    s.call(concat);
    s.global_get(on).call(concat);
    s.local_get(V).call(typeof_string).call(concat);
    s.global_get(space_quote).call(concat);
    s.local_get(V).call(to_string).call(concat);
    s.global_get(quote).call(concat);
    s.call(throw);
    s.end();
    f
}

fn delete(cx: &mut dyn Cx) -> Function {
    let to_key = h(cx, Helper::ToKey);
    let remove = h(cx, Helper::Remove);
    let find = h(cx, Helper::FindOwn);
    let desc = rh(cx, runtime::Helper::Prop(props::Helper::Desc));
    let of_obj = cx.str("' of ");
    let is_nullish = rh(cx, runtime::Helper::IsNullish);
    let concat = rh(cx, runtime::Helper::Concat);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let convert = cx.str("Cannot convert undefined or null to object");
    let cannot = cx.str("Cannot delete property '");
    let of = cx.str("' of [object String]");
    let of_array = cx.str("' of [object Array]");
    let array_own = cx.helper(runtime::Helper::Arr(array::Helper::OwnGet));
    let elem_attrs = cx.helper(runtime::Helper::Arr(array::Helper::Attrs));
    let set_elem_attrs = cx.helper(runtime::Helper::Arr(array::Helper::SetAttrs));
    let index = h(cx, Helper::ArrayIndex);
    let str_eq = rh(cx, runtime::Helper::StrEq);
    let length = cx.str("length");
    const V: u32 = 0;
    const KEY: u32 = 1;
    const K: u32 = 2;
    const E: u32 = 3;
    let mut f = Function::new([(1, rt(true, T_STRING)), (1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.global_get(convert).call(throw);
    s.end();
    s.local_get(KEY).call(to_key).local_set(K);
    // An array: `length` can't be deleted; an element becomes a hole.
    s.local_get(V).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    s.local_get(K)
        .ref_as_non_null()
        .global_get(length)
        .call(str_eq);
    s.if_(BlockType::Empty);
    s.global_get(cannot);
    s.local_get(K).ref_as_non_null().call(concat);
    s.global_get(of_array).call(concat);
    s.call(throw);
    s.end();
    s.local_get(V)
        .ref_cast_non_null(array::array())
        .local_get(K)
        .ref_as_non_null()
        .call(array_own)
        .ref_is_null()
        .i32_eqz();
    s.if_(BlockType::Empty);
    // A non-configurable element (sealed, frozen, or by its attributes, T32) can't be deleted.
    s.local_get(K).ref_as_non_null().call(index).i32_wrap_i64().local_set(E);
    s.local_get(V).ref_cast_non_null(array::array());
    s.local_get(E).call(elem_attrs);
    s.i32_const(FIXED).i32_and();
    s.if_(BlockType::Empty);
    s.global_get(cannot);
    s.local_get(K).ref_as_non_null().call(concat);
    s.global_get(of_array).call(concat);
    s.call(throw);
    s.end();
    s.local_get(V)
        .ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, array::F_ELEMS);
    s.local_get(E);
    s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
    s.array_set(T_VALS);
    s.local_get(V).ref_cast_non_null(array::array());
    s.local_get(E).i32_const(0).call(set_elem_attrs);
    s.i32_const(1).return_();
    s.end();
    s.end();
    s.test_object(V);
    s.if_(BlockType::Empty);
    // A non-configurable property can't be deleted.
    as_obj(&mut s, V);
    s.local_get(K).ref_as_non_null().call(find).local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.struct_get(T_OBJECT, F_ATTRS)
        .local_get(E)
        .array_get_u(T_BYTES)
        .i32_const(FIXED)
        .i32_and();
    s.if_(BlockType::Empty);
    s.global_get(cannot);
    s.local_get(K).ref_as_non_null().call(concat);
    s.global_get(of_obj).call(concat);
    as_obj(&mut s, V);
    s.call(desc).call(concat);
    s.call(throw);
    s.end();
    s.end();
    as_obj(&mut s, V);
    s.local_get(K).ref_as_non_null().call(remove);
    s.i32_const(1).return_();
    s.end();
    runtime::is_string(&mut s, V);
    s.if_(BlockType::Empty);
    string_has(cx, &mut s, V, K);
    s.if_(BlockType::Empty);
    s.global_get(cannot);
    s.local_get(K).ref_as_non_null().call(concat);
    s.global_get(of).call(concat);
    s.call(throw);
    s.end();
    s.end();
    s.i32_const(1);
    s.end();
    f
}

fn has(cx: &mut dyn Cx) -> Function {
    let to_key = h(cx, Helper::ToKey);
    let lookup = h(cx, Helper::Lookup);
    let to_string = rh(cx, runtime::Helper::ToString);
    let concat = rh(cx, runtime::Helper::Concat);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let search = cx.str("Cannot use 'in' operator to search for '");
    let in_ = cx.str("' in ");
    const KEY: u32 = 0;
    const V: u32 = 1;
    const J: u32 = 2;
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    let mut s = f.instructions();
    if cx.names("$typed") {
        // A typed array (T20): an index key is there iff it's in range.
        let index = h(cx, Helper::ArrayIndex);
        let length = crate::typed::Length::new(cx);
        const IDX: u32 = 3;
        s.local_get(V).ref_test_non_null(crate::typed::typed());
        s.if_(BlockType::Empty);
        {
            s.local_get(KEY)
                .ref_test_non_null(wasm_encoder::HeapType::I31);
            s.if_(BlockType::Result(ValType::I64));
            s.local_get(KEY)
                .ref_cast_non_null(wasm_encoder::HeapType::I31)
                .i31_get_s()
                .i64_extend_i32_s();
            s.else_();
            s.local_get(KEY).call(to_key).call(index);
            s.end();
            s.local_tee(IDX).i64_const(-1).i64_ne();
            s.if_(BlockType::Empty);
            s.local_get(IDX).i64_const(0).i64_ge_s();
            s.local_get(V).ref_cast_non_null(crate::typed::typed());
            length.emit(&mut s);
            s.i64_extend_i32_u()
                .local_get(IDX)
                .i64_gt_s();
            s.i32_and();
            s.return_();
            s.end();
        }
        s.end();
    }
    // Fast path: an array element at a small integer index (holes take the slow path).
    s.local_get(V).ref_test_non_null(array::array());
    s.local_get(KEY)
        .ref_test_non_null(wasm_encoder::HeapType::I31);
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(KEY)
        .ref_cast_non_null(wasm_encoder::HeapType::I31)
        .i31_get_s()
        .local_tee(J);
    s.local_get(V)
        .ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, array::F_LENGTH);
    s.i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(V)
        .ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, array::F_ELEMS)
        .local_get(J)
        .array_get(T_VALS)
        .ref_is_null()
        .i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(1).return_();
    s.end();
    s.end();
    s.end();
    s.test_object(V).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(search);
    s.local_get(KEY).call(to_key).call(concat);
    s.global_get(in_).call(concat);
    s.local_get(V).call(to_string).call(concat);
    s.call(throw);
    s.end();
    as_obj(&mut s, V);
    s.local_get(KEY).call(to_key).call(lookup);
    s.ref_is_null().i32_eqz();
    s.end();
    f
}

fn has_own(cx: &mut dyn Cx) -> Function {
    let to_key = h(cx, Helper::ToKey);
    let own = h(cx, Helper::OwnGet);
    let is_nullish = rh(cx, runtime::Helper::IsNullish);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let convert = cx.str("Cannot convert undefined or null to object");
    const V: u32 = 0;
    const KEY: u32 = 1;
    const K: u32 = 2;
    let mut f = Function::new([(1, rt(true, T_STRING))]);
    let mut s = f.instructions();
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.global_get(convert).call(throw);
    s.end();
    s.local_get(KEY).call(to_key).local_set(K);
    s.test_object(V);
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.local_get(K).ref_as_non_null().call(own);
    s.ref_is_null().i32_eqz().return_();
    s.end();
    runtime::is_string(&mut s, V);
    s.if_(BlockType::Empty);
    string_has(cx, &mut s, V, K);
    s.return_();
    s.end();
    s.i32_const(0);
    s.end();
    f
}

fn new_object(cx: &mut dyn Cx) -> Function {
    let alloc = h(cx, Helper::Alloc);
    let proto = h(cx, Helper::Proto(Builtin::Object));
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.i32_const(CLASS_PLAIN).call(proto).call(alloc);
    s.end();
    f
}

/// `$set_proto(o, p)`: `p` an object → it; `null` → none; anything else is ignored.
fn set_proto() -> Function {
    const O: u32 = 0;
    const P: u32 = 1;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    // An array with another prototype: its writes look at the chain (T34).
    s.local_get(O).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    s.i32_const(1).global_set(runtime::G_INDEX_PROTO);
    s.end();
    s.test_object(P);
    s.if_(BlockType::Empty);
    as_obj(&mut s, O);
    s.local_get(P)
        .ref_cast_non_null(object())
        .struct_set(T_OBJECT, F_PROTO);
    s.end();
    s.local_get(P).global_get(G_NULL).ref_eq();
    s.if_(BlockType::Empty);
    as_obj(&mut s, O);
    s.ref_null(object()).struct_set(T_OBJECT, F_PROTO);
    s.end();
    s.end();
    f
}

fn copy_props(cx: &mut dyn Cx) -> Function {
    let own_keys = h(cx, Helper::OwnKeys);
    let own = h(cx, Helper::OwnGet);
    let value = rh(cx, runtime::Helper::Prop(props::Helper::Value));
    let define = h(cx, Helper::Define);
    let number_to_string = rh(cx, runtime::Helper::Num(numstr::Helper::NumberToString));
    let own_symbols = h(cx, Helper::OwnSymbols);
    const T: u32 = 0;
    const SRC: u32 = 1;
    const I: u32 = 2;
    const N: u32 = 3;
    const KEYS: u32 = 4;
    const KK: u32 = 5;
    const SYMS: u32 = 6;
    let mut f = Function::new([
        (2, ValType::I32),
        (1, rt(true, T_KEYS)),
        (1, rt(true, T_STRING)),
        (1, jsval()),
    ]);
    let mut s = f.instructions();
    s.test_object(SRC);
    s.if_(BlockType::Empty);
    {
        as_obj(&mut s, SRC);
        s.call(own_keys).local_tee(KEYS).array_len().local_set(N);
        count_loop(&mut s, I, N, |s| {
            as_obj(s, T);
            s.local_get(KEYS)
                .local_get(I)
                .array_get(T_KEYS)
                .ref_as_non_null()
                .local_tee(KK);
            s.ref_as_non_null();
            as_obj(s, SRC);
            s.local_get(KK).ref_as_non_null().call(own);
            s.local_get(SRC).call(value);
            s.i32_const(0).call(define);
        });
        // Then the enumerable symbol keys (T25).
        as_obj(&mut s, SRC);
        s.i32_const(0).call(own_symbols).local_tee(SYMS);
        s.ref_cast_non_null(array::array())
            .struct_get(T_ARRAY, array::F_LENGTH)
            .local_set(N);
        count_loop(&mut s, I, N, |s| {
            as_obj(s, T);
            s.local_get(SYMS)
                .ref_cast_non_null(array::array())
                .struct_get(T_ARRAY, array::F_ELEMS)
                .local_get(I)
                .array_get(T_VALS)
                .ref_cast_non_null(string())
                .local_tee(KK)
                .ref_as_non_null();
            as_obj(s, SRC);
            s.local_get(KK).ref_as_non_null().call(own);
            s.local_get(SRC).call(value);
            s.i32_const(0).call(define);
        });
        s.return_();
    }
    s.end();
    // A string spreads its characters.
    runtime::is_string(&mut s, SRC);
    s.if_(BlockType::Empty);
    s.local_get(SRC)
        .ref_cast_non_null(string())
        .array_len()
        .local_set(N);
    count_loop(&mut s, I, N, |s| {
        as_obj(s, T);
        s.local_get(I).f64_convert_i32_u().call(number_to_string);
        s.local_get(SRC)
            .ref_cast_non_null(string())
            .local_get(I)
            .array_get_u(T_STRING)
            .array_new_fixed(T_STRING, 1);
        s.i32_const(0).call(define);
    });
    s.end();
    s.end();
    f
}

/// `$own_keys(o)`: live enumerable string keys: array indices ascending (an array's elements
/// first), then the others in insertion order. (Symbol keys are skipped.) With `all`
/// (`$own_names`): the non-enumerable ones too, and an array's `length` after its indices.
fn own_keys(cx: &mut dyn Cx, all: bool) -> Function {
    let index = h(cx, Helper::ArrayIndex);
    let index_keys = cx.helper(runtime::Helper::Arr(array::Helper::IndexKeys));
    const O: u32 = 0;
    const E: u32 = 1;
    const N: u32 = 2;
    const TOTAL: u32 = 3;
    const NI: u32 = 4;
    const J: u32 = 5;
    const M: u32 = 6;
    const P: u32 = 7;
    const A: u32 = 8;
    const KK: u32 = 9;
    const OUT: u32 = 10;
    const IDX: u32 = 11;
    const ELEMS: u32 = 12;
    const ALL: u32 = 13;
    let mut f = Function::new([
        (7, ValType::I32),
        (1, ValType::I64),
        (1, rt(true, T_STRING)),
        (1, rt(true, T_KEYS)),
        (1, rt(true, T_I32S)),
        (2, rt(true, T_KEYS)),
    ]);
    let length = all.then(|| cx.str("length"));
    let mut s = f.instructions();
    // Push "entry E is live and enumerable (or any, with `all`)", setting KK.
    let live = |s: &mut InstructionSink| {
        s.local_get(O)
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(E)
            .array_get(T_KEYS)
            .local_tee(KK)
            .ref_is_null()
            .i32_eqz();
        if !all {
            s.local_get(O)
                .struct_get(T_OBJECT, F_ATTRS)
                .local_get(E)
                .array_get_u(T_BYTES)
                .i32_const(HIDDEN)
                .i32_and()
                .i32_eqz();
            s.i32_and();
        }
        s.local_get(KK)
            .ref_test_non_null(runtime::symbol())
            .i32_eqz();
        s.i32_and();
    };
    s.local_get(O).struct_get(T_OBJECT, F_COUNT).local_set(N);
    count_loop(&mut s, E, N, |s| {
        live(s);
        s.if_(BlockType::Empty);
        s.local_get(TOTAL).i32_const(1).i32_add().local_set(TOTAL);
        s.local_get(KK).ref_as_non_null().call(index);
        s.i64_const(0).i64_ge_s();
        s.if_(BlockType::Empty);
        s.local_get(NI).i32_const(1).i32_add().local_set(NI);
        s.end();
        s.end();
    });
    s.local_get(TOTAL).array_new_default(T_KEYS).local_set(OUT);
    s.local_get(NI).array_new_default(T_I32S).local_set(IDX);
    s.local_get(NI).local_set(M);
    count_loop(&mut s, E, N, |s| {
        live(s);
        s.if_(BlockType::Empty);
        s.local_get(KK).ref_as_non_null().call(index).local_tee(A);
        s.i64_const(0).i64_lt_s();
        s.if_(BlockType::Empty);
        s.local_get(OUT)
            .local_get(M)
            .local_get(KK)
            .array_set(T_KEYS);
        s.local_get(M).i32_const(1).i32_add().local_set(M);
        s.else_();
        // Insertion sort into out[0..J] by index (compared unsigned).
        s.local_get(J).local_set(P);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(P).i32_eqz().br_if(1);
        s.local_get(IDX)
            .local_get(P)
            .i32_const(1)
            .i32_sub()
            .array_get(T_I32S)
            .local_get(A)
            .i32_wrap_i64()
            .i32_le_u()
            .br_if(1);
        s.local_get(IDX).local_get(P);
        s.local_get(IDX)
            .local_get(P)
            .i32_const(1)
            .i32_sub()
            .array_get(T_I32S);
        s.array_set(T_I32S);
        s.local_get(OUT).local_get(P);
        s.local_get(OUT)
            .local_get(P)
            .i32_const(1)
            .i32_sub()
            .array_get(T_KEYS);
        s.array_set(T_KEYS);
        s.local_get(P).i32_const(1).i32_sub().local_set(P);
        s.br(0).end().end();
        s.local_get(IDX)
            .local_get(P)
            .local_get(A)
            .i32_wrap_i64()
            .array_set(T_I32S);
        s.local_get(OUT)
            .local_get(P)
            .local_get(KK)
            .array_set(T_KEYS);
        s.local_get(J).i32_const(1).i32_add().local_set(J);
        s.end();
        s.end();
    });
    if cx.names("$typed") {
        // A typed array (T20): its indices, "0" to "length - 1", come first.
        let length = crate::typed::Length::new(cx);
        let number_to_string = rh(
            cx,
            runtime::Helper::Num(crate::numstr::Helper::NumberToString),
        );
        s.local_get(O).ref_test_non_null(crate::typed::typed());
        s.if_(BlockType::Empty);
        s.local_get(O).ref_cast_non_null(crate::typed::typed());
        length.emit(&mut s);
        s.local_tee(M)
            .array_new_default(T_KEYS)
            .local_set(ELEMS);
        s.i32_const(0).local_set(P);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(P).local_get(M).i32_ge_u().br_if(1);
        s.local_get(ELEMS).ref_as_non_null().local_get(P);
        s.local_get(P).f64_convert_i32_u().call(number_to_string);
        s.array_set(T_KEYS);
        s.local_get(P).i32_const(1).i32_add().local_set(P);
        s.br(0).end().end();
        s.local_get(M)
            .local_get(OUT)
            .ref_as_non_null()
            .array_len()
            .i32_add()
            .array_new_default(T_KEYS)
            .local_set(ALL);
        s.local_get(ALL).ref_as_non_null().i32_const(0);
        s.local_get(ELEMS)
            .ref_as_non_null()
            .i32_const(0)
            .local_get(M);
        s.array_copy(T_KEYS, T_KEYS);
        s.local_get(ALL).ref_as_non_null().local_get(M);
        s.local_get(OUT).ref_as_non_null().i32_const(0);
        s.local_get(OUT).ref_as_non_null().array_len();
        s.array_copy(T_KEYS, T_KEYS);
        s.local_get(ALL).ref_as_non_null().return_();
        s.end();
    }
    // An array: its elements' indices come first (then `length`, with `all`).
    let extra = all as i32;
    s.local_get(O).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    s.local_get(O)
        .ref_cast_non_null(array::array())
        .i32_const(all as i32)
        .call(index_keys)
        .local_tee(ELEMS)
        .array_len()
        .local_get(OUT)
        .array_len()
        .i32_add()
        .i32_const(extra)
        .i32_add()
        .array_new_default(T_KEYS)
        .local_set(ALL);
    s.local_get(ALL).ref_as_non_null().i32_const(0);
    s.local_get(ELEMS).ref_as_non_null().i32_const(0);
    s.local_get(ELEMS).array_len();
    s.array_copy(T_KEYS, T_KEYS);
    if let Some(length) = length {
        s.local_get(ALL).ref_as_non_null().local_get(ELEMS).array_len();
        s.global_get(length).array_set(T_KEYS);
    }
    s.local_get(ALL)
        .ref_as_non_null()
        .local_get(ELEMS)
        .array_len()
        .i32_const(extra)
        .i32_add();
    s.local_get(OUT).ref_as_non_null().i32_const(0);
    s.local_get(OUT).array_len();
    s.array_copy(T_KEYS, T_KEYS);
    s.local_get(ALL).ref_as_non_null().return_();
    s.end();
    s.local_get(OUT).ref_as_non_null();
    s.end();
    f
}

/// `$for_in_keys(v)`: own enumerable keys, then each prototype's that no object before it on
/// the chain has (enumerable or not). A string's indices. Nothing for other primitives.
fn for_in_keys(cx: &mut dyn Cx) -> Function {
    let own_keys = h(cx, Helper::OwnKeys);
    let own = h(cx, Helper::OwnGet);
    let number_to_string = rh(cx, runtime::Helper::Num(numstr::Helper::NumberToString));
    const V: u32 = 0;
    const I: u32 = 1;
    const N: u32 = 2;
    const LEN: u32 = 3;
    const O: u32 = 4;
    const Q: u32 = 5;
    const KS: u32 = 6;
    const OUT: u32 = 7;
    const KK: u32 = 8;
    const SEEN: u32 = 9;
    let mut f = Function::new([
        (3, ValType::I32),
        (2, rt(true, T_OBJECT)),
        (2, rt(true, T_KEYS)),
        (1, rt(true, T_STRING)),
        (1, ValType::I32),
    ]);
    let mut s = f.instructions();
    runtime::is_string(&mut s, V);
    s.if_(BlockType::Empty);
    {
        s.local_get(V)
            .ref_cast_non_null(string())
            .array_len()
            .local_tee(N)
            .array_new_default(T_KEYS)
            .local_set(OUT);
        count_loop(&mut s, I, N, |s| {
            s.local_get(OUT).local_get(I);
            s.local_get(I).f64_convert_i32_u().call(number_to_string);
            s.array_set(T_KEYS);
        });
        s.local_get(OUT).return_();
    }
    s.end();
    s.test_object(V).i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(0).array_new_default(T_KEYS).return_();
    s.end();
    s.i32_const(8).array_new_default(T_KEYS).local_set(OUT);
    s.local_get(V).ref_cast_non_null(object()).local_set(O);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(O).ref_is_null().br_if(1);
    s.local_get(O)
        .ref_as_non_null()
        .call(own_keys)
        .local_tee(KS)
        .array_len()
        .local_set(N);
    count_loop(&mut s, I, N, |s| {
        s.local_get(KS).local_get(I).array_get(T_KEYS).local_set(KK);
        // Shadowed by an object before O on the chain?
        s.i32_const(0).local_set(SEEN);
        s.local_get(V).ref_cast_non_null(object()).local_set(Q);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(Q).local_get(O).ref_eq().br_if(1);
        s.local_get(Q)
            .ref_as_non_null()
            .local_get(KK)
            .ref_as_non_null()
            .call(own)
            .ref_is_null()
            .i32_eqz();
        s.if_(BlockType::Empty);
        s.i32_const(1).local_set(SEEN);
        s.br(2);
        s.end();
        s.local_get(Q)
            .ref_as_non_null()
            .struct_get(T_OBJECT, F_PROTO)
            .local_set(Q);
        s.br(0).end().end();
        s.local_get(SEEN).i32_eqz();
        s.if_(BlockType::Empty);
        // Append, doubling OUT when full.
        s.local_get(LEN).local_get(OUT).array_len().i32_eq();
        s.if_(BlockType::Empty);
        s.local_get(LEN)
            .i32_const(1)
            .i32_shl()
            .array_new_default(T_KEYS)
            .local_tee(KS);
        s.drop();
        s.local_get(KS)
            .i32_const(0)
            .local_get(OUT)
            .i32_const(0)
            .local_get(LEN);
        s.array_copy(T_KEYS, T_KEYS);
        s.local_get(KS).local_set(OUT);
        // KS was the key list being walked: reload it.
        s.local_get(O)
            .ref_as_non_null()
            .call(own_keys)
            .local_set(KS);
        s.end();
        s.local_get(OUT)
            .local_get(LEN)
            .local_get(KK)
            .array_set(T_KEYS);
        s.local_get(LEN).i32_const(1).i32_add().local_set(LEN);
        s.end();
    });
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_PROTO)
        .local_set(O);
    s.br(0).end().end();
    // Exactly LEN keys.
    s.local_get(LEN).array_new_default(T_KEYS).local_tee(KS);
    s.i32_const(0).local_get(OUT).i32_const(0).local_get(LEN);
    s.array_copy(T_KEYS, T_KEYS);
    s.local_get(KS);
    s.end();
    f
}

fn create(cx: &mut dyn Cx) -> Function {
    let alloc = h(cx, Helper::Alloc);
    let to_string = rh(cx, runtime::Helper::ToString);
    let concat = rh(cx, runtime::Helper::Concat);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let msg = cx.str("Object prototype may only be an Object or null: ");
    const P: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.test_object(P);
    s.if_(BlockType::Empty);
    s.i32_const(CLASS_PLAIN);
    as_obj(&mut s, P);
    s.call(alloc).return_();
    s.end();
    s.local_get(P).global_get(G_NULL).ref_eq();
    s.if_(BlockType::Empty);
    s.i32_const(CLASS_PLAIN)
        .ref_null(object())
        .call(alloc)
        .return_();
    s.end();
    s.global_get(msg);
    s.local_get(P).call(to_string).call(concat);
    s.call(throw);
    s.unreachable();
    s.end();
    f
}

/// `$regexp_new(data)` (T27), see [`Helper::RegExpNew`].
fn regexp_new(cx: &mut dyn Cx) -> Function {
    let alloc = h(cx, Helper::Alloc);
    let proto = h(cx, Helper::Proto(Builtin::RegExp));
    let define = h(cx, Helper::Define);
    let last_index = cx.str("lastIndex");
    const DATA: u32 = 0;
    const O: u32 = 1;
    let mut f = Function::new([(1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.i32_const(CLASS_REGEXP).call(proto).call(alloc).local_tee(O).ref_as_non_null();
    s.global_get(last_index).i32_const(0).ref_i31();
    s.i32_const(HIDDEN | FIXED).call(define);
    s.local_get(O).ref_as_non_null().global_get(runtime::G_SYM_STATE);
    s.local_get(DATA);
    s.i32_const(HIDDEN | READONLY | FIXED | PRIVATE).call(define);
    s.local_get(O);
    s.end();
    f
}

/// `$regexp_data(v)` (T27), see [`Helper::RegExpData`].
fn regexp_data(cx: &mut dyn Cx) -> Function {
    let own = h(cx, Helper::OwnGet);
    const V: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.test_object(V);
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.struct_get(T_OBJECT, F_CLASS)
        .i32_const(CLASS_MASK)
        .i32_and()
        .i32_const(CLASS_REGEXP)
        .i32_eq();
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.global_get(runtime::G_SYM_STATE).call(own).return_();
    s.end();
    s.local_get(V)
        .global_get(G_PROTOS + Builtin::RegExp.index())
        .ref_eq();
    s.if_(BlockType::Empty);
    s.global_get(G_NULL).return_();
    s.end();
    s.end();
    s.global_get(G_UNDEFINED);
    s.end();
    f
}

/// `$date_new(data)` (T26), see [`Helper::DateNew`]; T45: also a stack's (`class`, `b`).
fn state_new(cx: &mut dyn Cx, class: i32, b: Builtin) -> Function {
    let alloc = h(cx, Helper::Alloc);
    let proto = h(cx, Helper::Proto(b));
    let define = h(cx, Helper::Define);
    const DATA: u32 = 0;
    const O: u32 = 1;
    let mut f = Function::new([(1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.i32_const(class).call(proto).call(alloc).local_tee(O).ref_as_non_null();
    s.global_get(runtime::G_SYM_STATE).local_get(DATA);
    s.i32_const(HIDDEN | READONLY | FIXED | PRIVATE).call(define);
    s.local_get(O);
    s.end();
    f
}

/// `$module_new()` (T41), see [`Helper::ModuleNew`].
fn module_new(cx: &mut dyn Cx) -> Function {
    let alloc = h(cx, Helper::Alloc);
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.i32_const(CLASS_MODULE).ref_null(object()).call(alloc);
    s.end();
    f
}

/// `$date_data(v)` (T26), see [`Helper::DateData`]; T45: also a stack's (`class`).
fn state_data(cx: &mut dyn Cx, class: i32) -> Function {
    let own = h(cx, Helper::OwnGet);
    const V: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.test_object(V);
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.struct_get(T_OBJECT, F_CLASS).i32_const(CLASS_MASK).i32_and();
    if class == CLASS_ASYNC_STACK {
        s.i32_const(1).i32_or();
    }
    s.i32_const(class).i32_eq();
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.global_get(runtime::G_SYM_STATE).call(own).return_();
    s.end();
    s.end();
    s.global_get(G_UNDEFINED);
    s.end();
    f
}

/// `$to_primitive_value(v, hint)` (T26), see [`Helper::ToPrimitiveValue`].
fn to_primitive_value(cx: &mut dyn Cx) -> Function {
    let to_primitive = h(cx, Helper::ToPrimitive);
    const V: u32 = 0;
    const HINT: u32 = 1;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.test_object(V);
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.local_get(HINT).call(to_primitive).return_();
    s.end();
    s.local_get(V);
    s.end();
    f
}

fn get_prototype_of(cx: &mut dyn Cx) -> Function {
    let is_nullish = rh(cx, runtime::Helper::IsNullish);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let convert = cx.str("Cannot convert undefined or null to object");
    const V: u32 = 0;
    const P: u32 = 1;
    let mut f = Function::new([(1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.global_get(convert).call(throw);
    s.end();
    s.test_object(V);
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.struct_get(T_OBJECT, F_PROTO).local_tee(P).ref_is_null();
    s.if_(BlockType::Empty);
    s.global_get(G_NULL).return_();
    s.end();
    s.local_get(P).return_();
    s.end();
    primitive_proto(cx, &mut s, V);
    s.end();
    f
}

fn instance_of(cx: &mut dyn Cx, b: Builtin) -> Function {
    let proto = h(cx, Helper::Proto(b));
    const V: u32 = 0;
    const O: u32 = 1;
    const P: u32 = 2;
    let mut f = Function::new([(2, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.test_object(V).i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.call(proto).local_set(P);
    as_obj(&mut s, V);
    s.struct_get(T_OBJECT, F_PROTO).local_set(O);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(O).ref_is_null().br_if(1);
    s.local_get(O).local_get(P).ref_eq();
    s.if_(BlockType::Empty);
    s.i32_const(1).return_();
    s.end();
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_PROTO)
        .local_set(O);
    s.br(0).end().end();
    s.i32_const(0);
    s.end();
    f
}

/// `$new_error(message, options)`: own `stack`, `message` unless it's `undefined`, and `cause` if
/// `options` is an object that has one. T63 (F261): `stack` is made on its first read, as V8 does
/// ([`error_stack`]); defined first, so it stays the first own key.
/// `AggregateError` (T25): `(errors, message, options)`, then an own `errors`: the iterable
/// `errors` as a new array.
/// Pushes whether local `stk` (an error `v`'s own `stack`, any value) is a `stack` without frames:
/// a string equal to the error's header (`Promise.any`'s error, T25). Since T63 every other error's
/// `stack` can be a string too (when the code names `stack`), with a frame.
pub fn frameless_stack(cx: &mut dyn Cx, s: &mut InstructionSink, v: u32, stk: u32) {
    if !cx.names("stack") {
        // Only the prelude's `Promise.any` error has a string `stack`.
        runtime::is_string(s, stk);
        return;
    }
    let to_string = rh(cx, runtime::Helper::ToString);
    let str_eq = rh(cx, runtime::Helper::StrEq);
    runtime::is_string(s, stk);
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(stk).ref_cast_non_null(runtime::string());
    s.local_get(v).call(to_string).call(str_eq);
    s.else_();
    s.i32_const(0);
    s.end();
}

/// `$error_stack(receiver)`: V8's `stack` on its first read: the header (`Error.prototype.toString`
/// of the receiver then: a `name` set after construction counts) and one frame, `    at <anonymous>`
/// (no source positions at run time). It replaces the lazy slot as a writable, hidden data property.
fn error_stack(cx: &mut dyn Cx) -> Function {
    let define = h(cx, Helper::Define);
    let to_string = rh(cx, runtime::Helper::ToString);
    let concat = rh(cx, runtime::Helper::Concat);
    let (stack, frame) = (cx.str("stack"), cx.str(STACK_FRAME));
    const R: u32 = 0;
    const S: u32 = 1;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.local_get(R).call(to_string).global_get(frame).call(concat).local_set(S);
    s.test_object(R);
    s.if_(BlockType::Empty);
    as_obj(&mut s, R);
    s.global_get(stack).local_get(S).i32_const(HIDDEN).call(define);
    s.end();
    s.local_get(S);
    s.end();
    f
}

/// The one frame of an error's `stack` (T63, [`error_stack`]).
pub const STACK_FRAME: &str = "\n    at <anonymous>";

fn new_error(cx: &mut dyn Cx, k: ErrorKind) -> Function {
    let alloc = h(cx, Helper::Alloc);
    let proto = h(cx, Helper::Proto(Builtin::Error(k)));
    let define = h(cx, Helper::Define);
    let lookup = h(cx, Helper::Lookup);
    let get = h(cx, Helper::Get);
    let to_string = rh(cx, runtime::Helper::ToString);
    let (stack, message, cause) = (cx.str("stack"), cx.str("message"), cx.str("cause"));
    let aggregate = k == ErrorKind::AggregateError;
    // T45: `(error, suppressed, message)`, no options.
    let suppressed = k == ErrorKind::SuppressedError;
    const ERRS: u32 = 0;
    let first = aggregate as u32;
    let (m, opt, o) = if suppressed { (2, 2, 3) } else { (first, first + 1, first + 2) };
    let mut f = Function::new([(1, rt(true, T_OBJECT)), (2, jsval())]);
    let mut s = f.instructions();
    s.i32_const(CLASS_ERROR)
        .call(proto)
        .call(alloc)
        .local_set(o);
    s.local_get(o).ref_as_non_null().global_get(stack);
    if cx.names("stack") {
        // Made on its first read (T63): an accessor slot whose getter and setter are the
        // built-in sentinel, which `property_value`, the setters and `getOwnPropertyDescriptor`
        // recognize. Programs that never name `stack` keep the sentinel (reading it traps).
        s.global_get(G_BUILTIN)
            .global_get(G_BUILTIN)
            .struct_new(runtime::T_ACCESSOR);
        s.i32_const(HIDDEN | ACCESSOR).call(define);
    } else {
        s.global_get(G_BUILTIN);
        s.i32_const(HIDDEN).call(define);
    }
    s.local_get(m).global_get(G_UNDEFINED).ref_eq().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(o).ref_as_non_null().global_get(message);
    s.local_get(m).call(to_string);
    s.i32_const(HIDDEN).call(define);
    s.end();
    if suppressed {
        for (i, key) in [(0, "error"), (1, "suppressed")] {
            let key = cx.str(key);
            s.local_get(o).ref_as_non_null().global_get(key);
            s.local_get(i).i32_const(HIDDEN).call(define);
        }
        s.local_get(o).ref_as_non_null();
        s.end();
        return f;
    }
    s.test_object(opt);
    s.if_(BlockType::Empty);
    as_obj(&mut s, opt);
    s.global_get(cause).call(lookup).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(o).ref_as_non_null().global_get(cause);
    s.local_get(opt).global_get(cause).call(get);
    s.i32_const(HIDDEN).call(define);
    s.end();
    s.end();
    if aggregate {
        let not_iterable = cx.helper(runtime::Helper::Prom(crate::promise::Helper::NotIterable));
        let open = rh(cx, runtime::Helper::Iter(crate::iter::Helper::Open));
        let step = rh(cx, runtime::Helper::Iter(crate::iter::Helper::Step));
        let new_array = rh(cx, runtime::Helper::Arr(crate::array::Helper::New));
        let push = rh(cx, runtime::Helper::Arr(crate::array::Helper::Push));
        let errors = cx.str("errors");
        let (it, list) = (o + 1, o + 2);
        s.local_get(ERRS).local_get(ERRS).call(not_iterable);
        s.call(open).local_set(it);
        s.i32_const(0).array_new_default(T_VALS).i32_const(0);
        s.call(new_array).local_set(list);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(it).call(step).i32_eqz().br_if(1);
        s.local_get(list).ref_cast_non_null(crate::array::array());
        s.local_get(it)
            .ref_cast_non_null(crate::iter::iter())
            .struct_get(runtime::T_ITER, crate::iter::F_VALUE);
        s.call(push);
        s.br(0).end().end();
        s.local_get(o).ref_as_non_null().global_get(errors);
        s.local_get(list).i32_const(HIDDEN).call(define);
    }
    s.local_get(o).ref_as_non_null();
    s.end();
    f
}

/// `$to_primitive(o, hint)`: OrdinaryToPrimitive (hint 0 default, 1 number, 2 string): `valueOf`
/// then `toString` (`toString` first for hint string), each called if it's a function, the first
/// primitive result wins (T16: user methods). Built-in sentinels: `Object.prototype.valueOf`
/// returns the object (skipped), `Object.prototype.toString` gives `[object Tag]`; any other
/// built-in traps.
fn to_primitive(cx: &mut dyn Cx) -> Function {
    let holder = h(cx, Helper::Holder);
    let own = h(cx, Helper::OwnGet);
    let obj_proto = h(cx, Helper::Proto(Builtin::Object));
    let user = h(cx, Helper::UserPrimitiveTrap);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let value = rh(cx, runtime::Helper::Prop(props::Helper::Value));
    let call_value = rh(cx, runtime::Helper::Func(func::Helper::CallValue));
    let value_of = cx.str("valueOf");
    let to_string_key = cx.str("toString");
    let tags = [
        cx.str("[object Object]"),
        cx.str("[object Error]"),
        cx.str("[object Array]"),
        cx.str("[object Function]"),
    ];
    let no_prim = cx.str("Cannot convert object to primitive value");
    let is_number = rh(cx, runtime::Helper::IsNumber);
    let to_string = rh(cx, runtime::Helper::ToString);
    let builtin_tag = cx.has_tags().then(|| {
        (
            h(cx, Helper::BuiltinTag),
            rh(cx, runtime::Helper::Concat),
            cx.str("[object "),
            cx.str("]"),
        )
    });
    const O: u32 = 0;
    const HINT: u32 = 1;
    const HD: u32 = 2;
    const T: u32 = 3;
    const R: u32 = 4;
    const I: u32 = 5;
    const KEY: u32 = 6;
    let mut f = Function::new([
        (1, rt(true, T_OBJECT)),
        (2, jsval()),
        (1, ValType::I32),
        (1, rt(true, T_STRING)),
    ]);
    let mut s = f.instructions();
    if cx.names("toPrimitive") || cx.names("$timers") {
        // `o[Symbol.toPrimitive](hint)` first (T25; only when the code names it, or has timers:
        // `+timeout` is its id).
        let get = h(cx, Helper::Get);
        let is_nullish = rh(cx, runtime::Helper::IsNullish);
        let sym = rh(
            cx,
            runtime::Helper::WellKnownSymbol(
                js2wasi3_ir::well_known_symbol("toPrimitive").expect("well-known"),
            ),
        );
        let typeof_string = rh(cx, runtime::Helper::TypeofString);
        let to_string = rh(cx, runtime::Helper::ToString);
        let concat = rh(cx, runtime::Helper::Concat);
        let (space, not_fn) = (cx.str(" "), cx.str(" is not a function"));
        let hints = [cx.str("default"), cx.str("number"), cx.str("string")];
        s.local_get(O).call(sym).call(get).local_tee(T);
        s.call(is_nullish).i32_eqz();
        s.if_(BlockType::Empty);
        {
            s.local_get(T).ref_test_non_null(func::function()).i32_eqz();
            s.if_(BlockType::Empty);
            s.local_get(T).call(typeof_string);
            s.global_get(space).call(concat);
            s.local_get(T).call(to_string).call(concat);
            s.global_get(not_fn).call(concat).call(throw);
            s.end();
            s.local_get(T).local_get(O);
            // (`select` picks its first operand when the condition holds.)
            s.global_get(hints[2]);
            s.global_get(hints[1]);
            s.global_get(hints[0]);
            s.local_get(HINT).i32_const(1).i32_eq();
            s.typed_select(string_ref());
            s.local_get(HINT).i32_const(2).i32_eq();
            s.typed_select(string_ref());
            s.array_new_fixed(T_VALS, 1);
            s.global_get(to_primitive_key(cx));
            s.call(call_value).local_tee(R);
            s.ref_test_non_null(object());
            s.if_(BlockType::Empty);
            s.global_get(no_prim).call(throw);
            s.end();
            s.local_get(R).return_();
        }
        s.end();
    }
    // A Symbol wrapper: `Symbol.prototype[Symbol.toPrimitive]` gives its symbol (T30).
    s.local_get(O).ref_test_non_null(wrapper::wrapper());
    s.if_(BlockType::Empty);
    s.local_get(O)
        .ref_cast_non_null(wrapper::wrapper())
        .struct_get(runtime::T_WRAPPER, wrapper::F_PRIM)
        .local_tee(T);
    s.ref_test_non_null(runtime::symbol());
    s.if_(BlockType::Empty);
    s.local_get(T).return_();
    s.end();
    s.end();
    if cx.names("$date") {
        // A Date's hint default is string (T26: `Date.prototype[Symbol.toPrimitive]`).
        s.local_get(HINT).i32_eqz();
        s.local_get(O)
            .struct_get(T_OBJECT, F_CLASS)
            .i32_const(CLASS_MASK)
            .i32_and()
            .i32_const(CLASS_DATE)
            .i32_eq()
            .i32_and();
        s.if_(BlockType::Empty);
        s.i32_const(2).local_set(HINT);
        s.end();
    }
    // Two rounds: I = 0, 1. The key of round I: toString when (I == 0) == (hint == string).
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).i32_const(2).i32_ge_u().br_if(1);
    s.global_get(to_string_key).global_get(value_of);
    s.local_get(I).i32_eqz();
    s.local_get(HINT).i32_const(2).i32_eq();
    s.i32_eq();
    s.typed_select(string_ref()).local_set(KEY);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.local_get(O)
        .local_get(KEY)
        .ref_as_non_null()
        .call(holder)
        .local_tee(HD);
    s.ref_is_null().br_if(0);
    s.local_get(HD)
        .ref_as_non_null()
        .local_get(KEY)
        .ref_as_non_null()
        .call(own)
        .local_tee(R);
    s.global_get(G_BUILTIN).ref_eq();
    s.if_(BlockType::Empty);
    {
        // T30: the built-in `valueOf`/`toString` of a wrapper's own kind of prototype give its
        // primitive (a prototype's global is set once a wrapper of its kind exists).
        s.local_get(O).ref_test_non_null(wrapper::wrapper());
        s.if_(BlockType::Empty);
        {
            s.local_get(O)
                .ref_cast_non_null(wrapper::wrapper())
                .struct_get(runtime::T_WRAPPER, wrapper::F_PRIM)
                .local_set(T);
            s.local_get(HD);
            s.global_get(G_PROTOS + Builtin::String.index());
            s.global_get(G_PROTOS + Builtin::Number.index());
            s.global_get(G_PROTOS + Builtin::Boolean.index());
            s.local_get(T).call(is_number);
            s.typed_select(rt(true, T_OBJECT));
            runtime::is_string(&mut s, T);
            s.typed_select(rt(true, T_OBJECT));
            s.ref_eq();
            s.if_(BlockType::Empty);
            s.local_get(T);
            s.local_get(T).call(to_string);
            s.local_get(KEY).global_get(value_of).ref_eq();
            s.typed_select(jsval()).return_();
            s.end();
        }
        s.end();
        s.local_get(KEY).global_get(value_of).ref_eq();
        s.if_(BlockType::Empty);
        // Object.prototype.valueOf returns the object: try the other method.
        s.local_get(HD).call(obj_proto).ref_eq().br_if(2);
        s.call(user);
        s.end();
        s.local_get(HD).call(obj_proto).ref_eq();
        s.if_(BlockType::Empty);
        // Object.prototype.toString: `[object Tag]`, the built-in tag first (T25; only if the
        // program can make a tagged object).
        if let Some((builtin_tag, concat, object_open, close_bracket)) = builtin_tag {
            s.local_get(O).call(builtin_tag).local_tee(T);
            runtime::is_string(&mut s, T);
            s.if_(BlockType::Empty);
            s.global_get(object_open);
            s.local_get(T).ref_cast_non_null(string()).call(concat);
            s.global_get(close_bracket).call(concat).return_();
            s.end();
        }
        // T30: a wrapper's class.
        s.local_get(O).ref_test_non_null(wrapper::wrapper());
        s.if_(BlockType::Empty);
        {
            let wrapper_tag = rh(cx, runtime::Helper::Wrap(wrapper::Helper::Tag));
            let concat = rh(cx, runtime::Helper::Concat);
            let (open, close) = (cx.str("[object "), cx.str("]"));
            s.global_get(open);
            s.local_get(O).call(wrapper_tag).ref_cast_non_null(string());
            s.call(concat);
            s.global_get(close).call(concat).return_();
        }
        s.end();
        s.local_get(O).ref_test_non_null(func::function());
        s.if_(BlockType::Empty);
        s.global_get(tags[3]).return_();
        s.end();
        s.local_get(O).ref_test_non_null(array::array());
        s.if_(BlockType::Empty);
        s.global_get(tags[2]).return_();
        s.end();
        s.local_get(O)
            .struct_get(T_OBJECT, F_CLASS)
            .i32_const(CLASS_MASK)
            .i32_and()
            .i32_const(CLASS_ERROR)
            .i32_eq();
        s.if_(BlockType::Empty);
        s.global_get(tags[1]).return_();
        s.end();
        s.global_get(tags[0]).return_();
        s.end();
        s.call(user);
    }
    s.end();
    // A user method (or an accessor's value).
    s.local_get(R).local_get(O).call(value).local_tee(T);
    s.ref_test_non_null(func::function());
    s.if_(BlockType::Empty);
    s.local_get(T).local_get(O);
    s.i32_const(0).array_new_default(T_VALS);
    s.local_get(KEY).ref_as_non_null();
    s.call(call_value).local_tee(T);
    s.ref_test_non_null(object()).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(T).return_();
    s.end();
    s.end();
    s.br(0).end().end();
    s.global_get(no_prim).call(throw);
    s.unreachable();
    s.end();
    f
}

/// `Error.prototype.toString` (a `$code`): `name: message`, `name` defaulting to "Error" and
/// `message` to "" (either alone when the other is empty).
fn error_to_string(cx: &mut dyn Cx) -> Function {
    let get = h(cx, Helper::Get);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let to_string = rh(cx, runtime::Helper::ToString);
    let concat = rh(cx, runtime::Helper::Concat);
    let (name, message) = (cx.str("name"), cx.str("message"));
    let (error, empty, colon) = (cx.str("Error"), cx.str(""), cx.str(": "));
    let not_obj = cx.str("Error.prototype.toString requires that 'this' be an Object");
    const THIS: u32 = 1;
    const N: u32 = 3;
    const M: u32 = 4;
    const T: u32 = 5;
    let mut f = Function::new([(2, rt(true, T_STRING)), (1, jsval())]);
    let mut s = f.instructions();
    s.test_object(THIS).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(not_obj).call(throw);
    s.end();
    for (key, local, default) in [(name, N, error), (message, M, empty)] {
        s.local_get(THIS).global_get(key).call(get).local_tee(T);
        s.global_get(G_UNDEFINED).ref_eq();
        s.if_(BlockType::Result(string_ref()));
        s.global_get(default);
        s.else_();
        s.local_get(T).call(to_string);
        s.end();
        s.local_set(local);
    }
    s.local_get(N).ref_as_non_null().array_len().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(M).ref_as_non_null().return_();
    s.end();
    s.local_get(M).ref_as_non_null().array_len().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(N).ref_as_non_null().return_();
    s.end();
    s.local_get(N)
        .ref_as_non_null()
        .global_get(colon)
        .call(concat);
    s.local_get(M).ref_as_non_null().call(concat);
    s.end();
    f
}

/// `$object_to_number(o)`: ToNumber of ToPrimitive(o, number). A string result: `""` is 0; one
/// starting with `[` or a letter other than `I` can't be numeric: NaN (no parsing needed); other
/// strings go through `$to_number` (the prelude's StringToNumber, T21).
fn object_to_number(cx: &mut dyn Cx) -> Function {
    let to_primitive = h(cx, Helper::ToPrimitive);
    let to_number = rh(cx, runtime::Helper::ToNumber);
    const O: u32 = 0;
    const P: u32 = 1;
    const C: u32 = 2;
    let mut f = Function::new([(1, jsval()), (1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(O).i32_const(1).call(to_primitive).local_set(P);
    runtime::is_string(&mut s, P);
    s.i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(P).call(to_number).return_();
    s.end();
    s.local_get(P)
        .ref_cast_non_null(string())
        .array_len()
        .i32_eqz();
    s.if_(BlockType::Empty);
    s.f64_const(0.0.into()).return_();
    s.end();
    s.local_get(P)
        .ref_cast_non_null(string())
        .i32_const(0)
        .array_get_u(T_STRING)
        .local_set(C);
    s.local_get(C).i32_const('[' as i32).i32_eq();
    // ASCII letter: (c | 0x20) - 'a' < 26
    s.local_get(C)
        .i32_const(0x20)
        .i32_or()
        .i32_const('a' as i32)
        .i32_sub()
        .i32_const(26)
        .i32_lt_u();
    s.local_get(C).i32_const('I' as i32).i32_ne().i32_and();
    s.i32_or();
    s.if_(BlockType::Empty);
    s.f64_const(f64::NAN.into()).return_();
    s.end();
    s.local_get(P).call(to_number);
    s.end();
    f
}

fn throw(cx: &mut dyn Cx, k: ErrorKind) -> Function {
    let new_error = h(cx, Helper::NewError(k));
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).global_get(G_UNDEFINED).call(new_error);
    s.throw(TAG);
    s.end();
    f
}

/// `{...rest}`: copy the own enumerable properties, then delete the excluded keys.
fn object_rest(cx: &mut dyn Cx) -> Function {
    let new_object = h(cx, Helper::NewObject);
    let copy = h(cx, Helper::CopyProps);
    let remove = h(cx, Helper::Remove);
    let to_key = h(cx, Helper::ToKey);
    const SRC: u32 = 0;
    const EX: u32 = 1;
    const O: u32 = 2;
    const I: u32 = 3;
    let mut f = Function::new([(1, jsval()), (1, ValType::I32)]);
    let mut s = f.instructions();
    s.call(new_object).local_set(O);
    s.local_get(O).local_get(SRC).call(copy);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I);
    s.local_get(EX)
        .ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, array::F_LENGTH);
    s.i32_ge_u().br_if(1);
    as_obj(&mut s, O);
    s.local_get(EX)
        .ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, array::F_ELEMS)
        .local_get(I)
        .array_get(T_VALS)
        .call(to_key)
        .call(remove);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(O);
    s.end();
    f
}

fn check_destructure(cx: &mut dyn Cx) -> Function {
    let concat = rh(cx, runtime::Helper::Concat);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let (undefined, null) = (cx.str("undefined."), cx.str("null."));
    let mut f = Function::new([]);
    let mut s = f.instructions();
    for (g, text) in [(G_UNDEFINED, undefined), (G_NULL, null)] {
        s.local_get(0).global_get(g).ref_eq();
        s.if_(BlockType::Empty);
        s.local_get(1).global_get(text).call(concat).call(throw);
        s.end();
    }
    s.end();
    f
}

/// `Object.keys` / `values` / `entries`: own enumerable string keys (a string's indices).
fn keys(cx: &mut dyn Cx, which: u8) -> Function {
    let is_nullish = rh(cx, runtime::Helper::IsNullish);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    // 3: `Object.getOwnPropertyNames` (T28).
    let own_keys = h(cx, if which == 3 { Helper::OwnNames } else { Helper::OwnKeys });
    let for_in_keys = h(cx, Helper::ForInKeys);
    let get = h(cx, Helper::Get);
    let new = cx.helper(runtime::Helper::Arr(array::Helper::New));
    let convert = cx.str("Cannot convert undefined or null to object");
    const V: u32 = 0;
    const KS: u32 = 1;
    const N: u32 = 2;
    const I: u32 = 3;
    const OUT: u32 = 4;
    const K: u32 = 5;
    let mut f = Function::new([
        (1, rt(true, T_KEYS)),
        (2, ValType::I32),
        (1, rt(true, T_VALS)),
        (1, jsval()),
    ]);
    let mut s = f.instructions();
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.global_get(convert).call(throw);
    s.end();
    s.test_object(V);
    s.if_(BlockType::Result(rt(false, T_KEYS)));
    as_obj(&mut s, V);
    s.call(own_keys);
    s.else_();
    // A string's indices; nothing for other primitives.
    s.local_get(V)
        .call(for_in_keys)
        .ref_cast_non_null(wasm_encoder::HeapType::Concrete(T_KEYS));
    s.end();
    s.local_tee(KS).array_len().local_tee(N);
    // `Object.getOwnPropertyNames` of a string: its indices, then `length`.
    let length = (which == 3).then(|| cx.str("length"));
    if length.is_some() {
        s.test_string(V).i32_add();
    }
    s.array_new_default(T_VALS).local_set(OUT);
    count_loop(&mut s, I, N, |s| {
        s.local_get(KS).local_get(I).array_get(T_KEYS).local_set(K);
        s.local_get(OUT).local_get(I);
        match which {
            0 | 3 => {
                s.local_get(K);
            }
            1 => {
                s.local_get(V).local_get(K).call(get);
            }
            _ => {
                s.local_get(K);
                s.local_get(V).local_get(K).call(get);
                s.array_new_fixed(T_VALS, 2).i32_const(2).call(new);
            }
        }
        s.array_set(T_VALS);
    });
    if let Some(length) = length {
        s.test_string(V);
        s.if_(BlockType::Empty);
        s.local_get(OUT).local_get(N).global_get(length).array_set(T_VALS);
        s.local_get(N).i32_const(1).i32_add().local_set(N);
        s.end();
    }
    s.local_get(OUT).ref_as_non_null().local_get(N).call(new);
    s.end();
    f
}

fn symbol_keys() -> Function {
    const O: u32 = 0;
    const E: u32 = 1;
    const N: u32 = 2;
    const C: u32 = 3;
    let mut f = Function::new([(3, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(O).struct_get(T_OBJECT, F_COUNT).local_set(N);
    count_loop(&mut s, E, N, |s| {
        s.local_get(O)
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(E)
            .array_get(T_KEYS)
            .ref_test_non_null(runtime::symbol());
        s.local_get(O)
            .struct_get(T_OBJECT, F_ATTRS)
            .local_get(E)
            .array_get_u(T_BYTES)
            .i32_const(HIDDEN | PRIVATE)
            .i32_and()
            .i32_eqz();
        s.i32_and();
        s.local_get(C).i32_add().local_set(C);
    });
    s.local_get(C);
    s.end();
    f
}

/// `$make_arguments(args)` (T25).
fn make_arguments(cx: &mut dyn Cx) -> Function {
    let alloc = h(cx, Helper::Alloc);
    let proto = h(cx, Helper::Proto(Builtin::Object));
    let array_proto = h(cx, Helper::Proto(Builtin::Array));
    let define = h(cx, Helper::Define);
    let get = h(cx, Helper::Get);
    let number_to_string = cx.helper(runtime::Helper::Num(numstr::Helper::NumberToString));
    let length = cx.str("length");
    const A: u32 = 0;
    const O: u32 = 1;
    const N: u32 = 2;
    const I: u32 = 3;
    let mut f = Function::new([(1, rt(true, T_OBJECT)), (2, ValType::I32)]);
    let mut s = f.instructions();
    s.i32_const(CLASS_ARGUMENTS)
        .call(proto)
        .call(alloc)
        .local_set(O);
    s.local_get(A)
        .ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, array::F_LENGTH)
        .local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(O).ref_as_non_null();
    s.local_get(I).f64_convert_i32_u().call(number_to_string);
    s.ref_cast_non_null(string());
    s.local_get(A)
        .ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, array::F_ELEMS)
        .local_get(I)
        .array_get(T_VALS);
    s.i32_const(0).call(define);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(O).ref_as_non_null().global_get(length);
    s.local_get(N).ref_i31();
    s.i32_const(HIDDEN).call(define);
    s.local_get(O)
        .ref_as_non_null()
        .global_get(runtime::G_SYM_ITERATOR);
    s.call(array_proto)
        .global_get(runtime::G_SYM_ITERATOR)
        .call(get);
    s.i32_const(HIDDEN).call(define);
    s.local_get(O);
    s.end();
    f
}

/// `$own_symbols(o, all)` (T25): the own symbol keys in insertion order, as an array.
fn own_symbols(cx: &mut dyn Cx) -> Function {
    let new_array = rh(cx, runtime::Helper::Arr(array::Helper::New));
    const O: u32 = 0;
    const ALL: u32 = 1;
    const E: u32 = 2;
    const N: u32 = 3;
    const C: u32 = 4;
    const V: u32 = 5;
    let mut f = Function::new([(3, ValType::I32), (1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    // Is entry E a symbol key to list?
    let listed = |s: &mut InstructionSink| {
        s.local_get(O)
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(E)
            .array_get(T_KEYS)
            .ref_test_non_null(runtime::symbol());
        s.local_get(O)
            .struct_get(T_OBJECT, F_ATTRS)
            .local_get(E)
            .array_get_u(T_BYTES);
        // Private: never; hidden (non-enumerable): only with `all`.
        // (`select` picks its first operand when `all` is set.)
        s.i32_const(PRIVATE)
            .i32_const(PRIVATE | HIDDEN)
            .local_get(ALL)
            .select();
        s.i32_and().i32_eqz();
        s.i32_and();
    };
    s.local_get(O).struct_get(T_OBJECT, F_COUNT).local_set(N);
    count_loop(&mut s, E, N, |s| {
        listed(s);
        s.local_get(C).i32_add().local_set(C);
    });
    s.local_get(C).array_new_default(T_VALS).local_set(V);
    s.i32_const(0).local_set(C);
    count_loop(&mut s, E, N, |s| {
        listed(s);
        s.if_(BlockType::Empty);
        s.local_get(V).ref_as_non_null().local_get(C);
        s.local_get(O)
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(E)
            .array_get(T_KEYS);
        s.array_set(T_VALS);
        s.local_get(C).i32_const(1).i32_add().local_set(C);
        s.end();
    });
    s.local_get(V)
        .ref_as_non_null()
        .local_get(C)
        .call(new_array);
    s.end();
    f
}

/// The callee text of a `Symbol.toPrimitive` call in errors (T25).
fn to_primitive_key(cx: &mut dyn Cx) -> u32 {
    cx.str("[Symbol.toPrimitive]")
}

/// The tags of the built-in prototypes (T25): `Symbol.toStringTag` own properties when the code
/// names `toStringTag`, and what [`Helper::BuiltinTag`] reports for their instances.
pub fn builtin_tag_name(b: Builtin) -> Option<&'static str> {
    Some(match b {
        Builtin::Map => "Map",
        Builtin::Set => "Set",
        Builtin::Promise => "Promise",
        Builtin::Generator => "Generator",
        Builtin::AsyncGenerator => "AsyncGenerator",
        Builtin::Symbol => "Symbol",
        Builtin::BigInt => "BigInt",
        Builtin::DisposableStack => "DisposableStack",
        Builtin::AsyncDisposableStack => "AsyncDisposableStack",
        Builtin::ArrayIterator => "Array Iterator",
        Builtin::MapIterator => "Map Iterator",
        Builtin::SetIterator => "Set Iterator",
        _ => return None,
    })
}

/// `$builtin_tag(o)` (T25), see [`Helper::BuiltinTag`].
fn builtin_tag(cx: &mut dyn Cx) -> Function {
    use crate::{generator, mapset, promise};
    const O: u32 = 0;
    const T: u32 = 1;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    if cx.names("toStringTag") {
        let get = h(cx, Helper::Get);
        let sym = rh(
            cx,
            runtime::Helper::WellKnownSymbol(
                js2wasi3_ir::well_known_symbol("toStringTag").expect("well-known"),
            ),
        );
        s.local_get(O).call(sym).call(get).local_set(T);
        runtime::is_string(&mut s, T);
        s.if_(BlockType::Empty);
        s.local_get(T).return_();
        s.end();
    }
    // T30: a wrapper's class.
    let wrapper_tag = rh(cx, runtime::Helper::Wrap(wrapper::Helper::Tag));
    s.local_get(O).ref_test_non_null(wrapper::wrapper());
    s.if_(BlockType::Empty);
    s.local_get(O).call(wrapper_tag).return_();
    s.end();
    let tag = |cx: &mut dyn Cx, s: &mut InstructionSink, name: &str| {
        let g = cx.str(name);
        s.if_(BlockType::Empty);
        s.global_get(g).return_();
        s.end();
    };
    let class = |s: &mut InstructionSink, c: i32| {
        s.local_get(O)
            .struct_get(T_OBJECT, F_CLASS)
            .i32_const(CLASS_MASK)
            .i32_and()
            .i32_const(c)
            .i32_eq();
    };
    if cx.stacks() {
        // T45.
        class(&mut s, CLASS_STACK);
        tag(cx, &mut s, "DisposableStack");
        class(&mut s, CLASS_ASYNC_STACK);
        tag(cx, &mut s, "AsyncDisposableStack");
    }
    if cx.names("$module") {
        // T41: a module namespace's `Symbol.toStringTag`.
        class(&mut s, CLASS_MODULE);
        tag(cx, &mut s, "Module");
    }
    if cx.has_maps() {
        class(&mut s, mapset::CLASS_MAP);
        tag(cx, &mut s, "Map");
        class(&mut s, mapset::CLASS_SET);
        tag(cx, &mut s, "Set");
        s.local_get(O).ref_test_non_null(mapset::mapiter());
        s.if_(BlockType::Empty);
        {
            let (m, st) = (cx.str("Map Iterator"), cx.str("Set Iterator"));
            // Modes 3–5 are a Set's (T53: were swapped).
            s.global_get(st).global_get(m);
            s.local_get(O)
                .ref_cast_non_null(mapset::mapiter())
                .struct_get(runtime::T_MAPITER, mapset::I_MODE)
                .i32_const(3)
                .i32_ge_u();
            s.typed_select(string_ref()).return_();
        }
        s.end();
    }
    s.local_get(O).ref_test_non_null(promise::promise());
    tag(cx, &mut s, "Promise");
    if cx.names("$typed") {
        // T20: typed arrays (`%TypedArray%.prototype[@@toStringTag]`) and `ArrayBuffer`s.
        let name = rh(cx, runtime::Helper::Typed(crate::typed::Helper::Name));
        s.local_get(O).ref_test_non_null(crate::typed::typed());
        s.if_(BlockType::Empty);
        s.local_get(O)
            .ref_cast_non_null(crate::typed::typed())
            .struct_get(runtime::T_TYPED, crate::typed::F_KIND)
            .call(name)
            .return_();
        s.end();
        s.local_get(O).ref_test_non_null(crate::typed::buffer());
        tag(cx, &mut s, "ArrayBuffer");
    }
    // Generator and async functions (their prototypes' tags, F70).
    for (kind, name) in [
        (func::KIND_GENERATOR, "GeneratorFunction"),
        (func::KIND_ASYNC, "AsyncFunction"),
        (func::KIND_ASYNC_GENERATOR, "AsyncGeneratorFunction"),
    ] {
        s.local_get(O).ref_test_non_null(func::function());
        s.if_(BlockType::Result(ValType::I32));
        s.local_get(O)
            .ref_cast_non_null(func::function())
            .struct_get(runtime::T_FUNCTION, func::F_KIND)
            .i32_const(kind)
            .i32_eq();
        s.else_();
        s.i32_const(0);
        s.end();
        tag(cx, &mut s, name);
    }
    class(&mut s, CLASS_ASYNC_GEN);
    tag(cx, &mut s, "AsyncGenerator");
    s.local_get(O).ref_test_non_null(generator::generator());
    tag(cx, &mut s, "Generator");
    s.local_get(O)
        .struct_get(T_OBJECT, F_PROTO)
        .global_get(G_PROTOS + Builtin::ArrayIterator.index())
        .ref_eq();
    s.local_get(O)
        .struct_get(T_OBJECT, F_PROTO)
        .ref_is_null()
        .i32_eqz()
        .i32_and();
    tag(cx, &mut s, "Array Iterator");
    if cx.has_namespaces() {
        class(&mut s, CLASS_MATH);
        tag(cx, &mut s, "Math");
        class(&mut s, CLASS_JSON);
        tag(cx, &mut s, "JSON");
        class(&mut s, CLASS_REFLECT);
        tag(cx, &mut s, "Reflect");
    }
    s.global_get(G_UNDEFINED);
    s.end();
    f
}

/// Define `o[Symbol.toStringTag] = tag` (read-only, non-enumerable) on the object in local `o`.
pub fn define_tag(cx: &mut dyn Cx, s: &mut InstructionSink, o: u32, tag: &str) {
    let define = h(cx, Helper::Define);
    let sym = rh(
        cx,
        runtime::Helper::WellKnownSymbol(
            js2wasi3_ir::well_known_symbol("toStringTag").expect("well-known"),
        ),
    );
    let tag = cx.str(tag);
    s.local_get(o).ref_cast_non_null(object());
    s.call(sym).ref_cast_non_null(string());
    s.global_get(tag);
    s.i32_const(HIDDEN | READONLY).call(define);
}
