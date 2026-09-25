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
//!   $i32s]` at type indices 0–8 (`$i64s` holds the number → string tables, see [`crate::numstr`];
//!   the last four are an object's arrays).
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

use crate::{abi, host, numstr, object};

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
/// Number of runtime types before the function types.
pub const TYPES: u32 = 9;

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
/// Number of runtime globals before the user globals.
pub const GLOBALS: u32 = G_PROTOS + js2wasi3_ir::Builtin::ALL.len() as u32;

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
    /// Function index of the command's `main`.
    fn main_func(&mut self) -> u32;

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

/// Emit the `rec` group. Must be the first entries of the type section.
pub fn types(types: &mut TypeSection) {
    let sub = |inner| SubType {
        is_final: true,
        supertype_idx: None,
        composite_type: CompositeType {
            inner,
            shared: false,
            descriptor: None,
            describes: None,
        },
    };
    let field = |ty: StorageType, mutable| FieldType {
        element_type: ty,
        mutable,
    };
    let st = |ty: ValType| {
        sub(CompositeInnerType::Struct(StructType {
            fields: Box::new([field(StorageType::Val(ty), false)]),
        }))
    };
    types.ty().rec([
        st(ValType::F64),
        st(ValType::I32),
        sub(CompositeInnerType::Array(ArrayType(field(
            StorageType::I16,
            true,
        )))),
        sub(CompositeInnerType::Array(ArrayType(field(
            StorageType::Val(ValType::I64),
            false,
        )))),
        // $object: class, proto, keys, vals, attrs, count, index (see `object.rs`).
        sub(CompositeInnerType::Struct(StructType {
            fields: Box::new([
                field(StorageType::Val(ValType::I32), false),
                field(StorageType::Val(rt(true, T_OBJECT)), true),
                field(StorageType::Val(rt(false, T_KEYS)), true),
                field(StorageType::Val(rt(false, T_VALS)), true),
                field(StorageType::Val(rt(false, T_BYTES)), true),
                field(StorageType::Val(ValType::I32), true),
                field(StorageType::Val(rt(true, T_I32S)), true),
            ]),
        })),
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
    ]);
}

/// Emit the runtime globals. Must be the first entries of the global section.
pub fn globals(globals: &mut GlobalSection) {
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
}

/// A string constant's global: `(global (ref $string) (array.new_fixed $string n ...))`.
pub fn string_global(globals: &mut GlobalSection, units: &[u16]) {
    let mut insns: Vec<Instruction> = units
        .iter()
        .map(|u| Instruction::I32Const(*u as i32))
        .collect();
    insns.push(Instruction::ArrayNewFixed {
        array_type_index: T_STRING,
        array_size: units.len() as u32,
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
    Typeof,
    TypeofString,
    StrEq,
    Concat,
    ToString,
    JsAdd,
    Inspect,
    NoFormat,
    /// `(v: jsval, prop: (ref $string))`: throw the TypeError for reading `prop` of `undefined`/`null`.
    ThrowRead,
    /// `(v: jsval) -> (ref $string)`: how an uncaught exception is shown: ToString for an
    /// error, the `console.log` text otherwise.
    Describe,
    Num(numstr::Helper),
    Obj(object::Helper),
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
            Helper::Typeof => "typeof",
            Helper::TypeofString => "typeof_string",
            Helper::StrEq => "string_eq",
            Helper::Concat => "concat",
            Helper::ToString => "to_string",
            Helper::JsAdd => "js_add",
            Helper::Inspect => "inspect",
            Helper::NoFormat => "no_format",
            Helper::ThrowRead => "throw_read_error",
            Helper::Describe => "describe",
            Helper::Num(h) => h.name(),
            Helper::Obj(h) => h.name(),
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
            Helper::TypeofString | Helper::ToString => (vec![v], vec![s]),
            Helper::StrEq => (vec![s, s], vec![ValType::I32]),
            Helper::Concat => (vec![s, s], vec![s]),
            Helper::JsAdd => (vec![v, v], vec![v]),
            Helper::NoFormat => (vec![v], vec![v]),
            Helper::Inspect | Helper::Describe => (vec![v], vec![s]),
            Helper::ThrowRead => (vec![v, s], vec![]),
            Helper::Num(h) => h.signature(),
            Helper::Obj(h) => h.signature(),
            Helper::Abi(h) => h.signature(),
            Helper::Host(h) => h.signature(),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Box => box_number(),
            Helper::ToNumber => to_number(cx),
            Helper::ToBoolean => to_boolean(),
            Helper::IsNumber => is_number(),
            Helper::IsNullish => is_nullish(),
            Helper::StrictEq => strict_eq(cx),
            Helper::LooseEq => loose_eq(cx),
            Helper::Typeof => typeof_(cx),
            Helper::TypeofString => typeof_string(cx),
            Helper::StrEq => str_eq(),
            Helper::Concat => concat(),
            Helper::ToString => to_string(cx),
            Helper::JsAdd => js_add(cx),
            Helper::Inspect => inspect(cx),
            Helper::NoFormat => no_format(),
            Helper::ThrowRead => throw_read(cx),
            Helper::Describe => describe(cx),
            Helper::Num(h) => h.body(cx),
            Helper::Obj(h) => h.body(cx),
            Helper::Host(h) => {
                let (base, main) = (cx.host_globals(), cx.main_func());
                h.body(cx, base, main)
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
    // ToNumber of a string (parsing) isn't implemented yet: trap (F20).
    s.local_get(V).ref_test_non_null(string());
    s.if_(BlockType::Empty);
    s.unreachable();
    s.end();
    // An object's ToPrimitive is a string: "" is 0; one starting with `[` ("[object Object]") or a
    // letter other than `I` (an error's "Name: message") can't be numeric, so NaN. Others: F20.
    s.local_get(V).ref_test_non_null(object());
    s.if_(BlockType::Empty);
    s.local_get(V)
        .ref_cast_non_null(object())
        .call(to_primitive_num);
    s.return_();
    s.end();
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
fn to_boolean() -> Function {
    const V: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(i31());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(V).ref_cast_non_null(i31()).i31_get_s();
    s.i32_const(0).i32_ne();
    s.else_();
    s.local_get(V).ref_test_non_null(number());
    s.if_(BlockType::Result(ValType::I32));
    // false for ±0 and NaN.
    s.local_get(V)
        .ref_cast_non_null(number())
        .struct_get(T_NUMBER, 0)
        .f64_abs()
        .f64_const(0.0.into())
        .f64_gt();
    s.else_();
    s.local_get(V).ref_test_non_null(string());
    s.if_(BlockType::Result(ValType::I32));
    // false only for "".
    as_string(&mut s, V);
    s.array_len().i32_const(0).i32_ne();
    s.else_();
    // Objects are truthy.
    s.local_get(V).ref_test_non_null(object());
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(1);
    s.else_();
    kind(&mut s, V);
    s.i32_const(K_TRUE).i32_eq();
    s.end();
    s.end();
    s.end();
    s.end();
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
    s.local_get(a).ref_test_non_null(string());
    s.local_get(b).ref_test_non_null(string());
    s.i32_and();
}

/// `$strict_eq(a, b) -> i32`: JS `===`. Numbers compare by value (NaN ≠ NaN, +0 = -0),
/// strings by content, everything else by identity.
fn strict_eq(cx: &mut dyn Cx) -> Function {
    let is_number = cx.helper(Helper::IsNumber);
    let to_number = cx.helper(Helper::ToNumber);
    let str_eq = cx.helper(Helper::StrEq);
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).call(is_number);
    s.local_get(1).call(is_number);
    s.i32_and();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(0).call(to_number);
    s.local_get(1).call(to_number);
    s.f64_eq();
    s.else_();
    both_strings(&mut s, 0, 1);
    s.if_(BlockType::Result(ValType::I32));
    as_string(&mut s, 0);
    as_string(&mut s, 1);
    s.call(str_eq);
    s.else_();
    s.local_get(0).local_get(1).ref_eq();
    s.end();
    s.end();
    s.end();
    f
}

/// `$loose_eq(a, b) -> i32`: JS `==`. Two objects compare by identity, one object as its
/// ToPrimitive (a string). Then `null == undefined`; a nullish value equals nothing else; two strings compare
/// by content; anything else compares as numbers.
fn loose_eq(cx: &mut dyn Cx) -> Function {
    let is_nullish = cx.helper(Helper::IsNullish);
    let to_number = cx.helper(Helper::ToNumber);
    let str_eq = cx.helper(Helper::StrEq);
    const NA: u32 = 2;
    const NB: u32 = 3;
    let to_string = cx.helper(Helper::ToString);
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    // Two objects compare by identity; one object compares as its ToPrimitive, a string.
    s.local_get(0).ref_test_non_null(object());
    s.local_get(1).ref_test_non_null(object());
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(0).local_get(1).ref_eq().return_();
    s.end();
    for v in [0, 1] {
        s.local_get(v).ref_test_non_null(object());
        s.if_(BlockType::Empty);
        s.local_get(v).call(to_string).local_set(v);
        s.end();
    }
    s.local_get(0).call(is_nullish).local_set(NA);
    s.local_get(1).call(is_nullish).local_set(NB);
    s.local_get(NA).local_get(NB).i32_or();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(NA).local_get(NB).i32_and();
    s.else_();
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
    let is_number = cx.helper(Helper::IsNumber);
    const V: u32 = 0;
    const K: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(V).call(is_number);
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(typeof_code(TypeofTag::Number));
    s.else_();
    s.local_get(V).ref_test_non_null(string());
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(typeof_code(TypeofTag::String));
    s.else_();
    s.local_get(V).ref_test_non_null(object());
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(typeof_code(TypeofTag::Object));
    s.else_();
    // Oddball kinds 0..=3 map to undefined, object (null), boolean, boolean: kind - (kind == 3).
    const _: () = assert!(K_UNDEFINED == 0 && K_NULL == 1 && K_FALSE == 2 && K_TRUE == 3);
    kind(&mut s, V);
    s.local_tee(K);
    s.local_get(K).i32_const(K_TRUE).i32_eq();
    s.i32_sub();
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

/// `$to_string(v: jsval) -> (ref $string)`: JS ToString (objects: their ToPrimitive).
fn to_string(cx: &mut dyn Cx) -> Function {
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
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(string());
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
    s.local_get(V).ref_test_non_null(object());
    s.if_(BlockType::Empty);
    s.local_get(V)
        .ref_cast_non_null(object())
        .call(to_primitive)
        .return_();
    s.end();
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

/// `$js_add(a, b: jsval) -> jsval`: JS `+`. If either is a string or an object (whose
/// ToPrimitive is a string), concatenate the ToString of both; otherwise add the ToNumber of both.
fn js_add(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(Helper::ToString);
    let concat = cx.helper(Helper::Concat);
    let to_number = cx.helper(Helper::ToNumber);
    let box_ = cx.helper(Helper::Box);
    let mut f = Function::new([]);
    let mut s = f.instructions();
    // An object's ToPrimitive is a string too.
    for v in [0, 1] {
        s.local_get(v).ref_test_non_null(string());
        s.local_get(v).ref_test_non_null(object());
        s.i32_or();
    }
    s.i32_or();
    s.if_(BlockType::Result(jsval()));
    s.local_get(0).call(to_string);
    s.local_get(1).call(to_string);
    s.call(concat);
    s.else_();
    s.local_get(0).call(to_number);
    s.local_get(1).call(to_number);
    s.f64_add().call(box_);
    s.end();
    s.end();
    f
}

/// `$inspect(v) -> (ref $string)`: how `console.log` shows a primitive: ToString, except
/// that `-0` shows as `-0`. An error shows as `[<ToString>]` (node, for an error without a stack).
/// Other objects trap (`inspect_object`): node's `util.inspect` format isn't implemented (F24).
fn inspect(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(Helper::ToString);
    let concat = cx.helper(Helper::Concat);
    let trap = cx.helper(Helper::Obj(object::Helper::InspectTrap));
    let minus_zero = cx.str("-0");
    let (open, close) = (cx.str("["), cx.str("]"));
    const V: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(object());
    s.if_(BlockType::Empty);
    object::is_error(&mut s, V);
    s.if_(BlockType::Empty);
    s.global_get(open);
    s.local_get(V).call(to_string).call(concat);
    s.global_get(close).call(concat).return_();
    s.end();
    s.call(trap);
    s.end();
    // -0 is never an i31.
    s.local_get(V).ref_test_non_null(number());
    s.if_(BlockType::Empty);
    s.local_get(V)
        .ref_cast_non_null(number())
        .struct_get(T_NUMBER, 0)
        .i64_reinterpret_f64()
        .i64_const(i64::MIN)
        .i64_eq();
    s.if_(BlockType::Empty);
    s.global_get(minus_zero).return_();
    s.end();
    s.end();
    s.local_get(V).call(to_string);
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
    s.local_get(V).ref_test_non_null(string());
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

/// `$describe(v) -> (ref $string)`: see [`Helper::Describe`].
fn describe(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(Helper::ToString);
    let inspect = cx.helper(Helper::Inspect);
    const V: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    object::is_error(&mut s, V);
    s.if_(BlockType::Result(string_ref()));
    s.local_get(V).call(to_string);
    s.else_();
    s.local_get(V).call(inspect);
    s.end();
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
