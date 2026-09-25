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

use js2wasi3_ir::{Builtin, ErrorKind};
use wasm_encoder::{BlockType, Function, InstructionSink, ValType};

use crate::runtime::{
    self, Cx, G_BUILTIN, G_NULL, G_PROTOS, G_UNDEFINED, T_BYTES, T_I32S, T_KEYS, T_OBJECT,
    T_STRING, T_VALS, TAG, jsval, object, rt, string, string_ref,
};
use crate::{func, numstr};

const F_CLASS: u32 = 0;
const F_PROTO: u32 = 1;
const F_KEYS: u32 = 2;
const F_VALS: u32 = 3;
const F_ATTRS: u32 = 4;
const F_COUNT: u32 = 5;
const F_INDEX: u32 = 6;

pub const CLASS_PLAIN: i32 = 0;
pub const CLASS_ERROR: i32 = 1;

/// Attribute bit: not enumerable.
pub const HIDDEN: i32 = 1;

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
        Builtin::Error(ErrorKind::Error) => &["toString"],
        Builtin::Error(_) => &[],
        // `call` is replaced by a real function (`crate::func`).
        Builtin::Function => &["apply", "bind", "call", "toString"],
    }
}

fn parent(b: Builtin) -> Option<Builtin> {
    match b {
        Builtin::Object => None,
        Builtin::Error(k) if k != ErrorKind::Error => Some(Builtin::Error(ErrorKind::Error)),
        _ => Some(Builtin::Object),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(s: (ref $string)) -> i32`: FNV-1a.
    Hash,
    /// `(o: (ref $object), k: (ref $string)) -> i32`: own entry index or -1.
    FindOwn,
    /// `(o: (ref null $object), k) -> jsval`: the value on the prototype chain, Wasm null if absent.
    Lookup,
    /// `(o: (ref null $object), k) -> (ref null $object)`: the object on the chain that has `k`.
    Holder,
    /// `(o, k, v: jsval, attrs: i32)`: set an own property (attributes only for a new one).
    Define,
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
    /// `(o: (ref $object)) -> (ref $string)`: ToPrimitive.
    ToPrimitive,
    /// `(o: (ref $object)) -> f64`: ToNumber of an object.
    ObjectToNumber,
    /// `(message: (ref $string))`: throw a new error of this kind.
    Throw(ErrorKind),
    /// `()`: trap: a built-in method was read.
    BuiltinTrap,
    /// `()`: trap: `console.log` of an object (F24).
    InspectTrap,
    /// `()`: trap: ToPrimitive needs a user `valueOf`/`toString` (T14).
    UserPrimitiveTrap,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Hash => "string_hash",
            Helper::FindOwn => "find_own",
            Helper::Lookup => "lookup",
            Helper::Holder => "holder",
            Helper::Define => "define",
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
                Builtin::Function => "Function.prototype",
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
            Helper::ForInKeys => "for_in_keys",
            Helper::Create => "object_create",
            Helper::GetPrototypeOf => "get_prototype_of",
            Helper::InstanceOf(_) => "instanceof",
            Helper::NewError(_) => "new_error",
            Helper::ToPrimitive => "to_primitive",
            Helper::ObjectToNumber => "object_to_number",
            Helper::Throw(_) => "throw_error",
            Helper::BuiltinTrap => "builtin_not_implemented",
            Helper::InspectTrap => "inspect_object_not_implemented",
            Helper::UserPrimitiveTrap => "user_to_primitive_not_implemented",
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
            Helper::Lookup => (vec![on, s], vec![v]),
            Helper::Holder => (vec![on, s], vec![on]),
            Helper::Define => (vec![o, s, v, I32], vec![]),
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
            Helper::OwnKeys => (vec![o], vec![rt(false, T_KEYS)]),
            Helper::ForInKeys | Helper::Create | Helper::GetPrototypeOf => (vec![v], vec![v]),
            Helper::InstanceOf(_) => (vec![v], vec![I32]),
            Helper::NewError(_) => (vec![v, v], vec![v]),
            Helper::ToPrimitive => (vec![o], vec![s]),
            Helper::ObjectToNumber => (vec![o], vec![F64]),
            Helper::Throw(_) => (vec![s], vec![]),
            Helper::BuiltinTrap | Helper::InspectTrap | Helper::UserPrimitiveTrap => {
                (vec![], vec![])
            }
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Hash => hash(),
            Helper::FindOwn => find_own(cx),
            Helper::Lookup => lookup(cx, false),
            Helper::Holder => lookup(cx, true),
            Helper::Define => define(cx),
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
                let mut f = Function::new([]);
                let mut s = f.instructions();
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
            Helper::OwnKeys => own_keys(cx),
            Helper::ForInKeys => for_in_keys(cx),
            Helper::Create => create(cx),
            Helper::GetPrototypeOf => get_prototype_of(cx),
            Helper::InstanceOf(b) => instance_of(cx, b),
            Helper::NewError(k) => new_error(cx, k),
            Helper::ToPrimitive => to_primitive(cx),
            Helper::ObjectToNumber => object_to_number(cx),
            Helper::Throw(k) => throw(cx, k),
            Helper::BuiltinTrap | Helper::InspectTrap | Helper::UserPrimitiveTrap => {
                let mut f = Function::new([]);
                f.instructions().unreachable().end();
                f
            }
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
    s.local_get(v).ref_test_non_null(object());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(v)
        .ref_cast_non_null(object())
        .struct_get(T_OBJECT, F_CLASS)
        .i32_const(CLASS_ERROR)
        .i32_eq();
    s.else_();
    s.i32_const(0);
    s.end();
}

/// Push local `v` (a jsval) cast to `(ref $object)`.
fn as_obj(s: &mut InstructionSink, v: u32) {
    s.local_get(v).ref_cast_non_null(object());
}

/// `for (i = 0; i < n; i++) body`, with `i` and `n` locals already holding values
/// (`i` is set to 0 here). `body` may `br 1` to exit or `br 0` after incrementing itself.
fn count_loop(s: &mut InstructionSink, i: u32, n: u32, body: impl FnOnce(&mut InstructionSink)) {
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

fn find_own(cx: &mut dyn Cx) -> Function {
    let str_eq = rh(cx, runtime::Helper::StrEq);
    let hash = h(cx, Helper::Hash);
    const O: u32 = 0;
    const K: u32 = 1;
    const E: u32 = 2;
    const N: u32 = 3;
    const MASK: u32 = 4;
    const I: u32 = 5;
    const KK: u32 = 6;
    const IDX: u32 = 7;
    let mut f = Function::new([
        (4, ValType::I32),
        (1, rt(true, T_STRING)),
        (1, rt(true, T_I32S)),
    ]);
    let mut s = f.instructions();
    // Is entry E's key K? (leaves i32)
    let matches = |s: &mut InstructionSink| {
        s.local_get(O)
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(E)
            .array_get(T_KEYS)
            .local_tee(KK)
            .ref_is_null();
        s.if_(BlockType::Result(ValType::I32));
        s.i32_const(0);
        s.else_();
        s.local_get(KK).ref_as_non_null().local_get(K).call(str_eq);
        s.end();
    };
    s.local_get(O)
        .struct_get(T_OBJECT, F_INDEX)
        .local_tee(IDX)
        .ref_is_null();
    s.if_(BlockType::Empty);
    {
        s.local_get(O).struct_get(T_OBJECT, F_COUNT).local_set(N);
        count_loop(&mut s, E, N, |s| {
            matches(s);
            s.if_(BlockType::Empty);
            s.local_get(E).return_();
            s.end();
        });
        s.i32_const(-1).return_();
    }
    s.end();
    s.local_get(IDX)
        .array_len()
        .i32_const(1)
        .i32_sub()
        .local_set(MASK);
    s.local_get(K)
        .call(hash)
        .local_get(MASK)
        .i32_and()
        .local_set(I);
    s.loop_(BlockType::Empty);
    s.local_get(IDX).local_get(I).array_get(T_I32S).local_tee(E);
    s.i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(-1).return_();
    s.end();
    s.local_get(E).i32_const(1).i32_sub().local_set(E);
    matches(&mut s);
    s.if_(BlockType::Empty);
    s.local_get(E).return_();
    s.end();
    s.local_get(I)
        .i32_const(1)
        .i32_add()
        .local_get(MASK)
        .i32_and()
        .local_set(I);
    s.br(0);
    s.end();
    s.unreachable();
    s.end();
    f
}

/// `$lookup` / `$holder`: walk the prototype chain.
fn lookup(cx: &mut dyn Cx, holder: bool) -> Function {
    let find = h(cx, Helper::FindOwn);
    const O: u32 = 0;
    const K: u32 = 1;
    const E: u32 = 2;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(O).ref_is_null().br_if(1);
    s.local_get(O)
        .ref_as_non_null()
        .local_get(K)
        .call(find)
        .local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    if holder {
        s.local_get(O).return_();
    } else {
        s.local_get(O)
            .struct_get(T_OBJECT, F_VALS)
            .local_get(E)
            .array_get(T_VALS)
            .return_();
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

fn define(cx: &mut dyn Cx) -> Function {
    let find = h(cx, Helper::FindOwn);
    let grow = h(cx, Helper::Grow);
    let reindex = h(cx, Helper::Reindex);
    let hash = h(cx, Helper::Hash);
    const O: u32 = 0;
    const K: u32 = 1;
    const V: u32 = 2;
    const A: u32 = 3;
    const E: u32 = 4;
    const MASK: u32 = 5;
    const I: u32 = 6;
    const IDX: u32 = 7;
    let mut f = Function::new([(3, ValType::I32), (1, rt(true, T_I32S))]);
    let mut s = f.instructions();
    s.local_get(O).local_get(K).call(find).local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .struct_get(T_OBJECT, F_VALS)
        .local_get(E)
        .local_get(V)
        .array_set(T_VALS);
    s.return_();
    s.end();
    // A new entry at `count`.
    s.local_get(O).struct_get(T_OBJECT, F_COUNT);
    s.local_get(O).struct_get(T_OBJECT, F_KEYS).array_len();
    s.i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(O).call(grow);
    s.end();
    s.local_get(O).struct_get(T_OBJECT, F_COUNT).local_set(E);
    s.local_get(O)
        .struct_get(T_OBJECT, F_KEYS)
        .local_get(E)
        .local_get(K)
        .array_set(T_KEYS);
    s.local_get(O)
        .struct_get(T_OBJECT, F_VALS)
        .local_get(E)
        .local_get(V)
        .array_set(T_VALS);
    s.local_get(O)
        .struct_get(T_OBJECT, F_ATTRS)
        .local_get(E)
        .local_get(A)
        .array_set(T_BYTES);
    s.local_get(O)
        .local_get(E)
        .i32_const(1)
        .i32_add()
        .struct_set(T_OBJECT, F_COUNT);
    // Index it: start an index past SMALL entries, grow it past half full.
    s.local_get(O)
        .struct_get(T_OBJECT, F_INDEX)
        .local_tee(IDX)
        .ref_is_null();
    s.if_(BlockType::Empty);
    s.local_get(E).i32_const(SMALL).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(O).i32_const(64).call(reindex);
    s.end();
    s.return_();
    s.end();
    s.local_get(E)
        .i32_const(1)
        .i32_add()
        .i32_const(1)
        .i32_shl()
        .local_get(IDX)
        .array_len()
        .i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .local_get(IDX)
        .array_len()
        .i32_const(1)
        .i32_shl()
        .call(reindex);
    s.return_();
    s.end();
    s.local_get(IDX)
        .array_len()
        .i32_const(1)
        .i32_sub()
        .local_set(MASK);
    s.local_get(K)
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
    let g = G_PROTOS + b.index();
    let mut props: Vec<(u32, Option<u32>)> = vec![(cx.str("constructor"), None)];
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
    // Function.prototype: `length: 0` and a real `call`.
    let function = (b == Builtin::Function).then(|| {
        let code = cx.helper(runtime::Helper::Func(func::Helper::Call));
        cx.declare_func(code);
        (
            cx.str("length"),
            cx.str("call"),
            code,
            cx.helper(runtime::Helper::Func(func::Helper::New)),
        )
    });
    const O: u32 = 0;
    let mut f = Function::new([(1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.global_get(g).local_tee(O).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(O).ref_as_non_null().return_();
    s.end();
    s.i32_const(CLASS_PLAIN);
    match parent {
        Some(p) => s.call(p),
        None => s.ref_null(object()),
    };
    s.call(alloc).local_tee(O).global_set(g);
    for (key, value) in props {
        s.local_get(O).ref_as_non_null().global_get(key);
        match value {
            Some(v) => s.global_get(v),
            None => s.global_get(G_BUILTIN),
        };
        s.i32_const(HIDDEN).call(define);
    }
    if let Some((length, call, code, new)) = function {
        s.local_get(O).ref_as_non_null().global_get(length);
        s.i32_const(0).ref_i31();
        s.i32_const(HIDDEN).call(define);
        s.local_get(O).ref_as_non_null().global_get(call);
        s.ref_func(code);
        s.ref_null(func::vals());
        s.global_get(call).i32_const(1).i32_const(0).call(new);
        s.i32_const(HIDDEN).call(define);
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

fn to_key(cx: &mut dyn Cx) -> Function {
    let to_string = rh(cx, runtime::Helper::ToString);
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).call(to_string);
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
    let (sp, np, bp) = (
        h(cx, Helper::Proto(Builtin::String)),
        h(cx, Helper::Proto(Builtin::Number)),
        h(cx, Helper::Proto(Builtin::Boolean)),
    );
    s.local_get(v).ref_test_non_null(string());
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
}

fn get(cx: &mut dyn Cx) -> Function {
    let to_key = h(cx, Helper::ToKey);
    let lookup = h(cx, Helper::Lookup);
    let is_nullish = rh(cx, runtime::Helper::IsNullish);
    let throw_read = rh(cx, runtime::Helper::ThrowRead);
    let str_eq = rh(cx, runtime::Helper::StrEq);
    let index = h(cx, Helper::ArrayIndex);
    let box_ = rh(cx, runtime::Helper::Box);
    let trap = h(cx, Helper::BuiltinTrap);
    let length = cx.str("length");
    const V: u32 = 0;
    const KEY: u32 = 1;
    const K: u32 = 2;
    const R: u32 = 3;
    const I: u32 = 4;
    let mut f = Function::new([(1, rt(true, T_STRING)), (1, jsval()), (1, ValType::I64)]);
    let mut s = f.instructions();
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.local_get(V).local_get(KEY).call(to_key).call(throw_read);
    s.unreachable();
    s.end();
    s.local_get(KEY).call(to_key).local_set(K);
    s.local_get(V).ref_test_non_null(object());
    s.if_(BlockType::Result(jsval()));
    as_obj(&mut s, V);
    s.local_get(K).ref_as_non_null().call(lookup);
    s.else_();
    s.local_get(V).ref_test_non_null(string());
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
    s.local_tee(R);
    s.ref_is_null();
    s.if_(BlockType::Empty);
    s.global_get(G_UNDEFINED).return_();
    s.end();
    s.local_get(R).global_get(G_BUILTIN).ref_eq();
    s.if_(BlockType::Empty);
    s.call(trap);
    s.end();
    s.local_get(R);
    s.end();
    f
}

fn set(cx: &mut dyn Cx) -> Function {
    let to_key = h(cx, Helper::ToKey);
    let define = h(cx, Helper::Define);
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
    let mut f = Function::new([(1, rt(true, T_STRING))]);
    let mut s = f.instructions();
    let k = |s: &mut InstructionSink| {
        s.local_get(K).ref_as_non_null();
    };
    // Concatenate the string globals / pushes in `parts`, then throw.
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.global_get(set_of);
    s.local_get(V).call(to_string).call(concat);
    s.global_get(setting).call(concat);
    s.local_get(KEY).call(to_key).call(concat);
    s.global_get(close).call(concat);
    s.call(throw);
    s.end();
    s.local_get(KEY).call(to_key).local_set(K);
    s.local_get(V).ref_test_non_null(object());
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
        s.local_get(VAL).i32_const(0).call(define).return_();
    }
    s.end();
    s.local_get(V).ref_test_non_null(string());
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
    let is_nullish = rh(cx, runtime::Helper::IsNullish);
    let concat = rh(cx, runtime::Helper::Concat);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let convert = cx.str("Cannot convert undefined or null to object");
    let cannot = cx.str("Cannot delete property '");
    let of = cx.str("' of [object String]");
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
    s.local_get(V).ref_test_non_null(object());
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.local_get(K).ref_as_non_null().call(remove);
    s.i32_const(1).return_();
    s.end();
    s.local_get(V).ref_test_non_null(string());
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
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(object()).i32_eqz();
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
    let find = h(cx, Helper::FindOwn);
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
    s.local_get(V).ref_test_non_null(object());
    s.if_(BlockType::Empty);
    as_obj(&mut s, V);
    s.local_get(K).ref_as_non_null().call(find);
    s.i32_const(0).i32_ge_s().return_();
    s.end();
    s.local_get(V).ref_test_non_null(string());
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
    s.local_get(P).ref_test_non_null(object());
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
    let find = h(cx, Helper::FindOwn);
    let define = h(cx, Helper::Define);
    let number_to_string = rh(cx, runtime::Helper::Num(numstr::Helper::NumberToString));
    const T: u32 = 0;
    const SRC: u32 = 1;
    const I: u32 = 2;
    const N: u32 = 3;
    const KEYS: u32 = 4;
    const KK: u32 = 5;
    let mut f = Function::new([
        (2, ValType::I32),
        (1, rt(true, T_KEYS)),
        (1, rt(true, T_STRING)),
    ]);
    let mut s = f.instructions();
    s.local_get(SRC).ref_test_non_null(object());
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
            s.struct_get(T_OBJECT, F_VALS);
            as_obj(s, SRC);
            s.local_get(KK).ref_as_non_null().call(find);
            s.array_get(T_VALS);
            s.i32_const(0).call(define);
        });
        s.return_();
    }
    s.end();
    // A string spreads its characters.
    s.local_get(SRC).ref_test_non_null(string());
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

/// `$own_keys(o)`: live enumerable keys: array indices ascending, then the others in insertion order.
fn own_keys(cx: &mut dyn Cx) -> Function {
    let index = h(cx, Helper::ArrayIndex);
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
    let mut f = Function::new([
        (7, ValType::I32),
        (1, ValType::I64),
        (1, rt(true, T_STRING)),
        (1, rt(true, T_KEYS)),
        (1, rt(true, T_I32S)),
    ]);
    let mut s = f.instructions();
    // Push "entry E is live and enumerable", setting KK.
    let live = |s: &mut InstructionSink| {
        s.local_get(O)
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(E)
            .array_get(T_KEYS)
            .local_tee(KK)
            .ref_is_null()
            .i32_eqz();
        s.local_get(O)
            .struct_get(T_OBJECT, F_ATTRS)
            .local_get(E)
            .array_get_u(T_BYTES)
            .i32_const(HIDDEN)
            .i32_and()
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
    s.local_get(OUT).ref_as_non_null();
    s.end();
    f
}

/// `$for_in_keys(v)`: own enumerable keys, then each prototype's that no object before it on
/// the chain has (enumerable or not). A string's indices. Nothing for other primitives.
fn for_in_keys(cx: &mut dyn Cx) -> Function {
    let own_keys = h(cx, Helper::OwnKeys);
    let find = h(cx, Helper::FindOwn);
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
    s.local_get(V).ref_test_non_null(string());
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
    s.local_get(V).ref_test_non_null(object()).i32_eqz();
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
            .call(find)
            .i32_const(0)
            .i32_ge_s();
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
    s.local_get(P).ref_test_non_null(object());
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
    s.local_get(V).ref_test_non_null(object());
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
    s.local_get(V).ref_test_non_null(object()).i32_eqz();
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

/// `$new_error(message, options)`: own `stack` (a sentinel: reading it traps, F31), `message`
/// unless it's `undefined`, and `cause` if `options` is an object that has one.
fn new_error(cx: &mut dyn Cx, k: ErrorKind) -> Function {
    let alloc = h(cx, Helper::Alloc);
    let proto = h(cx, Helper::Proto(Builtin::Error(k)));
    let define = h(cx, Helper::Define);
    let lookup = h(cx, Helper::Lookup);
    let get = h(cx, Helper::Get);
    let to_string = rh(cx, runtime::Helper::ToString);
    let (stack, message, cause) = (cx.str("stack"), cx.str("message"), cx.str("cause"));
    const M: u32 = 0;
    const OPT: u32 = 1;
    const O: u32 = 2;
    let mut f = Function::new([(1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.i32_const(CLASS_ERROR)
        .call(proto)
        .call(alloc)
        .local_set(O);
    s.local_get(O)
        .ref_as_non_null()
        .global_get(stack)
        .global_get(G_BUILTIN);
    s.i32_const(HIDDEN).call(define);
    s.local_get(M).global_get(G_UNDEFINED).ref_eq().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(O).ref_as_non_null().global_get(message);
    s.local_get(M).call(to_string);
    s.i32_const(HIDDEN).call(define);
    s.end();
    s.local_get(OPT).ref_test_non_null(object());
    s.if_(BlockType::Empty);
    as_obj(&mut s, OPT);
    s.global_get(cause).call(lookup).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(O).ref_as_non_null().global_get(cause);
    s.local_get(OPT).global_get(cause).call(get);
    s.i32_const(HIDDEN).call(define);
    s.end();
    s.end();
    s.local_get(O).ref_as_non_null();
    s.end();
    f
}

/// `$to_primitive(o)`: OrdinaryToPrimitive with the built-in `valueOf` (which returns the
/// object, so it's skipped) and `toString`.
fn to_primitive(cx: &mut dyn Cx) -> Function {
    let holder = h(cx, Helper::Holder);
    let get = h(cx, Helper::Get);
    let obj_proto = h(cx, Helper::Proto(Builtin::Object));
    let err_proto = h(cx, Helper::Proto(Builtin::Error(ErrorKind::Error)));
    let user = h(cx, Helper::UserPrimitiveTrap);
    let throw = h(cx, Helper::Throw(ErrorKind::TypeError));
    let to_string = rh(cx, runtime::Helper::ToString);
    let concat = rh(cx, runtime::Helper::Concat);
    let value_of = cx.str("valueOf");
    let to_string_key = cx.str("toString");
    let (name, message) = (cx.str("name"), cx.str("message"));
    let (error, empty, colon) = (cx.str("Error"), cx.str(""), cx.str(": "));
    let (tag_object, tag_error) = (cx.str("[object Object]"), cx.str("[object Error]"));
    let no_prim = cx.str("Cannot convert object to primitive value");
    const O: u32 = 0;
    const HD: u32 = 1;
    const N: u32 = 2;
    const M: u32 = 3;
    const T: u32 = 4;
    let mut f = Function::new([
        (1, rt(true, T_OBJECT)),
        (2, rt(true, T_STRING)),
        (1, jsval()),
    ]);
    let mut s = f.instructions();
    s.local_get(O)
        .global_get(value_of)
        .call(holder)
        .local_tee(HD);
    s.ref_is_null().i32_eqz();
    s.local_get(HD).call(obj_proto).ref_eq().i32_eqz();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.call(user);
    s.end();
    s.local_get(O)
        .global_get(to_string_key)
        .call(holder)
        .local_tee(HD);
    s.ref_is_null();
    s.if_(BlockType::Empty);
    s.global_get(no_prim).call(throw);
    s.end();
    s.local_get(HD).call(obj_proto).ref_eq();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .struct_get(T_OBJECT, F_CLASS)
        .i32_const(CLASS_ERROR)
        .i32_eq();
    s.if_(BlockType::Result(string_ref()));
    s.global_get(tag_error);
    s.else_();
    s.global_get(tag_object);
    s.end();
    s.return_();
    s.end();
    s.local_get(HD).call(err_proto).ref_eq().i32_eqz();
    s.if_(BlockType::Empty);
    s.call(user);
    s.end();
    // Error.prototype.toString.
    for (key, local, default) in [(name, N, error), (message, M, empty)] {
        s.local_get(O).global_get(key).call(get).local_tee(T);
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

/// `$object_to_number(o)`: ToNumber of ToPrimitive(o). `""` is 0; a string starting with `[` or
/// a letter other than `I` can't be numeric: NaN. Anything else would need string parsing (F20): trap.
fn object_to_number(cx: &mut dyn Cx) -> Function {
    let to_primitive = h(cx, Helper::ToPrimitive);
    const O: u32 = 0;
    const P: u32 = 1;
    const C: u32 = 2;
    let mut f = Function::new([(1, rt(true, T_STRING)), (1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(O).call(to_primitive).local_tee(P);
    s.array_len().i32_eqz();
    s.if_(BlockType::Empty);
    s.f64_const(0.0.into()).return_();
    s.end();
    s.local_get(P)
        .ref_as_non_null()
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
    s.unreachable();
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
