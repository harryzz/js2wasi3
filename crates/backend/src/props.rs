//! Property attributes and accessors (T16, F36).
//!
//! An entry's attribute byte (see [`crate::object`]) has four bits: [`HIDDEN`] (not enumerable),
//! [`READONLY`] (not writable), [`FIXED`] (not configurable) and [`ACCESSOR`]: then the value slot
//! holds an `$accessor { get, set }` (absent functions are `undefined`), which never reaches JS.
//! An object's `class` field also holds [`NO_EXTEND`] (and, for arrays, whose elements have no
//! attribute bytes, [`SEALED`] / [`FROZEN`]).
//!
//! - Reads resolve the raw slot with [`Helper::Value`] (a getter is called with the receiver).
//! - Writes to an object go through [`Helper::Set`], OrdinarySet: an own writable data property is
//!   overwritten, a setter found on the chain is called, a read-only property or a getter without
//!   a setter throws node's TypeError, otherwise a new own property is added (if extensible).
//! - `Object.defineProperty` / `getOwnPropertyDescriptor` / `freeze` / `seal` /
//!   `preventExtensions` / `isFrozen` / `isSealed` / `isExtensible`.
//!
//! Arrays: attributes of indices and `length` other than the defaults (or those of a sealed /
//! frozen array) aren't representable: `defineProperty` traps on them.

use crate::runtime::JsTest;
use js2wasi3_ir::{ErrorKind, PrivateKind};
use wasm_encoder::{BlockType, Function, HeapType, InstructionSink, ValType};

use crate::array;
use crate::func;
use crate::object::{
    self, ACCESSOR, CLASS_MASK, F_ATTRS, F_CLASS, F_COUNT, F_KEYS, F_PROTO, F_VALS, FIXED, FROZEN,
    HIDDEN, NO_EXTEND, PRIVATE, READONLY, SEALED, as_obj, count_loop,
};
use crate::runtime::{
    self, Cx, G_BUILTIN, G_FALSE, G_TRUE, G_UNDEFINED, T_ACCESSOR, T_ARRAY, T_BYTES, T_KEYS,
    T_OBJECT, T_STRING, T_VALS, jsval, object, rt, string, string_ref,
};

pub fn accessor() -> HeapType {
    HeapType::Concrete(T_ACCESSOR)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(raw: eqref, receiver: jsval) -> jsval`: a property's value from its raw slot (Wasm null:
    /// absent → `undefined`; an accessor: call the getter; a built-in sentinel traps).
    Value,
    /// `(o: (ref $object), k: (ref $string), v: jsval)`: OrdinarySet with receiver `o`.
    Set,
    /// `(o, k, v, hash: i32)`: [`Helper::Set`] for a key that is neither an array index nor `length`,
    /// with its hash (T74): no array paths, one search per level, a new property appended directly.
    SetNamed,
    /// `(o: (ref null $object)) -> (ref null $string)`: the name of the first `constructor` found
    /// on the chain from `o` (a function with a non-empty name whose `prototype` is on `o`'s
    /// chain, like node's `getConstructorName`), else null.
    CtorName,
    /// `(o: (ref $object)) -> (ref $string)`: how node names an object in TypeErrors:
    /// `#<Name>` (`#<Object>` by default), `[object Array]` for an array.
    Desc,
    /// `(o: (ref $object), k: (ref $string), v: jsval, attrs: i32)`: define a data property with
    /// exactly these attributes (replacing an existing one).
    Put,
    /// `(o: jsval, key: jsval, f: jsval, which: i32, attrs: i32)`: define (or complete) an accessor:
    /// `f` becomes its getter (which = 0) or setter (1).
    DefineAccessor,
    /// `(o, key, desc: jsval) -> jsval`: `Object.defineProperty`.
    DefineProperty,
    /// `(o, key) -> jsval`: `Object.getOwnPropertyDescriptor`.
    GetOwnPropertyDescriptor,
    /// `(v) -> jsval`: `Object.preventExtensions` (0), `seal` (1), `freeze` (2).
    Integrity(u8),
    /// `(v) -> i32`: `Object.isExtensible` (0), `isSealed` (1), `isFrozen` (2).
    TestIntegrity(u8),
    /// `(home, key, this: jsval) -> jsval`: `super[key]`: the property of the home object's
    /// prototype, with `this` as the receiver.
    SuperGet,
    /// `(home, key, v, this: jsval)`: `super[key] = v`: OrdinarySet on the home object's prototype
    /// with `this` as the receiver.
    SuperSet,
    /// `(o, key, v: jsval)`: [`Helper::Put`] with these attributes (methods: [`HIDDEN`], fields: 0).
    DefineData(i32),
    /// `(o, sym, message: (ref $string)) -> jsval`: `o.#x`.
    PrivateGet,
    /// `(o, sym, v, message: (ref $string))`: `o.#x = v`.
    PrivateSet,
    /// `(sym, o) -> i32`: `#x in o`.
    PrivateHas,
    /// `(o, sym, v, message: (ref $string))`: add a private field, method (read-only) or accessor.
    PrivateAdd(PrivateKind),
    /// `(get, set: jsval) -> jsval`: a new `$accessor`.
    NewAccessor,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Value => "property_value",
            Helper::Set => "ordinary_set",
            Helper::SetNamed => "ordinary_set_named",
            Helper::CtorName => "constructor_name",
            Helper::Desc => "object_desc",
            Helper::Put => "put",
            Helper::DefineAccessor => "define_accessor",
            Helper::DefineProperty => "Object.defineProperty",
            Helper::GetOwnPropertyDescriptor => "Object.getOwnPropertyDescriptor",
            Helper::Integrity(0) => "Object.preventExtensions",
            Helper::Integrity(1) => "Object.seal",
            Helper::Integrity(_) => "Object.freeze",
            Helper::TestIntegrity(0) => "Object.isExtensible",
            Helper::TestIntegrity(1) => "Object.isSealed",
            Helper::TestIntegrity(_) => "Object.isFrozen",
            Helper::SuperGet => "super_get",
            Helper::SuperSet => "super_set",
            Helper::DefineData(HIDDEN) => "define_method",
            Helper::DefineData(_) => "define_field",
            Helper::PrivateGet => "private_get",
            Helper::PrivateSet => "private_set",
            Helper::PrivateHas => "private_has",
            Helper::PrivateAdd(PrivateKind::Field) => "private_add_field",
            Helper::PrivateAdd(PrivateKind::Method) => "private_add_method",
            Helper::PrivateAdd(PrivateKind::Accessor) => "private_add_accessor",
            Helper::NewAccessor => "new_accessor",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::I32;
        let v = jsval();
        let s = string_ref();
        let o = rt(false, T_OBJECT);
        match self {
            Helper::Value => (vec![v, v], vec![v]),
            Helper::Set => (vec![o, s, v], vec![]),
            Helper::SetNamed => (vec![o, s, v, I32], vec![]),
            Helper::CtorName => (vec![rt(true, T_OBJECT)], vec![rt(true, T_STRING)]),
            Helper::Desc => (vec![o], vec![s]),
            Helper::Put => (vec![o, s, v, I32], vec![]),
            Helper::DefineAccessor => (vec![v, v, v, I32, I32], vec![]),
            Helper::DefineProperty => (vec![v, v, v], vec![v]),
            Helper::GetOwnPropertyDescriptor => (vec![v, v], vec![v]),
            Helper::Integrity(_) => (vec![v], vec![v]),
            Helper::TestIntegrity(_) => (vec![v], vec![I32]),
            Helper::SuperGet => (vec![v, v, v], vec![v]),
            Helper::DefineData(_) => (vec![v, v, v], vec![]),
            Helper::SuperSet => (vec![v, v, v, v], vec![]),
            Helper::PrivateGet => (vec![v, v, s], vec![v]),
            Helper::PrivateSet | Helper::PrivateAdd(_) => (vec![v, v, v, s], vec![]),
            Helper::NewAccessor => (vec![v, v], vec![v]),
            Helper::PrivateHas => (vec![v, v], vec![I32]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Value => value(cx),
            Helper::Set => set(cx, false),
            Helper::SetNamed => set(cx, true),
            Helper::CtorName => ctor_name(cx),
            Helper::Desc => desc(cx),
            Helper::Put => put(cx),
            Helper::DefineAccessor => define_accessor(cx),
            Helper::DefineProperty => define_property(cx),
            Helper::GetOwnPropertyDescriptor => get_own_property_descriptor(cx),
            Helper::Integrity(op) => integrity(cx, op),
            Helper::TestIntegrity(op) => test_integrity(op, cx.names("$module")),
            Helper::SuperGet => super_get(cx),
            Helper::SuperSet => super_set(cx),
            Helper::PrivateGet | Helper::PrivateSet => {
                private_access(cx, self == Helper::PrivateSet)
            }
            Helper::PrivateHas => private_has(cx),
            Helper::PrivateAdd(kind) => private_add(cx, kind),
            Helper::NewAccessor => {
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(0).local_get(1).struct_new(T_ACCESSOR);
                s.end();
                f
            }
            Helper::DefineData(a) => {
                let to_key = oh(cx, object::Helper::ToKey);
                let put = ph(cx, Helper::Put);
                let mut f = Function::new([]);
                let mut s = f.instructions();
                as_obj(&mut s, 0);
                s.local_get(1).call(to_key);
                s.local_get(2).i32_const(a).call(put);
                s.end();
                f
            }
        }
    }
}

fn oh(cx: &mut dyn Cx, h: object::Helper) -> u32 {
    cx.helper(runtime::Helper::Obj(h))
}

fn ph(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Prop(h))
}

/// Push `o.attrs[e]` (locals holding `(ref $object)` / `i32`).
fn attrs(s: &mut InstructionSink, o: u32, e: u32) {
    s.local_get(o)
        .struct_get(T_OBJECT, F_ATTRS)
        .local_get(e)
        .array_get_u(T_BYTES);
}

/// Push `o.vals[e]`.
fn slot(s: &mut InstructionSink, o: u32, e: u32) {
    s.local_get(o)
        .struct_get(T_OBJECT, F_VALS)
        .local_get(e)
        .array_get(T_VALS);
}

fn value(cx: &mut dyn Cx) -> Function {
    let trap = oh(cx, object::Helper::BuiltinTrap);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let getter = cx.str("getter");
    const RAW: u32 = 0;
    const RCV: u32 = 1;
    const G: u32 = 2;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.local_get(RAW).ref_is_null();
    s.if_(BlockType::Empty);
    s.global_get(G_UNDEFINED).return_();
    s.end();
    s.local_get(RAW).ref_test_non_null(accessor());
    s.if_(BlockType::Empty);
    s.local_get(RAW)
        .ref_cast_non_null(accessor())
        .struct_get(T_ACCESSOR, 0)
        .local_tee(G);
    if cx.names("stack") {
        // An error's `stack` before its first read (T63).
        let error_stack = oh(cx, object::Helper::ErrorStack);
        s.global_get(G_BUILTIN).ref_eq();
        s.if_(BlockType::Empty);
        s.local_get(RCV).call(error_stack).return_();
        s.end();
        s.local_get(G);
    }
    s.global_get(G_UNDEFINED).ref_eq();
    s.if_(BlockType::Empty);
    s.global_get(G_UNDEFINED).return_();
    s.end();
    s.local_get(G).local_get(RCV);
    s.i32_const(0).array_new_default(T_VALS);
    s.global_get(getter);
    s.call(call_value).return_();
    s.end();
    s.local_get(RAW).global_get(G_BUILTIN).ref_eq();
    s.if_(BlockType::Empty);
    s.call(trap);
    s.end();
    s.local_get(RAW);
    s.end();
    f
}

/// `(object) -> i32`: is the object on the stack of this class?
fn is_class(s: &mut InstructionSink, class: i32) {
    s.struct_get(T_OBJECT, object::F_CLASS)
        .i32_const(object::CLASS_MASK)
        .i32_and()
        .i32_const(class)
        .i32_eq();
}

/// Throw "Cannot assign to read only property 'k' of object '<desc>'" (locals o, k).
fn throw_read_only(cx: &mut dyn Cx, s: &mut InstructionSink, o: u32, k: u32) {
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let desc = ph(cx, Helper::Desc);
    let (a, b, c) = (
        cx.str("Cannot assign to read only property '"),
        cx.str("' of object '"),
        cx.str("'"),
    );
    s.global_get(a);
    s.local_get(k).call(concat);
    s.global_get(b).call(concat);
    s.local_get(o).call(desc).call(concat);
    s.global_get(c).call(concat);
    s.call(throw);
}

/// An error's `stack` before its first read (T63): a write makes it a data property holding `v`
/// (on `o`, the receiver) and returns from the function being emitted.
fn lazy_stack_set(cx: &mut dyn Cx, s: &mut InstructionSink, acc: u32, o: u32, k: u32, v: u32) {
    if !cx.names("stack") {
        return;
    }
    let define = oh(cx, object::Helper::Define);
    s.local_get(acc)
        .ref_cast_non_null(accessor())
        .struct_get(T_ACCESSOR, 1)
        .global_get(G_BUILTIN)
        .ref_eq();
    s.if_(BlockType::Empty);
    s.local_get(o).ref_cast_non_null(runtime::object());
    s.local_get(k).ref_as_non_null();
    s.local_get(v).i32_const(object::HIDDEN).call(define);
    s.return_();
    s.end();
}

/// Call the setter of accessor `acc` (a local) with `this` = local `o` and argument local `v`, or
/// throw node's TypeError if it has none.
fn call_setter(cx: &mut dyn Cx, s: &mut InstructionSink, acc: u32, o: u32, k: u32, v: u32) {
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let desc = ph(cx, Helper::Desc);
    let setter = cx.str("setter");
    let (a, b, c) = (
        cx.str("Cannot set property "),
        cx.str(" of "),
        cx.str(" which has only a getter"),
    );
    if cx.names("$module") {
        // T41: a namespace's exports look like writable data properties that reject writes.
        s.local_get(o);
        is_class(s, object::CLASS_MODULE);
        s.if_(BlockType::Empty);
        throw_read_only(cx, s, o, k);
        s.end();
    }
    lazy_stack_set(cx, s, acc, o, k, v);
    s.local_get(acc)
        .ref_cast_non_null(accessor())
        .struct_get(T_ACCESSOR, 1)
        .global_get(G_UNDEFINED)
        .ref_eq();
    s.if_(BlockType::Empty);
    s.global_get(a);
    s.local_get(k).call(concat);
    s.global_get(b).call(concat);
    s.local_get(o).call(desc).call(concat);
    s.global_get(c).call(concat);
    s.call(throw);
    s.end();
    s.local_get(acc)
        .ref_cast_non_null(accessor())
        .struct_get(T_ACCESSOR, 1);
    s.local_get(o);
    s.local_get(v).array_new_fixed(T_VALS, 1);
    s.global_get(setter);
    s.call(call_value).drop();
}

/// `ordinary_set(o, k, v)`; with `named`, `ordinary_set_named(o, k, v, hash)` (see [`Helper::SetNamed`]).
fn set(cx: &mut dyn Cx, named: bool) -> Function {
    let find = oh(cx, if named { object::Helper::FindOwnNamed } else { object::Helper::FindOwn });
    let define = oh(cx, if named { object::Helper::AppendNamed } else { object::Helper::Define });
    let array_define = cx.helper(runtime::Helper::Arr(array::Helper::Define { set: true }));
    let array_own = cx.helper(runtime::Helper::Arr(array::Helper::OwnGet));
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let (add, not_ext) = (
        cx.str("Cannot add property "),
        cx.str(", object is not extensible"),
    );
    const O: u32 = 0;
    const K: u32 = 1;
    const V: u32 = 2;
    // With `named`, param 3 is the hash and the locals start one later.
    const HV: u32 = 3;
    let n = named as u32;
    let (e, a, p, x) = (3 + n, 4 + n, 5 + n, 6 + n);
    let find_k = |s: &mut InstructionSink| {
        s.local_get(K);
        if named {
            s.local_get(HV);
        }
    };
    let mut f = Function::new([(2, ValType::I32), (1, rt(true, T_OBJECT)), (1, jsval())]);
    let mut s = f.instructions();
    // An own property.
    s.local_get(O);
    find_k(&mut s);
    s.call(find).local_tee(e);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    {
        attrs(&mut s, O, e);
        s.local_tee(a).i32_const(ACCESSOR).i32_and();
        s.if_(BlockType::Empty);
        slot(&mut s, O, e);
        s.local_set(x);
        call_setter(cx, &mut s, x, O, K, V);
        s.return_();
        s.end();
        s.local_get(a).i32_const(READONLY).i32_and();
        s.if_(BlockType::Empty);
        throw_read_only(cx, &mut s, O, K);
        s.end();
        s.local_get(O)
            .struct_get(T_OBJECT, F_VALS)
            .local_get(e)
            .local_get(V)
            .array_set(T_VALS);
        s.return_();
    }
    s.end();
    // An array's index or `length` (its own checks).
    if !named {
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
    }
    // The prototype chain: a setter, or a read-only property, forbids a new own property.
    s.local_get(O).struct_get(T_OBJECT, F_PROTO).local_set(p);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(p).ref_is_null().br_if(1);
    if !named {
        s.local_get(p).ref_test_non_null(array::array());
        s.if_(BlockType::Empty);
        s.local_get(p)
            .ref_cast_non_null(array::array())
            .local_get(K)
            .call(array_own)
            .ref_is_null()
            .i32_eqz()
            .br_if(2);
        s.end();
    }
    s.local_get(p).ref_as_non_null();
    find_k(&mut s);
    s.call(find).local_tee(e);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    {
        s.local_get(p)
            .ref_as_non_null()
            .struct_get(T_OBJECT, F_ATTRS)
            .local_get(e)
            .array_get_u(T_BYTES)
            .local_tee(a);
        s.i32_const(ACCESSOR).i32_and();
        s.if_(BlockType::Empty);
        s.local_get(p)
            .ref_as_non_null()
            .struct_get(T_OBJECT, F_VALS)
            .local_get(e)
            .array_get(T_VALS)
            .local_set(x);
        call_setter(cx, &mut s, x, O, K, V);
        s.return_();
        s.end();
        s.local_get(a).i32_const(READONLY).i32_and();
        s.if_(BlockType::Empty);
        throw_read_only(cx, &mut s, O, K);
        s.end();
        s.br(2);
    }
    s.end();
    s.local_get(p)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_PROTO)
        .local_set(p);
    s.br(0).end().end();
    s.local_get(O)
        .struct_get(T_OBJECT, F_CLASS)
        .i32_const(NO_EXTEND)
        .i32_and();
    s.if_(BlockType::Empty);
    s.global_get(add);
    s.local_get(K).call(concat);
    s.global_get(not_ext).call(concat);
    s.call(throw);
    s.end();
    s.local_get(O).local_get(K).local_get(V).i32_const(0);
    if named {
        s.local_get(HV);
    }
    s.call(define);
    s.end();
    f
}

fn ctor_name(cx: &mut dyn Cx) -> Function {
    let find = oh(cx, object::Helper::FindOwn);
    let own = oh(cx, object::Helper::OwnGet);
    let (constructor, name, prototype) =
        (cx.str("constructor"), cx.str("name"), cx.str("prototype"));
    const O: u32 = 0;
    const E: u32 = 1;
    const C: u32 = 2;
    const N: u32 = 3;
    const PP: u32 = 4;
    const START: u32 = 5;
    const Q: u32 = 6;
    let mut f = Function::new([(1, ValType::I32), (3, jsval()), (2, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.local_get(O).local_set(START);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(O).ref_is_null().br_if(1);
    s.local_get(O)
        .ref_as_non_null()
        .global_get(constructor)
        .call(find)
        .local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_VALS)
        .local_get(E)
        .array_get(T_VALS)
        .local_tee(C);
    s.ref_test_non_null(func::function());
    s.if_(BlockType::Empty);
    s.local_get(C)
        .ref_cast_non_null(object())
        .global_get(name)
        .call(own)
        .local_set(N);
    runtime::is_string(&mut s, N);
    s.if_(BlockType::Empty);
    s.local_get(N).ref_cast_non_null(string()).array_len();
    s.if_(BlockType::Empty);
    // node: only if the object is an instance of it (`C.prototype` on its chain).
    s.local_get(C)
        .ref_cast_non_null(object())
        .global_get(prototype)
        .call(own)
        .local_set(PP);
    s.local_get(START).local_set(Q);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(Q).ref_is_null().br_if(1);
    s.local_get(Q)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_PROTO)
        .local_tee(Q);
    s.local_get(PP).ref_eq();
    s.if_(BlockType::Empty);
    s.local_get(N).ref_cast_non_null(string()).return_();
    s.end();
    s.br(0).end().end();
    s.end();
    s.end();
    s.end();
    s.end();
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_PROTO)
        .local_set(O);
    s.br(0).end().end();
    s.ref_null(string());
    s.end();
    f
}

fn desc(cx: &mut dyn Cx) -> Function {
    let ctor_name = ph(cx, Helper::CtorName);
    let concat = cx.helper(runtime::Helper::Concat);
    let (array, open, close, object_) = (
        cx.str("[object Array]"),
        cx.str("#<"),
        cx.str(">"),
        cx.str("Object"),
    );
    const O: u32 = 0;
    const N: u32 = 1;
    let mut f = Function::new([(1, rt(true, T_STRING))]);
    let mut s = f.instructions();
    s.local_get(O).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    s.global_get(array).return_();
    s.end();
    // T30: a wrapper is `[object String]` etc.
    s.local_get(O).ref_test_non_null(crate::wrapper::wrapper());
    s.if_(BlockType::Empty);
    {
        let tag = cx.helper(runtime::Helper::Wrap(crate::wrapper::Helper::Tag));
        let (a, b) = (cx.str("[object "), cx.str("]"));
        s.global_get(a);
        s.local_get(O).call(tag).ref_cast_non_null(runtime::string());
        s.call(concat);
        s.global_get(b).call(concat).return_();
    }
    s.end();
    if cx.names("$module") {
        // T41.
        let module = cx.str("[object Module]");
        s.local_get(O);
        is_class(&mut s, object::CLASS_MODULE);
        s.if_(BlockType::Empty);
        s.global_get(module).return_();
        s.end();
    }
    if cx.names("$date") {
        // T26.
        let date = cx.str("[object Date]");
        s.local_get(O)
            .struct_get(T_OBJECT, object::F_CLASS)
            .i32_const(object::CLASS_MASK)
            .i32_and()
            .i32_const(object::CLASS_DATE)
            .i32_eq();
        s.if_(BlockType::Empty);
        s.global_get(date).return_();
        s.end();
    }
    if cx.names("$regexp") {
        // T27: V8's text for a RegExp.
        let regexp = cx.str("[object RegExp]");
        s.local_get(O)
            .struct_get(T_OBJECT, object::F_CLASS)
            .i32_const(object::CLASS_MASK)
            .i32_and()
            .i32_const(object::CLASS_REGEXP)
            .i32_eq();
        s.if_(BlockType::Empty);
        s.global_get(regexp).return_();
        s.end();
    }
    s.global_get(open);
    s.local_get(O).call(ctor_name).local_tee(N);
    s.ref_is_null();
    s.if_(BlockType::Result(string_ref()));
    s.global_get(object_);
    s.else_();
    s.local_get(N).ref_as_non_null();
    s.end();
    s.call(concat);
    s.global_get(close).call(concat);
    s.end();
    f
}

fn put(cx: &mut dyn Cx) -> Function {
    let define = oh(cx, object::Helper::Define);
    let find = oh(cx, object::Helper::FindOwn);
    let set_attr = oh(cx, object::Helper::SetAttr);
    const O: u32 = 0;
    const K: u32 = 1;
    const V: u32 = 2;
    const A: u32 = 3;
    const E: u32 = 4;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    // `define` gives a new property its attributes (T83: a shape transition with them); an existing
    // one changes them after.
    s.local_get(O)
        .local_get(K)
        .local_get(V)
        .local_get(A)
        .call(define);
    s.local_get(O).local_get(K).call(find).local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(O).local_get(E).local_get(A).call(set_attr);
    s.end();
    s.end();
    f
}

fn define_accessor(cx: &mut dyn Cx) -> Function {
    let to_key = oh(cx, object::Helper::ToKey);
    let find = oh(cx, object::Helper::FindOwn);
    let put = ph(cx, Helper::Put);
    const V: u32 = 0;
    const KEY: u32 = 1;
    const F: u32 = 2;
    const WHICH: u32 = 3;
    const A: u32 = 4;
    const O: u32 = 5;
    const K: u32 = 6;
    const E: u32 = 7;
    const X: u32 = 8;
    let mut f = Function::new([
        (1, rt(true, T_OBJECT)),
        (1, rt(true, T_STRING)),
        (1, ValType::I32),
        (1, rt(true, T_ACCESSOR)),
    ]);
    let mut s = f.instructions();
    s.local_get(V).ref_cast_non_null(object()).local_set(O);
    s.local_get(KEY).call(to_key).local_set(K);
    s.local_get(O)
        .ref_as_non_null()
        .local_get(K)
        .ref_as_non_null()
        .call(find)
        .local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_ATTRS)
        .local_get(E)
        .array_get_u(T_BYTES)
        .i32_const(ACCESSOR)
        .i32_and();
    s.if_(BlockType::Empty);
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_VALS)
        .local_get(E)
        .array_get(T_VALS)
        .ref_cast_non_null(accessor())
        .local_set(X);
    s.end();
    s.end();
    s.local_get(X).ref_is_null();
    s.if_(BlockType::Empty);
    s.global_get(G_UNDEFINED)
        .global_get(G_UNDEFINED)
        .struct_new(T_ACCESSOR)
        .local_set(X);
    s.local_get(O)
        .ref_as_non_null()
        .local_get(K)
        .ref_as_non_null();
    s.local_get(X);
    s.local_get(A).i32_const(ACCESSOR).i32_or();
    s.call(put);
    s.end();
    s.local_get(WHICH);
    s.if_(BlockType::Empty);
    s.local_get(X).local_get(F).struct_set(T_ACCESSOR, 1);
    s.else_();
    s.local_get(X).local_get(F).struct_set(T_ACCESSOR, 0);
    s.end();
    s.end();
    f
}

/// Descriptor fields present (bits) and their boolean values.
const D_ENUM: i32 = 1;
const D_CONF: i32 = 2;
const D_VALUE: i32 = 4;
const D_WRIT: i32 = 8;
const D_GET: i32 = 16;
const D_SET: i32 = 32;

fn define_property(cx: &mut dyn Cx) -> Function {
    let set_attr = oh(cx, object::Helper::SetAttr);
    let to_key = oh(cx, object::Helper::ToKey);
    let find = oh(cx, object::Helper::FindOwn);
    let lookup = oh(cx, object::Helper::Lookup);
    let get = oh(cx, object::Helper::Get);
    let define = oh(cx, object::Helper::Define);
    let to_boolean = cx.helper(runtime::Helper::ToBoolean);
    let to_string = cx.helper(runtime::Helper::ToString);
    let strict_eq = cx.helper(runtime::Helper::StrictEq);
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let desc = ph(cx, Helper::Desc);
    let set_index = cx.helper(runtime::Helper::Arr(array::Helper::DefineIndex));
    let elem_attrs = cx.helper(runtime::Helper::Arr(array::Helper::Attrs));
    let set_elem_attrs = cx.helper(runtime::Helper::Arr(array::Helper::SetAttrs));
    let resize = cx.helper(runtime::Helper::Arr(array::Helper::Resize));
    let to_number = cx.helper(runtime::Helper::ToNumber);
    let to_int32 = cx.to_int32();
    let range_error = oh(cx, object::Helper::Throw(ErrorKind::RangeError));
    let invalid_length = cx.str("Invalid array length");
    let index = oh(cx, object::Helper::ArrayIndex);
    let str_eq = cx.helper(runtime::Helper::StrEq);
    let keys = [
        cx.str("enumerable"),
        cx.str("configurable"),
        cx.str("value"),
        cx.str("writable"),
        cx.str("get"),
        cx.str("set"),
    ];
    let length = cx.str("length");
    let non_object = cx.str("Object.defineProperty called on non-object");
    let not_desc = cx.str("Property description must be an object: ");
    let (getter_fn, setter_fn) = (
        cx.str("Getter must be a function: "),
        cx.str("Setter must be a function: "),
    );
    let both = cx.str(
        "Invalid property descriptor. Cannot both specify accessors and a value or writable attribute, ",
    );
    let redefine = cx.str("Cannot redefine property: ");
    let (define_prop, not_ext) = (
        cx.str("Cannot define property "),
        cx.str(", object is not extensible"),
    );
    const OV: u32 = 0;
    const KEY: u32 = 1;
    const D: u32 = 2;
    const O: u32 = 3;
    const K: u32 = 4;
    const HAS: u32 = 5;
    const BITS: u32 = 6;
    const VAL: u32 = 7;
    const GT: u32 = 8;
    const ST: u32 = 9;
    const E: u32 = 10;
    const A: u32 = 11;
    const X: u32 = 12;
    const CUR: u32 = 13;
    const T: u32 = 14;
    const J: u32 = 15;
    const NL: u32 = 16;
    const NL2: u32 = 17;
    let mut f = Function::new([
        (1, rt(true, T_OBJECT)),
        (1, rt(true, T_STRING)),
        (2, ValType::I32),
        (3, jsval()),
        (2, ValType::I32),
        (1, jsval()),
        (1, rt(true, T_ACCESSOR)),
        (1, jsval()),
        (1, ValType::I64),
        (2, ValType::F64),
    ]);
    let mut s = f.instructions();
    let fail = |s: &mut InstructionSink, msg: u32| {
        s.global_get(msg);
        s.local_get(K).ref_as_non_null().call(concat);
        s.call(throw);
    };
    s.test_object(OV).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(non_object).call(throw);
    s.end();
    s.local_get(OV).ref_cast_non_null(object()).local_set(O);
    s.local_get(KEY).call(to_key).local_set(K);
    s.test_object(D).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(not_desc);
    s.local_get(D).call(to_string).call(concat);
    s.call(throw);
    s.end();
    s.global_get(G_UNDEFINED)
        .local_tee(VAL)
        .local_tee(GT)
        .local_set(ST);
    // ToPropertyDescriptor: each field if present (`in`).
    for (i, key) in keys.iter().enumerate() {
        let bit = 1 << i;
        s.local_get(D)
            .ref_cast_non_null(object())
            .global_get(*key)
            .call(lookup)
            .ref_is_null()
            .i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(HAS).i32_const(bit).i32_or().local_set(HAS);
        s.local_get(D).global_get(*key).call(get).local_set(T);
        match bit {
            D_VALUE => {
                s.local_get(T).local_set(VAL);
            }
            D_GET | D_SET => {
                s.local_get(T).ref_test_non_null(func::function()).i32_eqz();
                s.local_get(T).global_get(G_UNDEFINED).ref_eq().i32_eqz();
                s.i32_and();
                s.if_(BlockType::Empty);
                s.global_get(if bit == D_GET { getter_fn } else { setter_fn });
                s.local_get(T).call(to_string).call(concat);
                s.call(throw);
                s.end();
                s.local_get(T).local_set(if bit == D_GET { GT } else { ST });
            }
            _ => {
                s.local_get(T).call(to_boolean);
                s.if_(BlockType::Empty);
                s.local_get(BITS).i32_const(bit).i32_or().local_set(BITS);
                s.end();
            }
        }
        s.end();
    }
    s.local_get(HAS)
        .i32_const(D_GET | D_SET)
        .i32_and()
        .i32_const(0)
        .i32_ne();
    s.local_get(HAS)
        .i32_const(D_VALUE | D_WRIT)
        .i32_and()
        .i32_const(0)
        .i32_ne();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.global_get(both);
    s.local_get(D)
        .ref_cast_non_null(object())
        .call(desc)
        .call(concat);
    s.call(throw);
    s.end();
    let arr = |s: &mut InstructionSink| {
        s.local_get(O).ref_cast_non_null(array::array());
    };
    // Where the property lives: an entry of the dictionary part (`E`: its index) or an array
    // element (`elem`, `E`: the index, T32).
    let load_attrs = |s: &mut InstructionSink, elem: bool| {
        if elem {
            arr(s);
            s.local_get(E).call(elem_attrs);
        } else {
            s.local_get(O)
                .ref_as_non_null()
                .struct_get(T_OBJECT, F_ATTRS)
                .local_get(E)
                .array_get_u(T_BYTES);
        }
    };
    let slots = |s: &mut InstructionSink, elem: bool| {
        if elem {
            arr(s);
            s.struct_get(T_ARRAY, array::F_ELEMS);
        } else {
            s.local_get(O).ref_as_non_null().struct_get(T_OBJECT, F_VALS);
        }
    };
    let load_slot = |s: &mut InstructionSink, elem: bool| {
        slots(s, elem);
        s.local_get(E).array_get(T_VALS);
    };
    let store_attrs = |s: &mut InstructionSink, elem: bool| {
        if elem {
            arr(s);
            s.local_get(E).local_get(A).call(set_elem_attrs);
        } else {
            // T83: an attribute change goes to dictionary mode.
            s.local_get(O).ref_as_non_null();
            s.local_get(E).local_get(A).call(set_attr);
        }
    };
    // An existing non-configurable property: only compatible changes.
    let validate = |s: &mut InstructionSink, elem: bool| {
        load_attrs(s, elem);
        s.local_tee(A);
        s.i32_const(FIXED).i32_and();
        s.if_(BlockType::Empty);
        s.block(BlockType::Empty);
        {
            s.block(BlockType::Empty);
            // configurable: true
            s.local_get(BITS).i32_const(D_CONF).i32_and().br_if(0);
            // enumerable changes
            s.local_get(HAS).i32_const(D_ENUM).i32_and();
            s.local_get(BITS).i32_const(D_ENUM).i32_and().i32_eqz();
            s.local_get(A)
                .i32_const(HIDDEN)
                .i32_and()
                .i32_eqz()
                .i32_eqz();
            s.i32_ne();
            s.i32_and().br_if(0);
            s.local_get(A).i32_const(ACCESSOR).i32_and();
            s.if_(BlockType::Empty);
            // accessor: no data fields, same get/set
            s.local_get(HAS)
                .i32_const(D_VALUE | D_WRIT)
                .i32_and()
                .br_if(1);
            load_slot(s, elem);
            s.ref_cast_non_null(accessor()).local_set(CUR);
            for (bit, field, local) in [(D_GET, 0, GT), (D_SET, 1, ST)] {
                s.local_get(HAS)
                    .i32_const(bit)
                    .i32_and()
                    .i32_const(0)
                    .i32_ne();
                s.local_get(CUR)
                    .ref_as_non_null()
                    .struct_get(T_ACCESSOR, field)
                    .local_get(local)
                    .ref_eq()
                    .i32_eqz();
                s.i32_and().br_if(1);
            }
            s.else_();
            // data: no accessor fields; read-only: not made writable, same value
            s.local_get(HAS).i32_const(D_GET | D_SET).i32_and().br_if(1);
            s.local_get(A).i32_const(READONLY).i32_and();
            s.if_(BlockType::Empty);
            s.local_get(BITS).i32_const(D_WRIT).i32_and().br_if(2);
            s.local_get(HAS)
                .i32_const(D_VALUE)
                .i32_and()
                .i32_const(0)
                .i32_ne();
            load_slot(s, elem);
            s.local_get(VAL).call(strict_eq).i32_eqz();
            s.i32_and().br_if(2);
            s.end();
            s.end();
            s.br(1);
            s.end();
            fail(s, redefine);
        }
        s.end();
        s.end();
    };
    // Apply the fields given.
    let apply = |s: &mut InstructionSink, elem: bool| {
        load_attrs(s, elem);
        s.local_set(A);
        for (bit, attr) in [(D_ENUM, HIDDEN), (D_CONF, FIXED)] {
            s.local_get(HAS).i32_const(bit).i32_and();
            s.if_(BlockType::Empty);
            s.local_get(A).i32_const(!attr).i32_and();
            // The attribute bit is set when the field is false.
            s.i32_const(0).i32_const(attr);
            s.local_get(BITS).i32_const(bit).i32_and();
            s.select();
            s.i32_or().local_set(A);
            s.end();
        }
        s.local_get(HAS).i32_const(D_GET | D_SET).i32_and();
        s.if_(BlockType::Empty);
        {
            // An accessor (a data property becomes one with both functions undefined).
            s.local_get(A).i32_const(ACCESSOR).i32_and().i32_eqz();
            s.if_(BlockType::Empty);
            slots(s, elem);
            s.local_get(E);
            s.global_get(G_UNDEFINED)
                .global_get(G_UNDEFINED)
                .struct_new(T_ACCESSOR);
            s.array_set(T_VALS);
            s.local_get(A)
                .i32_const(!READONLY)
                .i32_and()
                .i32_const(ACCESSOR)
                .i32_or()
                .local_set(A);
            s.end();
            load_slot(s, elem);
            s.ref_cast_non_null(accessor()).local_set(CUR);
            for (bit, field, local) in [(D_GET, 0, GT), (D_SET, 1, ST)] {
                s.local_get(HAS).i32_const(bit).i32_and();
                s.if_(BlockType::Empty);
                s.local_get(CUR)
                    .ref_as_non_null()
                    .local_get(local)
                    .struct_set(T_ACCESSOR, field);
                s.end();
            }
        }
        s.else_();
        {
            s.local_get(HAS)
                .i32_const(D_VALUE | D_WRIT)
                .i32_and()
                .i32_const(0)
                .i32_ne();
            s.local_get(A)
                .i32_const(ACCESSOR)
                .i32_and()
                .i32_const(0)
                .i32_ne();
            s.i32_and();
            s.if_(BlockType::Empty);
            // An accessor becomes a data property: `undefined`, read-only unless said otherwise.
            slots(s, elem);
            s.local_get(E).global_get(G_UNDEFINED).array_set(T_VALS);
            s.local_get(A)
                .i32_const(!ACCESSOR)
                .i32_and()
                .i32_const(READONLY)
                .i32_or()
                .local_set(A);
            s.end();
            s.local_get(HAS).i32_const(D_WRIT).i32_and();
            s.if_(BlockType::Empty);
            s.local_get(A).i32_const(!READONLY).i32_and();
            s.i32_const(0).i32_const(READONLY);
            s.local_get(BITS).i32_const(D_WRIT).i32_and();
            s.select();
            s.i32_or().local_set(A);
            s.end();
            s.local_get(HAS).i32_const(D_VALUE).i32_and();
            s.if_(BlockType::Empty);
            slots(s, elem);
            s.local_get(E).local_get(VAL).array_set(T_VALS);
            s.end();
        }
        s.end();
        store_attrs(s, elem);
    };
    s.local_get(O).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    {
        // An array's `length` (ArraySetLength, T32): the value is converted first (twice, as
        // the spec does); it is never configurable, enumerable or an accessor.
        s.local_get(K)
            .ref_as_non_null()
            .global_get(length)
            .call(str_eq);
        s.if_(BlockType::Empty);
        {
            s.local_get(HAS).i32_const(D_VALUE).i32_and();
            s.if_(BlockType::Empty);
            s.local_get(VAL).call(to_number).local_set(NL);
            s.local_get(VAL).call(to_number).local_set(NL2);
            s.local_get(NL).call(to_int32).f64_convert_i32_u();
            s.local_get(NL2).f64_ne();
            s.if_(BlockType::Empty);
            s.global_get(invalid_length).call(range_error);
            s.end();
            s.end();
            s.local_get(HAS).i32_const(D_GET | D_SET).i32_and();
            s.local_get(BITS).i32_const(D_CONF | D_ENUM).i32_and();
            s.i32_or();
            s.if_(BlockType::Empty);
            fail(&mut s, redefine);
            s.end();
            arr(&mut s);
            s.struct_get(T_ARRAY, object::F_CLASS)
                .i32_const(FROZEN | object::LENGTH_READONLY)
                .i32_and();
            s.if_(BlockType::Empty);
            // Read-only: not made writable, same value.
            s.local_get(BITS).i32_const(D_WRIT).i32_and();
            s.local_get(HAS).i32_const(D_VALUE).i32_and();
            s.if_(BlockType::Result(ValType::I32));
            s.local_get(NL2);
            arr(&mut s);
            s.struct_get(T_ARRAY, array::F_LENGTH).f64_convert_i32_u();
            s.f64_ne();
            s.else_();
            s.i32_const(0);
            s.end();
            s.i32_or();
            s.if_(BlockType::Empty);
            fail(&mut s, redefine);
            s.end();
            s.local_get(O).ref_as_non_null().return_();
            s.end();
            // `writable: false` applies even when a deletion fails below.
            s.local_get(HAS)
                .i32_const(D_WRIT)
                .i32_and()
                .i32_const(0)
                .i32_ne();
            s.local_get(BITS).i32_const(D_WRIT).i32_and().i32_eqz();
            s.i32_and();
            s.if_(BlockType::Empty);
            arr(&mut s);
            arr(&mut s);
            s.struct_get(T_ARRAY, object::F_CLASS)
                .i32_const(object::LENGTH_READONLY)
                .i32_or()
                .struct_set(T_ARRAY, object::F_CLASS);
            s.end();
            s.local_get(HAS).i32_const(D_VALUE).i32_and();
            s.if_(BlockType::Empty);
            arr(&mut s);
            s.local_get(NL2).i32_trunc_f64_u().call(resize);
            s.end();
            s.local_get(O).ref_as_non_null().return_();
        }
        s.end();
        // An index.
        s.local_get(K).ref_as_non_null().call(index).local_tee(J);
        s.i64_const(0).i64_ge_s();
        s.if_(BlockType::Empty);
        {
            s.local_get(J).i64_const(array::MAX_DENSE as i64).i64_ge_u();
            s.if_(BlockType::Empty);
            s.unreachable(); // sparse arrays aren't implemented
            s.end();
            s.local_get(J).i32_wrap_i64().local_set(E);
            // Absent (a hole or past the end): a new element, absent attributes false.
            s.local_get(E);
            arr(&mut s);
            s.struct_get(T_ARRAY, array::F_LENGTH).i32_lt_u();
            s.if_(BlockType::Result(ValType::I32));
            load_slot(&mut s, true);
            s.ref_is_null();
            s.else_();
            s.i32_const(1);
            s.end();
            s.if_(BlockType::Empty);
            {
                arr(&mut s);
                s.struct_get(T_ARRAY, object::F_CLASS)
                    .i32_const(NO_EXTEND)
                    .i32_and();
                s.local_get(E);
                arr(&mut s);
                s.struct_get(T_ARRAY, array::F_LENGTH).i32_ge_u();
                arr(&mut s);
                s.struct_get(T_ARRAY, object::F_CLASS)
                    .i32_const(object::LENGTH_READONLY)
                    .i32_and()
                    .i32_const(0)
                    .i32_ne();
                s.i32_and();
                s.i32_or();
                s.if_(BlockType::Empty);
                s.global_get(define_prop);
                s.local_get(K).ref_as_non_null().call(concat);
                s.global_get(not_ext).call(concat);
                s.call(throw);
                s.end();
                arr(&mut s);
                s.local_get(E).global_get(G_UNDEFINED).call(set_index);
                arr(&mut s);
                s.local_get(E)
                    .i32_const(HIDDEN | READONLY | FIXED)
                    .call(set_elem_attrs);
            }
            s.else_();
            validate(&mut s, true);
            s.end();
            apply(&mut s, true);
            s.local_get(O).ref_as_non_null().return_();
        }
        s.end();
    }
    s.end();
    s.local_get(O)
        .ref_as_non_null()
        .local_get(K)
        .ref_as_non_null()
        .call(find)
        .local_tee(E);
    s.i32_const(0).i32_lt_s();
    s.if_(BlockType::Empty);
    {
        // A new property: absent attributes are false.
        s.local_get(O)
            .ref_as_non_null()
            .struct_get(T_OBJECT, F_CLASS)
            .i32_const(NO_EXTEND)
            .i32_and();
        s.if_(BlockType::Empty);
        s.global_get(define_prop);
        s.local_get(K).ref_as_non_null().call(concat);
        s.global_get(not_ext).call(concat);
        s.call(throw);
        s.end();
        s.local_get(O)
            .ref_as_non_null()
            .local_get(K)
            .ref_as_non_null()
            .global_get(G_UNDEFINED)
            .i32_const(HIDDEN | READONLY | FIXED)
            .call(define);
        s.local_get(O)
            .ref_as_non_null()
            .local_get(K)
            .ref_as_non_null()
            .call(find)
            .local_set(E);
    }
    s.else_();
    validate(&mut s, false);
    s.end();
    apply(&mut s, false);
    let _ = X;
    s.local_get(O).ref_as_non_null();
    s.end();
    f
}

fn get_own_property_descriptor(cx: &mut dyn Cx) -> Function {
    let to_key = oh(cx, object::Helper::ToKey);
    let find = oh(cx, object::Helper::FindOwn);
    let new_object = oh(cx, object::Helper::NewObject);
    let define = oh(cx, object::Helper::Define);
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let array_own = cx.helper(runtime::Helper::Arr(array::Helper::OwnGet));
    let elem_attrs = cx.helper(runtime::Helper::Arr(array::Helper::Attrs));
    let index = oh(cx, object::Helper::ArrayIndex);
    let str_eq = cx.helper(runtime::Helper::StrEq);
    let convert = cx.str("Cannot convert undefined or null to object");
    let (value, writable, enumerable, configurable, get, set, length) = (
        cx.str("value"),
        cx.str("writable"),
        cx.str("enumerable"),
        cx.str("configurable"),
        cx.str("get"),
        cx.str("set"),
        cx.str("length"),
    );
    const V: u32 = 0;
    const KEY: u32 = 1;
    const O: u32 = 2;
    const K: u32 = 3;
    const E: u32 = 4;
    const A: u32 = 5;
    const R: u32 = 6;
    const X: u32 = 7;
    let mut f = Function::new([
        (1, rt(true, T_OBJECT)),
        (1, rt(true, T_STRING)),
        (2, ValType::I32),
        (2, jsval()),
    ]);
    let mut s = f.instructions();
    let bool_of = |s: &mut InstructionSink| {
        // i32 on the stack → true/false
        s.if_(BlockType::Result(jsval()));
        s.global_get(G_TRUE);
        s.else_();
        s.global_get(G_FALSE);
        s.end();
    };
    let field = |s: &mut InstructionSink, key: u32, value: &dyn Fn(&mut InstructionSink)| {
        s.local_get(R).ref_cast_non_null(object()).global_get(key);
        value(s);
        s.i32_const(0).call(define);
    };
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.global_get(convert).call(throw);
    s.end();
    s.test_object(V).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(G_UNDEFINED).return_();
    s.end();
    s.local_get(V).ref_cast_non_null(object()).local_set(O);
    s.local_get(KEY).call(to_key).local_set(K);
    s.call(new_object).local_set(R);
    // The property's attributes (`A`) and slot (`X`): an array's index or `length` (T32), or an
    // entry of the dictionary part.
    s.block(BlockType::Empty);
    s.local_get(O).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    s.local_get(O)
        .ref_cast_non_null(array::array())
        .local_get(K)
        .ref_as_non_null()
        .call(array_own)
        .local_tee(X);
    s.ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(K)
        .ref_as_non_null()
        .global_get(length)
        .call(str_eq);
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(HIDDEN | FIXED | READONLY).i32_const(HIDDEN | FIXED);
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_CLASS)
        .i32_const(FROZEN | object::LENGTH_READONLY)
        .i32_and();
    s.select();
    s.else_();
    s.local_get(O).ref_cast_non_null(array::array());
    s.local_get(K).ref_as_non_null().call(index).i32_wrap_i64();
    s.call(elem_attrs);
    s.end();
    s.local_set(A);
    s.br(2);
    s.end();
    s.end();
    s.local_get(O)
        .ref_as_non_null()
        .local_get(K)
        .ref_as_non_null()
        .call(find)
        .local_tee(E);
    s.i32_const(0).i32_lt_s();
    s.if_(BlockType::Empty);
    s.global_get(G_UNDEFINED).return_();
    s.end();
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_ATTRS)
        .local_get(E)
        .array_get_u(T_BYTES)
        .local_set(A);
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_VALS)
        .local_get(E)
        .array_get(T_VALS)
        .local_set(X);
    s.end();
    if cx.names("stack") {
        // T63: an error's `stack` before its first read looks like the data property it becomes.
        let error_stack = oh(cx, object::Helper::ErrorStack);
        s.local_get(A).i32_const(ACCESSOR).i32_and();
        s.if_(BlockType::Empty);
        s.local_get(X)
            .ref_cast_non_null(accessor())
            .struct_get(T_ACCESSOR, 0)
            .global_get(G_BUILTIN)
            .ref_eq();
        s.if_(BlockType::Empty);
        s.local_get(O).call(error_stack).local_set(X);
        s.local_get(A).i32_const(!ACCESSOR & 0xff).i32_and().local_set(A);
        s.end();
        s.end();
    }
    if cx.names("$module") {
        // T41: a namespace's export (a getter) is a writable data property holding its value.
        let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
        let getter = cx.str("getter");
        s.local_get(A).i32_const(ACCESSOR).i32_and().i32_const(0).i32_ne();
        s.local_get(O).ref_as_non_null();
        is_class(&mut s, object::CLASS_MODULE);
        s.i32_and();
        s.if_(BlockType::Empty);
        s.local_get(X)
            .ref_cast_non_null(accessor())
            .struct_get(T_ACCESSOR, 0);
        s.local_get(O);
        s.i32_const(0).array_new_default(T_VALS);
        s.global_get(getter);
        s.call(call_value).local_set(X);
        s.local_get(A)
            .i32_const(!(ACCESSOR | READONLY) & 0xff)
            .i32_and()
            .local_set(A);
        s.end();
    }
    s.local_get(A).i32_const(ACCESSOR).i32_and();
    s.if_(BlockType::Empty);
    for (key, i) in [(get, 0), (set, 1)] {
        field(&mut s, key, &|s| {
            s.local_get(X)
                .ref_cast_non_null(accessor())
                .struct_get(T_ACCESSOR, i);
        });
    }
    s.else_();
    field(&mut s, value, &|s| {
        s.local_get(X);
    });
    field(&mut s, writable, &|s| {
        s.local_get(A).i32_const(READONLY).i32_and().i32_eqz();
        bool_of(s);
    });
    s.end();
    field(&mut s, enumerable, &|s| {
        s.local_get(A).i32_const(HIDDEN).i32_and().i32_eqz();
        bool_of(s);
    });
    field(&mut s, configurable, &|s| {
        s.local_get(A).i32_const(FIXED).i32_and().i32_eqz();
        bool_of(s);
    });
    s.local_get(R);
    s.end();
    f
}

fn integrity(cx: &mut dyn Cx, op: u8) -> Function {
    let unshape = oh(cx, object::Helper::Unshape);
    const V: u32 = 0;
    const O: u32 = 1;
    const E: u32 = 2;
    const N: u32 = 3;
    const A: u32 = 4;
    let mut f = Function::new([(1, rt(true, T_OBJECT)), (3, ValType::I32)]);
    let mut s = f.instructions();
    s.test_object(V).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(V).return_();
    s.end();
    s.local_get(V).ref_cast_non_null(object()).local_set(O);
    // T83: dictionary mode (a shaped object is extensible, and its attributes never change); a
    // layout change of a prototype.
    s.local_get(O).ref_as_non_null().call(unshape);
    if op >= 1 {
        s.local_get(O)
            .ref_as_non_null()
            .struct_get(T_OBJECT, F_CLASS)
            .i32_const(object::IS_PROTO)
            .i32_and();
        s.if_(BlockType::Empty);
        object::bump_epoch(&mut s);
        s.end();
    }
    let mut flags = NO_EXTEND;
    if op >= 1 {
        flags |= SEALED;
    }
    if op == 2 {
        flags |= FROZEN;
    }
    s.local_get(O).ref_as_non_null();
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_CLASS)
        .i32_const(flags)
        .i32_or();
    s.struct_set(T_OBJECT, F_CLASS);
    if op >= 1 {
        s.local_get(O)
            .ref_as_non_null()
            .struct_get(T_OBJECT, F_COUNT)
            .local_set(N);
        count_loop(&mut s, E, N, |s| {
            s.local_get(O)
                .ref_as_non_null()
                .struct_get(T_OBJECT, F_ATTRS)
                .local_get(E);
            s.local_get(O)
                .ref_as_non_null()
                .struct_get(T_OBJECT, F_ATTRS)
                .local_get(E)
                .array_get_u(T_BYTES)
                .local_tee(A);
            s.i32_const(FIXED).i32_or();
            if op == 2 {
                // Data properties become read-only.
                s.i32_const(READONLY).i32_const(0);
                s.local_get(A).i32_const(ACCESSOR).i32_and().i32_eqz();
                s.select().i32_or();
            }
            // (Private members aren't affected.)
            s.local_get(A);
            s.local_get(A).i32_const(PRIVATE).i32_and().i32_eqz();
            s.select();
            s.array_set(T_BYTES);
        });
    }
    s.local_get(V);
    s.end();
    f
}

/// `module`: the program has module namespaces (T41), whose exports (getters) count as writable.
fn test_integrity(op: u8, module: bool) -> Function {
    const V: u32 = 0;
    const O: u32 = 1;
    const E: u32 = 2;
    const N: u32 = 3;
    const A: u32 = 4;
    let mut f = Function::new([(1, rt(true, T_OBJECT)), (3, ValType::I32)]);
    let mut s = f.instructions();
    s.test_object(V).i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const((op != 0) as i32).return_();
    s.end();
    s.local_get(V).ref_cast_non_null(object()).local_set(O);
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_CLASS)
        .i32_const(NO_EXTEND)
        .i32_and();
    if op == 0 {
        s.i32_eqz().return_();
        s.end();
        return f;
    }
    s.i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    // Arrays: their elements.
    s.local_get(O).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_CLASS)
        .i32_const(if op == 2 { FROZEN } else { SEALED })
        .i32_and()
        .i32_eqz();
    if op == 1 {
        s.local_get(O)
            .ref_cast_non_null(array::array())
            .struct_get(T_ARRAY, array::F_LENGTH)
            .i32_const(0)
            .i32_ne()
            .i32_and();
    }
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.end();
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_COUNT)
        .local_set(N);
    count_loop(&mut s, E, N, |s| {
        s.local_get(O)
            .ref_as_non_null()
            .struct_get(T_OBJECT, F_KEYS)
            .local_get(E)
            .array_get(T_KEYS)
            .ref_is_null()
            .i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(O)
            .ref_as_non_null()
            .struct_get(T_OBJECT, F_ATTRS)
            .local_get(E)
            .array_get_u(T_BYTES)
            .local_tee(A);
        s.i32_const(FIXED).i32_and().i32_eqz();
        if op == 2 {
            s.local_get(A)
                .i32_const(ACCESSOR | READONLY)
                .i32_and()
                .i32_eqz();
            s.i32_or();
            if module {
                s.local_get(A).i32_const(ACCESSOR).i32_and().i32_const(0).i32_ne();
                s.local_get(O).ref_as_non_null();
                is_class(s, object::CLASS_MODULE);
                s.i32_and();
                s.i32_or();
            }
        }
        s.local_get(A).i32_const(PRIVATE).i32_and().i32_eqz();
        s.i32_and();
        s.if_(BlockType::Empty);
        s.i32_const(0).return_();
        s.end();
        s.end();
    });
    s.i32_const(1);
    s.end();
    f
}

fn super_get(cx: &mut dyn Cx) -> Function {
    let to_key = oh(cx, object::Helper::ToKey);
    let lookup = oh(cx, object::Helper::Lookup);
    let value = ph(cx, Helper::Value);
    let throw_read = cx.helper(runtime::Helper::ThrowRead);
    const HOME: u32 = 0;
    const KEY: u32 = 1;
    const THIS: u32 = 2;
    const P: u32 = 3;
    const K: u32 = 4;
    let mut f = Function::new([(1, rt(true, T_OBJECT)), (1, rt(true, T_STRING))]);
    let mut s = f.instructions();
    // GetSuperBase before ToPropertyKey; a `null` base throws like `null.x` (T38, F168).
    as_obj(&mut s, HOME);
    s.struct_get(T_OBJECT, F_PROTO).local_set(P);
    s.local_get(KEY).call(to_key).local_set(K);
    s.local_get(P).ref_is_null();
    s.if_(BlockType::Empty);
    s.global_get(runtime::G_NULL).local_get(K).ref_as_non_null().call(throw_read);
    s.end();
    s.local_get(P).local_get(K).ref_as_non_null().call(lookup);
    s.local_get(THIS).call(value);
    s.end();
    let _ = CLASS_MASK;
    f
}

/// `$private_get(o, sym, message)` / `$private_set(o, sym, v, message)`: an own private member
/// (`message`: the TypeError when `o` doesn't have it).
fn private_access(cx: &mut dyn Cx, write: bool) -> Function {
    let find = oh(cx, object::Helper::FindOwn);
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let throw_read = cx.helper(runtime::Helper::ThrowRead);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let concat = cx.helper(runtime::Helper::Concat);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let (method, not_writable) = (cx.str("Private method '"), cx.str("' is not writable"));
    let (quote, what, without) = if write {
        (cx.str("'"), cx.str("setter"), cx.str("' was defined without a setter"))
    } else {
        (cx.str("'"), cx.str("getter"), cx.str("' was defined without a getter"))
    };
    const O: u32 = 0;
    const SYM: u32 = 1;
    let v: u32 = 2;
    let msg: u32 = if write { 3 } else { 2 };
    let e: u32 = msg + 1;
    let g: u32 = e + 1;
    let mut f = Function::new([(1, ValType::I32), (1, jsval())]);
    let mut s = f.instructions();
    if !write {
        s.local_get(O).call(is_nullish);
        s.if_(BlockType::Empty);
        s.local_get(O)
            .local_get(SYM)
            .ref_cast_non_null(string())
            .call(throw_read);
        s.end();
    }
    s.test_object(O);
    s.if_(BlockType::Result(ValType::I32));
    as_obj(&mut s, O);
    s.local_get(SYM).ref_cast_non_null(string()).call(find);
    s.else_();
    s.i32_const(-1);
    s.end();
    s.local_tee(e).i32_const(0).i32_lt_s();
    s.if_(BlockType::Empty);
    s.local_get(msg).call(throw);
    s.end();
    // An accessor (T31): call its getter / setter, or throw node's TypeError without one.
    as_obj(&mut s, O);
    s.struct_get(T_OBJECT, F_ATTRS)
        .local_get(e)
        .array_get_u(T_BYTES)
        .i32_const(ACCESSOR)
        .i32_and();
    s.if_(BlockType::Empty);
    as_obj(&mut s, O);
    s.struct_get(T_OBJECT, F_VALS)
        .local_get(e)
        .array_get(T_VALS)
        .ref_cast_non_null(accessor())
        .struct_get(T_ACCESSOR, if write { 1 } else { 0 })
        .local_tee(g);
    s.global_get(G_UNDEFINED).ref_eq();
    s.if_(BlockType::Empty);
    s.global_get(quote);
    s.local_get(SYM).ref_cast_non_null(string()).call(concat);
    s.global_get(without).call(concat);
    s.call(throw);
    s.end();
    s.local_get(g).local_get(O);
    if write {
        s.local_get(v).array_new_fixed(T_VALS, 1);
    } else {
        s.i32_const(0).array_new_default(T_VALS);
    }
    s.global_get(what).call(call_value);
    if write {
        s.drop();
    }
    s.return_();
    s.end();
    as_obj(&mut s, O);
    if write {
        s.struct_get(T_OBJECT, F_ATTRS)
            .local_get(e)
            .array_get_u(T_BYTES)
            .i32_const(READONLY)
            .i32_and();
        s.if_(BlockType::Empty);
        s.global_get(method);
        s.local_get(SYM).ref_cast_non_null(string()).call(concat);
        s.global_get(not_writable).call(concat);
        s.call(throw);
        s.end();
        as_obj(&mut s, O);
        s.struct_get(T_OBJECT, F_VALS)
            .local_get(e)
            .local_get(v)
            .array_set(T_VALS);
    } else {
        s.struct_get(T_OBJECT, F_VALS)
            .local_get(e)
            .array_get(T_VALS);
    }
    s.end();
    f
}

fn private_has(cx: &mut dyn Cx) -> Function {
    let find = oh(cx, object::Helper::FindOwn);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let concat = cx.helper(runtime::Helper::Concat);
    let to_string = cx.helper(runtime::Helper::ToString);
    let (search, in_) = (
        cx.str("Cannot use 'in' operator to search for '"),
        cx.str("' in "),
    );
    const SYM: u32 = 0;
    const O: u32 = 1;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.test_object(O).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(search);
    s.local_get(SYM).ref_cast_non_null(string()).call(concat);
    s.global_get(in_).call(concat);
    s.local_get(O).call(to_string).call(concat);
    s.call(throw);
    s.end();
    as_obj(&mut s, O);
    s.local_get(SYM).ref_cast_non_null(string()).call(find);
    s.i32_const(0).i32_ge_s();
    s.end();
    f
}

fn private_add(cx: &mut dyn Cx, kind: PrivateKind) -> Function {
    let find = oh(cx, object::Helper::FindOwn);
    let define = oh(cx, object::Helper::Define);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    const O: u32 = 0;
    const SYM: u32 = 1;
    const V: u32 = 2;
    const MSG: u32 = 3;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    as_obj(&mut s, O);
    s.local_get(SYM).ref_cast_non_null(string()).call(find);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(MSG).call(throw);
    s.end();
    as_obj(&mut s, O);
    s.local_get(SYM).ref_cast_non_null(string());
    s.local_get(V);
    s.i32_const(
        HIDDEN
            | PRIVATE
            | match kind {
                PrivateKind::Field => 0,
                PrivateKind::Method => READONLY,
                PrivateKind::Accessor => ACCESSOR,
            },
    );
    s.call(define);
    s.end();
    f
}

/// OrdinarySet(home.[[Prototype]], key, v, this): a setter found on the chain is called with
/// `this`; a read-only property throws; otherwise `this` gets (or updates) an own data property.
fn super_set(cx: &mut dyn Cx) -> Function {
    let to_key = oh(cx, object::Helper::ToKey);
    let find = oh(cx, object::Helper::FindOwn);
    let define = oh(cx, object::Helper::Define);
    const HOME: u32 = 0;
    const KEY: u32 = 1;
    const V: u32 = 2;
    const THIS: u32 = 3;
    const K: u32 = 4;
    const P: u32 = 5;
    const E: u32 = 6;
    const A: u32 = 7;
    const X: u32 = 8;
    const O: u32 = 9;
    let mut f = Function::new([
        (1, rt(true, T_STRING)),
        (1, rt(true, T_OBJECT)),
        (2, ValType::I32),
        (1, jsval()),
        (1, rt(true, T_OBJECT)),
    ]);
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let (set_null, close) = (cx.str("Cannot set properties of null (setting '"), cx.str("')"));
    let mut s = f.instructions();
    // GetSuperBase before ToPropertyKey; a `null` base throws like `null.x = v` (T38, F168).
    as_obj(&mut s, HOME);
    s.struct_get(T_OBJECT, F_PROTO).local_set(P);
    s.local_get(KEY).call(to_key).local_set(K);
    s.local_get(P).ref_is_null();
    s.if_(BlockType::Empty);
    s.global_get(set_null);
    s.local_get(K).ref_as_non_null().call(concat);
    s.global_get(close).call(concat).call(throw);
    s.end();
    s.local_get(THIS).ref_cast_non_null(object()).local_set(O);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(P).ref_is_null().br_if(1);
    s.local_get(P)
        .ref_as_non_null()
        .local_get(K)
        .ref_as_non_null()
        .call(find)
        .local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    {
        s.local_get(P)
            .ref_as_non_null()
            .struct_get(T_OBJECT, F_ATTRS)
            .local_get(E)
            .array_get_u(T_BYTES)
            .local_tee(A);
        s.i32_const(ACCESSOR).i32_and();
        s.if_(BlockType::Empty);
        s.local_get(P)
            .ref_as_non_null()
            .struct_get(T_OBJECT, F_VALS)
            .local_get(E)
            .array_get(T_VALS)
            .local_set(X);
        call_setter_on(cx, &mut s, X, O, K, V);
        s.return_();
        s.end();
        s.local_get(A).i32_const(READONLY).i32_and();
        s.if_(BlockType::Empty);
        throw_read_only_nullable(cx, &mut s, O, K);
        s.end();
        s.br(2);
    }
    s.end();
    s.local_get(P)
        .ref_as_non_null()
        .struct_get(T_OBJECT, F_PROTO)
        .local_set(P);
    s.br(0).end().end();
    // An own data property of `this`.
    s.local_get(O)
        .ref_as_non_null()
        .local_get(K)
        .ref_as_non_null()
        .local_get(V)
        .i32_const(0)
        .call(define);
    s.end();
    f
}

/// [`call_setter`] with nullable locals for the object and the key.
fn call_setter_on(cx: &mut dyn Cx, s: &mut InstructionSink, acc: u32, o: u32, k: u32, v: u32) {
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let desc = ph(cx, Helper::Desc);
    let setter = cx.str("setter");
    let (a, b, c) = (
        cx.str("Cannot set property "),
        cx.str(" of "),
        cx.str(" which has only a getter"),
    );
    if cx.names("$module") {
        s.local_get(o).ref_as_non_null();
        is_class(s, object::CLASS_MODULE);
        s.if_(BlockType::Empty);
        throw_read_only_nullable(cx, s, o, k);
        s.end();
    }
    lazy_stack_set(cx, s, acc, o, k, v);
    s.local_get(acc)
        .ref_cast_non_null(accessor())
        .struct_get(T_ACCESSOR, 1)
        .global_get(G_UNDEFINED)
        .ref_eq();
    s.if_(BlockType::Empty);
    s.global_get(a);
    s.local_get(k).ref_as_non_null().call(concat);
    s.global_get(b).call(concat);
    s.local_get(o).ref_as_non_null().call(desc).call(concat);
    s.global_get(c).call(concat);
    s.call(throw);
    s.end();
    s.local_get(acc)
        .ref_cast_non_null(accessor())
        .struct_get(T_ACCESSOR, 1);
    s.local_get(o);
    s.local_get(v).array_new_fixed(T_VALS, 1);
    s.global_get(setter);
    s.call(call_value).drop();
}

/// [`throw_read_only`] with nullable locals.
fn throw_read_only_nullable(cx: &mut dyn Cx, s: &mut InstructionSink, o: u32, k: u32) {
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let desc = ph(cx, Helper::Desc);
    let (a, b, c) = (
        cx.str("Cannot assign to read only property '"),
        cx.str("' of object '"),
        cx.str("'"),
    );
    s.global_get(a);
    s.local_get(k).ref_as_non_null().call(concat);
    s.global_get(b).call(concat);
    s.local_get(o).ref_as_non_null().call(desc).call(concat);
    s.global_get(c).call(concat);
    s.call(throw);
}
