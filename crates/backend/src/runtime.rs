//! The JS value runtime on WasmGC (design D1), emitted only when a module uses `jsval`.
//!
//! - A JS value is `(ref null eq)`. `null` (the Wasm null) is never a JS value: it only fills
//!   locals before their first assignment.
//! - Numbers: `i31ref` when the value is an integer in [-2^30, 2^30) and not -0, otherwise a
//!   `$number` struct holding the `f64`.
//! - `undefined`, `null`, `false`, `true`: four `$oddball` singletons (immutable globals 0–3)
//!   whose field is the kind below. Identity (`ref.eq`) compares them.
//! - Types: one `rec` group `[$number, $oddball]` at type indices 0 and 1.
//!
//! The helpers only know these primitives. Strings (T09) and objects (T13) extend them.

use js2wasi3_ir::TypeofTag;
use wasm_encoder::{
    BlockType, CompositeInnerType, CompositeType, ConstExpr, FieldType, Function, GlobalSection,
    GlobalType, HeapType, Instruction, InstructionSink, RefType, StorageType, StructType, SubType,
    TypeSection, ValType,
};

pub const T_NUMBER: u32 = 0;
pub const T_ODDBALL: u32 = 1;
/// Number of runtime types before the function types.
pub const TYPES: u32 = 2;

pub const G_UNDEFINED: u32 = 0;
pub const G_NULL: u32 = 1;
pub const G_FALSE: u32 = 2;
pub const G_TRUE: u32 = 3;
/// Number of runtime globals before the user globals.
pub const GLOBALS: u32 = 4;

/// `$oddball` kinds: the global index of each singleton equals its kind.
const K_UNDEFINED: i32 = 0;
const K_NULL: i32 = 1;
const K_TRUE: i32 = 3;

/// Smallest and one past the largest integer kept as an `i31ref`.
const I31_MIN: i32 = -(1 << 30);
const I31_END: i32 = 1 << 30;

pub fn jsval() -> ValType {
    ValType::Ref(RefType::EQREF)
}

fn number() -> HeapType {
    HeapType::Concrete(T_NUMBER)
}

fn oddball() -> HeapType {
    HeapType::Concrete(T_ODDBALL)
}

/// The `typeof` code of a tag: its index in [`TypeofTag::ALL`].
pub fn typeof_code(tag: TypeofTag) -> i32 {
    TypeofTag::ALL.iter().position(|t| *t == tag).unwrap() as i32
}

/// Emit the `rec` group. Must be the first entries of the type section.
pub fn types(types: &mut TypeSection) {
    let st = |ty: ValType| SubType {
        is_final: true,
        supertype_idx: None,
        composite_type: CompositeType {
            inner: CompositeInnerType::Struct(StructType {
                fields: Box::new([FieldType {
                    element_type: StorageType::Val(ty),
                    mutable: false,
                }]),
            }),
            shared: false,
            descriptor: None,
            describes: None,
        },
    };
    types.ty().rec([st(ValType::F64), st(ValType::I32)]);
}

/// Emit the four singletons. Must be the first entries of the global section.
pub fn globals(globals: &mut GlobalSection) {
    for kind in 0..GLOBALS as i32 {
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
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        match self {
            Helper::Box => (vec![ValType::F64], vec![v]),
            Helper::ToNumber => (vec![v], vec![ValType::F64]),
            Helper::ToBoolean | Helper::IsNumber | Helper::IsNullish | Helper::Typeof => {
                (vec![v], vec![ValType::I32])
            }
            Helper::StrictEq | Helper::LooseEq => (vec![v, v], vec![ValType::I32]),
        }
    }

    /// The helper's body. `index` gives the function index of another helper.
    pub fn body(self, index: &mut dyn FnMut(Helper) -> u32) -> Function {
        match self {
            Helper::Box => box_number(),
            Helper::ToNumber => to_number(),
            Helper::ToBoolean => to_boolean(),
            Helper::IsNumber => is_number(),
            Helper::IsNullish => is_nullish(),
            Helper::StrictEq => strict_eq(index(Helper::IsNumber), index(Helper::ToNumber)),
            Helper::LooseEq => loose_eq(index(Helper::IsNullish), index(Helper::ToNumber)),
            Helper::Typeof => typeof_(index(Helper::IsNumber)),
        }
    }
}

fn i31() -> HeapType {
    HeapType::I31
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

/// `$to_number(v: jsval) -> f64`: JS ToNumber on primitives.
fn to_number() -> Function {
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
    kind(&mut s, V);
    s.i32_const(K_TRUE).i32_eq();
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

/// `$strict_eq(a, b) -> i32`: JS `===`. Numbers compare by value (NaN ≠ NaN, +0 = -0),
/// everything else by identity.
fn strict_eq(is_number: u32, to_number: u32) -> Function {
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
    s.local_get(0).local_get(1).ref_eq();
    s.end();
    s.end();
    f
}

/// `$loose_eq(a, b) -> i32`: JS `==` on primitives. `null == undefined`; a nullish value
/// equals nothing else; numbers and booleans compare as numbers.
fn loose_eq(is_nullish: u32, to_number: u32) -> Function {
    const NA: u32 = 2;
    const NB: u32 = 3;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(0).call(is_nullish).local_set(NA);
    s.local_get(1).call(is_nullish).local_set(NB);
    s.local_get(NA).local_get(NB).i32_or();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(NA).local_get(NB).i32_and();
    s.else_();
    s.local_get(0).call(to_number);
    s.local_get(1).call(to_number);
    s.f64_eq();
    s.end();
    s.end();
    f
}

/// `$typeof(v) -> i32`: a [`typeof_code`].
fn typeof_(is_number: u32) -> Function {
    const V: u32 = 0;
    const K: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(V).call(is_number);
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(typeof_code(TypeofTag::Number));
    s.else_();
    // Oddball kinds 0..=3 map to undefined, object (null), boolean, boolean: kind - (kind == 3).
    const _: () = assert!(K_UNDEFINED == 0 && K_NULL == 1 && K_TRUE == 3);
    kind(&mut s, V);
    s.local_tee(K);
    s.local_get(K).i32_const(K_TRUE).i32_eq();
    s.i32_sub();
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
        assert_eq!(typeof_code(TypeofTag::Boolean), 2);
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
