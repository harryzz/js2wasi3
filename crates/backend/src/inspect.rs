//! `console.log` formatting (node's `util.inspect` with default options), T15 subset.
//!
//! [`Helper::Format`] `(v, depth, indent, nested)` follows node's `formatValue`:
//! - primitives: ToString (`-0` as `-0`); strings are quoted when `nested` (inside an array or
//!   object), with node's choice of `'`, `"` or `` ` ``; a string that would need escapes traps.
//! - symbols: `Symbol(description)`; functions: `[Function: name]` / `[Function (anonymous)]`.
//! - arrays and plain objects (prototype `Object.prototype`): entries formatted with `depth + 1` and
//!   `indent + 2`; past depth 2 they're `[Array]` / `[Object]`. Consecutive holes are
//!   `<n empty items>`. The layout is node's `reduceToSingleString`: one line if it fits in 80
//!   columns, otherwise one entry per line; arrays with more than 6 entries are grouped in columns
//!   first (`groupArrayElements`).
//! - errors: `[name: message]` at the top level (node, without a stack).
//!
//! Anything else traps (`inspect_object_not_implemented`, F24): other objects, arrays with extra
//! properties or more than 100 elements, objects with symbol keys, nested errors.

use wasm_encoder::{BlockType, Function, InstructionSink, ValType};

use crate::array::{self, F_ELEMS, F_LENGTH};
use crate::runtime::{
    self, Cx, T_ARRAY, T_KEYS, T_NUMBER, T_STRING, T_VALS, jsval, rt, string, string_ref,
};
use crate::{func, object, props};

const BREAK_LENGTH: i32 = 80;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(v, depth: i32, indent: i32, nested: i32) -> (ref $string)`.
    Format,
    /// `(a: (ref $array), depth, indent) -> (ref $string)`.
    Array,
    /// `(o: (ref $object), depth, indent, prefix: (ref null $string)) -> (ref $string)`: `prefix`
    /// (a function's `[Function: f] `) replaces the constructor-name prefix (`Point `).
    Object,
    /// `(s: (ref $string)) -> (ref $string)`: a string in quotes (traps if it needs escapes).
    Quote,
    /// `(k: (ref $string)) -> (ref $string)`: a property key: as is if it's an identifier, else quoted.
    Key,
    /// `(entries: (ref $keys), n: i32, indent: i32, open, close: (ref $string), array: i32,
    /// numbers: i32) -> (ref $string)`: node's `reduceToSingleString`.
    Layout,
    /// `(entries, n, indent, numbers) -> (ref null $keys)`: node's `groupArrayElements`; null if
    /// the entries aren't grouped.
    Group,
    /// `(n: i32) -> (ref $string)`: `n` spaces.
    Spaces,
    /// `(s, width: i32, start: i32) -> (ref $string)`: `padStart` (start = 1) / `padEnd` with spaces.
    Pad,
    /// `(entries, n, sep) -> (ref $string)`.
    Join,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Format => "inspect_format",
            Helper::Array => "inspect_array",
            Helper::Object => "inspect_object",
            Helper::Quote => "inspect_quote",
            Helper::Key => "inspect_key",
            Helper::Layout => "inspect_layout",
            Helper::Group => "inspect_group",
            Helper::Spaces => "spaces",
            Helper::Pad => "pad",
            Helper::Join => "join_strings",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::I32;
        let v = jsval();
        let s = string_ref();
        let k = rt(false, T_KEYS);
        match self {
            Helper::Format => (vec![v, I32, I32, I32], vec![s]),
            Helper::Array => (vec![rt(false, T_ARRAY), I32, I32], vec![s]),
            Helper::Object => (
                vec![rt(false, runtime::T_OBJECT), I32, I32, rt(true, T_STRING)],
                vec![s],
            ),
            Helper::Quote | Helper::Key => (vec![s], vec![s]),
            Helper::Layout => (vec![k, I32, I32, s, s, I32, I32], vec![s]),
            Helper::Group => (vec![k, I32, I32, I32], vec![rt(true, T_KEYS)]),
            Helper::Spaces => (vec![I32], vec![s]),
            Helper::Pad => (vec![s, I32, I32], vec![s]),
            Helper::Join => (vec![k, I32, s], vec![s]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Format => format(cx),
            Helper::Array => format_array(cx),
            Helper::Object => format_object(cx),
            Helper::Quote => quote(cx),
            Helper::Key => key(cx),
            Helper::Layout => layout(cx),
            Helper::Group => group(cx),
            Helper::Spaces => spaces(),
            Helper::Pad => pad(cx),
            Helper::Join => join(cx),
        }
    }
}

fn ih(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Fmt(h))
}

fn oh(cx: &mut dyn Cx, h: object::Helper) -> u32 {
    cx.helper(runtime::Helper::Obj(h))
}

fn trap(cx: &mut dyn Cx, s: &mut InstructionSink) {
    let t = oh(cx, object::Helper::InspectTrap);
    s.call(t);
}

fn format(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(runtime::Helper::ToString);
    let concat = cx.helper(runtime::Helper::Concat);
    let quote = ih(cx, Helper::Quote);
    let f_array = ih(cx, Helper::Array);
    let f_object = ih(cx, Helper::Object);
    let own_keys = oh(cx, object::Helper::OwnKeys);
    let get = oh(cx, object::Helper::Get);
    let minus_zero = cx.str("-0");
    let (open, close) = (cx.str("["), cx.str("]"));
    let (sym_open, sym_close) = (cx.str("Symbol("), cx.str(")"));
    let (name, named, anonymous) = (
        cx.str("name"),
        cx.str("[Function: "),
        cx.str("[Function (anonymous)]"),
    );
    let (class_open, anon_class, extends, space) = (
        cx.str("[class "),
        cx.str("(anonymous)"),
        cx.str(" extends "),
        cx.str(" "),
    );
    let empty = cx.str("");
    const V: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const NESTED: u32 = 3;
    const N: u32 = 4;
    const P: u32 = 5;
    const B: u32 = 6;
    let mut f = Function::new([
        (1, jsval()),
        (1, rt(true, runtime::T_OBJECT)),
        (1, rt(true, T_STRING)),
    ]);
    let mut s = f.instructions();
    runtime::is_string(&mut s, V);
    s.if_(BlockType::Empty);
    s.local_get(V).ref_cast_non_null(string());
    s.local_get(NESTED);
    s.if_(BlockType::Result(string_ref()));
    s.local_get(V).ref_cast_non_null(string()).call(quote);
    s.else_();
    s.local_get(V).ref_cast_non_null(string());
    s.end();
    s.return_();
    s.end();
    s.local_get(V).ref_test_non_null(runtime::symbol());
    s.if_(BlockType::Empty);
    s.global_get(sym_open);
    s.local_get(V).ref_cast_non_null(string()).call(concat);
    s.global_get(sym_close).call(concat).return_();
    s.end();
    // A function: `[Function: name]` / `[class Name extends Parent]`, then its own enumerable
    // properties like an object's.
    s.local_get(V).ref_test_non_null(func::function());
    s.if_(BlockType::Empty);
    {
        s.local_get(V)
            .ref_cast_non_null(func::function())
            .struct_get(runtime::T_FUNCTION, func::F_KIND)
            .i32_const(func::KIND_CLASS)
            .i32_eq();
        s.if_(BlockType::Result(string_ref()));
        {
            s.global_get(class_open);
            s.local_get(V).global_get(name).call(get).local_set(N);
            runtime::is_string(&mut s, N);
            s.if_(BlockType::Result(ValType::I32));
            s.local_get(N).ref_cast_non_null(string()).array_len();
            s.else_();
            s.i32_const(0);
            s.end();
            s.if_(BlockType::Result(string_ref()));
            s.local_get(N).ref_cast_non_null(string());
            s.else_();
            s.global_get(anon_class);
            s.end();
            s.call(concat);
            // Derived: ` extends Parent`.
            s.local_get(V)
                .ref_cast_non_null(runtime::object())
                .struct_get(runtime::T_OBJECT, object::F_PROTO)
                .local_tee(P);
            s.ref_test_non_null(func::function());
            s.if_(BlockType::Result(string_ref()));
            s.global_get(extends);
            s.local_get(P).global_get(name).call(get).local_set(N);
            runtime::is_string(&mut s, N);
            s.if_(BlockType::Result(string_ref()));
            s.local_get(N).ref_cast_non_null(string());
            s.else_();
            s.global_get(anon_class);
            s.end();
            s.call(concat);
            s.else_();
            s.global_get(empty);
            s.end();
            s.call(concat);
            s.global_get(close).call(concat);
        }
        s.else_();
        {
            s.local_get(V).global_get(name).call(get).local_set(N);
            runtime::is_string(&mut s, N);
            s.if_(BlockType::Result(ValType::I32));
            s.local_get(N).ref_cast_non_null(string()).array_len();
            s.else_();
            s.i32_const(0);
            s.end();
            s.if_(BlockType::Result(string_ref()));
            s.global_get(named);
            s.local_get(N).ref_cast_non_null(string()).call(concat);
            s.global_get(close).call(concat);
            s.else_();
            s.global_get(anonymous);
            s.end();
        }
        s.end();
        s.local_set(B);
        s.local_get(V)
            .ref_cast_non_null(runtime::object())
            .call(own_keys)
            .array_len()
            .i32_const(0)
            .i32_ne();
        s.local_get(DEPTH).i32_const(2).i32_le_s();
        s.i32_and();
        s.if_(BlockType::Result(string_ref()));
        s.local_get(V).ref_cast_non_null(runtime::object());
        s.local_get(DEPTH).local_get(INDENT);
        s.local_get(B)
            .ref_as_non_null()
            .global_get(space)
            .call(concat);
        s.call(f_object);
        s.else_();
        s.local_get(B).ref_as_non_null();
        s.end();
        s.return_();
    }
    s.end();
    s.local_get(V).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    s.local_get(V).ref_cast_non_null(array::array());
    s.local_get(DEPTH).local_get(INDENT).call(f_array).return_();
    s.end();
    s.local_get(V).ref_test_non_null(runtime::object());
    s.if_(BlockType::Empty);
    object::is_error(&mut s, V);
    s.if_(BlockType::Empty);
    // An error inside an array or object: node prints its stack (F31).
    s.local_get(NESTED);
    s.if_(BlockType::Empty);
    trap(cx, &mut s);
    s.end();
    s.global_get(open);
    s.local_get(V).call(to_string).call(concat);
    s.global_get(close).call(concat).return_();
    s.end();
    s.local_get(V).ref_cast_non_null(runtime::object());
    s.local_get(DEPTH).local_get(INDENT);
    s.ref_null(string());
    s.call(f_object).return_();
    s.end();
    // -0 is never an i31.
    s.local_get(V)
        .ref_test_non_null(wasm_encoder::HeapType::Concrete(T_NUMBER));
    s.if_(BlockType::Empty);
    s.local_get(V)
        .ref_cast_non_null(wasm_encoder::HeapType::Concrete(T_NUMBER))
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

fn format_array(cx: &mut dyn Cx) -> Function {
    let format = ih(cx, Helper::Format);
    let layout = ih(cx, Helper::Layout);
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let is_number = cx.helper(runtime::Helper::IsNumber);
    let own_keys = oh(cx, object::Helper::OwnKeys);
    let index_keys = array::ah(cx, array::Helper::IndexKeys);
    let symbols = oh(cx, object::Helper::SymbolKeys);
    let (open, close, deep) = (cx.str("["), cx.str("]"), cx.str("[Array]"));
    let (lt, item, items) = (cx.str("<"), cx.str(" empty item>"), cx.str(" empty items>"));
    const A: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const LEN: u32 = 3;
    const I: u32 = 4;
    const J: u32 = 5;
    const N: u32 = 6;
    const NUMBERS: u32 = 7;
    const OUT: u32 = 8;
    const E: u32 = 9;
    let mut f = Function::new([(5, ValType::I32), (1, rt(true, T_KEYS)), (1, jsval())]);
    let mut s = f.instructions();
    // Too deep (node: only when there is something to show).
    s.local_get(DEPTH).i32_const(2).i32_gt_s();
    s.local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i32_const(0)
        .i32_ne();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.global_get(deep).return_();
    s.end();
    s.local_get(A).struct_get(T_ARRAY, F_LENGTH).local_tee(LEN);
    // More than 100 elements ("... n more items") or extra properties: not implemented (F24).
    s.i32_const(100).i32_gt_u();
    s.local_get(A).call(own_keys).array_len();
    s.local_get(A).call(index_keys).array_len();
    s.i32_ne().i32_or();
    s.local_get(A).call(symbols).i32_or();
    s.if_(BlockType::Empty);
    trap(cx, &mut s);
    s.end();
    s.local_get(LEN).array_new_default(T_KEYS).local_set(OUT);
    s.i32_const(1).local_set(NUMBERS);
    let elem = |s: &mut InstructionSink, idx: u32| {
        s.local_get(A)
            .struct_get(T_ARRAY, F_ELEMS)
            .local_get(idx)
            .array_get(T_VALS);
    };
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    elem(&mut s, I);
    s.local_tee(E).ref_is_null();
    s.if_(BlockType::Empty);
    {
        // A run of holes: `<k empty item(s)>`.
        s.local_get(I).local_set(J);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(J).local_get(LEN).i32_ge_u().br_if(1);
        elem(&mut s, J);
        s.ref_is_null().i32_eqz().br_if(1);
        s.local_get(J).i32_const(1).i32_add().local_set(J);
        s.br(0).end().end();
        s.i32_const(0).local_set(NUMBERS);
        s.local_get(OUT).local_get(N);
        s.global_get(lt);
        s.local_get(J)
            .local_get(I)
            .i32_sub()
            .f64_convert_i32_u()
            .call(number_to_string);
        s.call(concat);
        s.local_get(J).local_get(I).i32_sub().i32_const(1).i32_eq();
        s.if_(BlockType::Result(string_ref()));
        s.global_get(item);
        s.else_();
        s.global_get(items);
        s.end();
        s.call(concat);
        s.array_set(T_KEYS);
        s.local_get(J).local_set(I);
    }
    s.else_();
    {
        s.local_get(E).call(is_number).i32_eqz();
        s.if_(BlockType::Empty);
        s.i32_const(0).local_set(NUMBERS);
        s.end();
        s.local_get(OUT).local_get(N);
        s.local_get(E);
        s.local_get(DEPTH).i32_const(1).i32_add();
        s.local_get(INDENT).i32_const(2).i32_add();
        s.i32_const(1).call(format);
        s.array_set(T_KEYS);
        s.local_get(I).i32_const(1).i32_add().local_set(I);
    }
    s.end();
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.br(0).end().end();
    s.local_get(OUT)
        .ref_as_non_null()
        .local_get(N)
        .local_get(INDENT);
    s.global_get(open).global_get(close);
    s.i32_const(1).local_get(NUMBERS).call(layout);
    s.end();
    f
}

fn format_object(cx: &mut dyn Cx) -> Function {
    let format = ih(cx, Helper::Format);
    let layout = ih(cx, Helper::Layout);
    let key = ih(cx, Helper::Key);
    let concat = cx.helper(runtime::Helper::Concat);
    let own_keys = oh(cx, object::Helper::OwnKeys);
    let own = oh(cx, object::Helper::OwnGet);
    let obj_proto = oh(cx, object::Helper::Proto(js2wasi3_ir::Builtin::Object));
    let symbols = oh(cx, object::Helper::SymbolKeys);
    let ctor_name = cx.helper(runtime::Helper::Prop(props::Helper::CtorName));
    let (open, close, deep, colon) = (cx.str("{"), cx.str("}"), cx.str("[Object]"), cx.str(": "));
    let (null_proto, deep_null) = (
        cx.str("[Object: null prototype] "),
        cx.str("[Object: null prototype]"),
    );
    let (lbracket, rbracket, space, object_) =
        (cx.str("["), cx.str("]"), cx.str(" "), cx.str("Object"));
    let (getter, setter, both) = (
        cx.str("[Getter]"),
        cx.str("[Setter]"),
        cx.str("[Getter/Setter]"),
    );
    let str_eq = cx.helper(runtime::Helper::StrEq);
    const O: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const PREFIX: u32 = 3;
    const KS: u32 = 4;
    const N: u32 = 5;
    const I: u32 = 6;
    const OUT: u32 = 7;
    const NAME: u32 = 8;
    const Q: u32 = 9;
    const R: u32 = 10;
    const PLAIN: u32 = 11;
    let mut f = Function::new([
        (1, rt(true, T_KEYS)),
        (2, ValType::I32),
        (1, rt(true, T_KEYS)),
        (1, rt(true, T_STRING)),
        (1, rt(true, runtime::T_OBJECT)),
        (1, jsval()),
        (1, ValType::I32),
    ]);
    let mut s = f.instructions();
    // Symbol keys aren't shown yet (F24).
    s.local_get(O).call(symbols);
    s.if_(BlockType::Empty);
    trap(cx, &mut s);
    s.end();
    // The prefix: `Name ` from the constructor on the chain; none for `Object`; for no constructor,
    // `[Object: null prototype] ` unless the chain reaches Object.prototype.
    s.local_get(PREFIX).ref_is_null();
    s.if_(BlockType::Empty);
    {
        s.local_get(O).call(ctor_name).local_tee(NAME).ref_is_null();
        s.if_(BlockType::Empty);
        {
            s.local_get(O).local_set(Q);
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(Q).ref_is_null().br_if(1);
            s.local_get(Q).call(obj_proto).ref_eq();
            s.if_(BlockType::Empty);
            s.i32_const(1).local_set(PLAIN);
            s.br(2);
            s.end();
            s.local_get(Q)
                .ref_as_non_null()
                .struct_get(runtime::T_OBJECT, object::F_PROTO)
                .local_set(Q);
            s.br(0).end().end();
            s.local_get(PLAIN).i32_eqz();
            s.if_(BlockType::Empty);
            s.global_get(null_proto).local_set(PREFIX);
            s.end();
        }
        s.else_();
        {
            s.local_get(NAME)
                .ref_as_non_null()
                .global_get(object_)
                .call(str_eq)
                .i32_eqz();
            s.if_(BlockType::Empty);
            s.local_get(NAME)
                .ref_as_non_null()
                .global_get(space)
                .call(concat)
                .local_set(PREFIX);
            s.end();
        }
        s.end();
    }
    s.end();
    // Too deep (node: only when there is something to show).
    s.local_get(DEPTH).i32_const(2).i32_gt_s();
    s.local_get(O)
        .call(own_keys)
        .array_len()
        .i32_const(0)
        .i32_ne();
    s.i32_and();
    s.if_(BlockType::Empty);
    {
        s.local_get(PREFIX).ref_is_null();
        s.if_(BlockType::Empty);
        s.global_get(deep).return_();
        s.end();
        s.local_get(PREFIX).global_get(null_proto).ref_eq();
        s.if_(BlockType::Empty);
        s.global_get(deep_null).return_();
        s.end();
        // (A function's own properties aren't shown that deep: `format` stops first.)
        s.local_get(NAME).ref_is_null();
        s.if_(BlockType::Empty);
        s.global_get(deep).return_();
        s.end();
        s.global_get(lbracket);
        s.local_get(NAME).ref_as_non_null().call(concat);
        s.global_get(rbracket).call(concat).return_();
    }
    s.end();
    s.local_get(O)
        .call(own_keys)
        .local_tee(KS)
        .array_len()
        .local_tee(N);
    s.array_new_default(T_KEYS).local_set(OUT);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(OUT).local_get(I);
    s.local_get(KS)
        .local_get(I)
        .array_get(T_KEYS)
        .ref_as_non_null()
        .call(key);
    s.global_get(colon).call(concat);
    s.local_get(O);
    s.local_get(KS)
        .local_get(I)
        .array_get(T_KEYS)
        .ref_as_non_null()
        .call(own)
        .local_tee(R);
    s.ref_test_non_null(props::accessor());
    s.if_(BlockType::Result(string_ref()));
    {
        // `[Getter]`, `[Setter]` or `[Getter/Setter]`.
        s.global_get(getter).global_get(setter);
        s.local_get(R)
            .ref_cast_non_null(props::accessor())
            .struct_get(runtime::T_ACCESSOR, 0)
            .global_get(runtime::G_UNDEFINED)
            .ref_eq()
            .i32_eqz();
        s.typed_select(string_ref());
        s.global_get(both);
        s.local_get(R)
            .ref_cast_non_null(props::accessor())
            .struct_get(runtime::T_ACCESSOR, 0)
            .global_get(runtime::G_UNDEFINED)
            .ref_eq()
            .local_get(R)
            .ref_cast_non_null(props::accessor())
            .struct_get(runtime::T_ACCESSOR, 1)
            .global_get(runtime::G_UNDEFINED)
            .ref_eq()
            .i32_or();
        s.typed_select(string_ref());
    }
    s.else_();
    s.local_get(R);
    s.local_get(DEPTH).i32_const(1).i32_add();
    s.local_get(INDENT).i32_const(2).i32_add();
    s.i32_const(1).call(format);
    s.end();
    s.call(concat);
    s.array_set(T_KEYS);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(OUT)
        .ref_as_non_null()
        .local_get(N)
        .local_get(INDENT);
    s.local_get(PREFIX).ref_is_null();
    s.if_(BlockType::Result(string_ref()));
    s.global_get(open);
    s.else_();
    s.local_get(PREFIX)
        .ref_as_non_null()
        .global_get(open)
        .call(concat);
    s.end();
    s.global_get(close);
    s.i32_const(0).i32_const(0).call(layout);
    s.end();
    let _ = lbracket;
    f
}

/// node's `strEscape` when nothing needs escaping: `'s'`, else `"s"` if `s` has a `'` but no `"`,
/// else `` `s` `` if it has no `` ` `` or `${`. Anything needing escapes traps.
fn quote(cx: &mut dyn Cx) -> Function {
    let concat = cx.helper(runtime::Helper::Concat);
    let (sq, dq, bq) = (cx.str("'"), cx.str("\""), cx.str("`"));
    const S: u32 = 0;
    const I: u32 = 1;
    const N: u32 = 2;
    const C: u32 = 3;
    const HAS_SQ: u32 = 4;
    const HAS_DQ: u32 = 5;
    const HAS_BQ: u32 = 6;
    const Q: u32 = 7;
    let mut f = Function::new([(6, ValType::I32), (1, rt(true, T_STRING))]);
    let mut s = f.instructions();
    s.local_get(S).array_len().local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(S)
        .local_get(I)
        .array_get_u(T_STRING)
        .local_set(C);
    // Needs an escape: control characters, `\`, U+007F–U+009F, a lone surrogate.
    s.local_get(C).i32_const(0x20).i32_lt_u();
    s.local_get(C).i32_const('\\' as i32).i32_eq().i32_or();
    s.local_get(C)
        .i32_const(0x7f)
        .i32_sub()
        .i32_const(0x21)
        .i32_lt_u()
        .i32_or();
    s.if_(BlockType::Empty);
    trap(cx, &mut s);
    s.end();
    s.local_get(C)
        .i32_const(0xF800)
        .i32_and()
        .i32_const(0xD800)
        .i32_eq();
    s.if_(BlockType::Empty);
    {
        // A high surrogate followed by a low one is fine (skip both).
        s.local_get(C)
            .i32_const(0xFC00)
            .i32_and()
            .i32_const(0xD800)
            .i32_eq();
        s.local_get(I)
            .i32_const(1)
            .i32_add()
            .local_get(N)
            .i32_lt_u()
            .i32_and();
        s.if_(BlockType::Result(ValType::I32));
        s.local_get(S)
            .local_get(I)
            .i32_const(1)
            .i32_add()
            .array_get_u(T_STRING)
            .i32_const(0xFC00)
            .i32_and()
            .i32_const(0xDC00)
            .i32_eq();
        s.else_();
        s.i32_const(0);
        s.end();
        s.i32_eqz();
        s.if_(BlockType::Empty);
        trap(cx, &mut s);
        s.end();
        s.local_get(I).i32_const(1).i32_add().local_set(I);
    }
    s.end();
    for (ch, flag) in [('\'', HAS_SQ), ('"', HAS_DQ), ('`', HAS_BQ)] {
        s.local_get(C).i32_const(ch as i32).i32_eq();
        s.if_(BlockType::Empty);
        s.i32_const(1).local_set(flag);
        s.end();
    }
    // `${` rules out backticks too.
    s.local_get(C).i32_const('$' as i32).i32_eq();
    s.local_get(I)
        .i32_const(1)
        .i32_add()
        .local_get(N)
        .i32_lt_u()
        .i32_and();
    s.if_(BlockType::Empty);
    s.local_get(S)
        .local_get(I)
        .i32_const(1)
        .i32_add()
        .array_get_u(T_STRING)
        .i32_const('{' as i32)
        .i32_eq();
    s.if_(BlockType::Empty);
    s.i32_const(1).local_set(HAS_BQ);
    s.end();
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.global_get(sq).local_set(Q);
    s.local_get(HAS_SQ);
    s.if_(BlockType::Empty);
    s.local_get(HAS_DQ).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(dq).local_set(Q);
    s.else_();
    s.local_get(HAS_BQ).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(bq).local_set(Q);
    s.else_();
    // `'` inside single quotes needs an escape.
    trap(cx, &mut s);
    s.end();
    s.end();
    s.end();
    s.local_get(Q).ref_as_non_null().local_get(S).call(concat);
    s.local_get(Q).ref_as_non_null().call(concat);
    s.end();
    f
}

/// An identifier (`[a-zA-Z_][a-zA-Z_0-9]*`) as is, anything else quoted.
fn key(cx: &mut dyn Cx) -> Function {
    let quote = ih(cx, Helper::Quote);
    const K: u32 = 0;
    const I: u32 = 1;
    const C: u32 = 2;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(K).array_len().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(K).call(quote).return_();
    s.end();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(K).array_len().i32_ge_u().br_if(1);
    s.local_get(K)
        .local_get(I)
        .array_get_u(T_STRING)
        .local_set(C);
    // letter or _ ; digits only after the first
    s.local_get(C)
        .i32_const(0x20)
        .i32_or()
        .i32_const('a' as i32)
        .i32_sub()
        .i32_const(26)
        .i32_lt_u();
    s.local_get(C).i32_const('_' as i32).i32_eq().i32_or();
    s.local_get(C)
        .i32_const('0' as i32)
        .i32_sub()
        .i32_const(10)
        .i32_lt_u();
    s.local_get(I).i32_const(0).i32_gt_u().i32_and();
    s.i32_or().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(K).call(quote).return_();
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(K);
    s.end();
    f
}

fn layout(cx: &mut dyn Cx) -> Function {
    let group = ih(cx, Helper::Group);
    let join = ih(cx, Helper::Join);
    let spaces = ih(cx, Helper::Spaces);
    let concat = cx.helper(runtime::Helper::Concat);
    let (space, comma_space, comma) = (cx.str(" "), cx.str(", "), cx.str(","));
    let newline = cx.str("\n");
    const E: u32 = 0;
    const N: u32 = 1;
    const INDENT: u32 = 2;
    const OPEN: u32 = 3;
    const CLOSE: u32 = 4;
    const ARRAY: u32 = 5;
    const NUMBERS: u32 = 6;
    const G: u32 = 7;
    const TOTAL: u32 = 8;
    const I: u32 = 9;
    const J: u32 = 10;
    const NL: u32 = 11;
    const IND: u32 = 12;
    let mut f = Function::new([
        (1, rt(true, T_KEYS)),
        (4, ValType::I32),
        (1, rt(true, T_STRING)),
    ]);
    let mut s = f.instructions();
    s.local_get(N).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(OPEN).local_get(CLOSE).call(concat).return_();
    s.end();
    s.local_get(ARRAY)
        .local_get(N)
        .i32_const(6)
        .i32_gt_u()
        .i32_and();
    s.if_(BlockType::Empty);
    s.local_get(E)
        .local_get(N)
        .local_get(INDENT)
        .local_get(NUMBERS)
        .call(group)
        .local_set(G);
    s.end();
    s.local_get(G).ref_is_null();
    s.if_(BlockType::Empty);
    {
        // Single line if it fits: output.length + start + Σ lengths ≤ 80, where
        // start = output.length + indentationLvl + braces[0].length + 10, and no entry has a newline.
        s.local_get(N)
            .i32_const(1)
            .i32_shl()
            .local_get(INDENT)
            .i32_add()
            .local_get(OPEN)
            .array_len()
            .i32_add()
            .i32_const(10)
            .i32_add()
            .local_set(TOTAL);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(I).local_get(N).i32_ge_u().br_if(1);
        s.local_get(TOTAL);
        s.local_get(E)
            .local_get(I)
            .array_get(T_KEYS)
            .ref_as_non_null()
            .array_len();
        s.i32_add().local_set(TOTAL);
        s.i32_const(0).local_set(J);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(J);
        s.local_get(E)
            .local_get(I)
            .array_get(T_KEYS)
            .ref_as_non_null()
            .array_len();
        s.i32_ge_u().br_if(1);
        s.local_get(E)
            .local_get(I)
            .array_get(T_KEYS)
            .ref_as_non_null()
            .local_get(J)
            .array_get_u(T_STRING)
            .i32_const('\n' as i32)
            .i32_eq();
        s.if_(BlockType::Empty);
        s.i32_const(1).local_set(NL);
        s.end();
        s.local_get(J).i32_const(1).i32_add().local_set(J);
        s.br(0).end().end();
        s.local_get(I).i32_const(1).i32_add().local_set(I);
        s.br(0).end().end();
        s.local_get(TOTAL).i32_const(BREAK_LENGTH).i32_le_s();
        s.local_get(NL).i32_eqz().i32_and();
        s.if_(BlockType::Empty);
        s.local_get(OPEN).global_get(space).call(concat);
        s.local_get(E)
            .local_get(N)
            .global_get(comma_space)
            .call(join);
        s.call(concat);
        s.global_get(space).call(concat);
        s.local_get(CLOSE).call(concat).return_();
        s.end();
        s.local_get(E).local_set(G);
    }
    s.else_();
    s.local_get(G).ref_as_non_null().array_len().local_set(N);
    s.end();
    // One entry (or group line) per line.
    s.global_get(newline);
    s.local_get(INDENT).call(spaces).call(concat).local_set(IND);
    s.local_get(OPEN);
    s.local_get(IND)
        .ref_as_non_null()
        .global_get(space)
        .call(concat)
        .global_get(space)
        .call(concat);
    s.call(concat);
    s.local_get(G).ref_as_non_null().local_get(N);
    s.global_get(comma);
    s.local_get(IND).ref_as_non_null().call(concat);
    s.global_get(space)
        .call(concat)
        .global_get(space)
        .call(concat);
    s.call(join);
    s.call(concat);
    s.local_get(IND).ref_as_non_null().call(concat);
    s.local_get(CLOSE).call(concat);
    s.end();
    f
}

fn group(cx: &mut dyn Cx) -> Function {
    let pad = ih(cx, Helper::Pad);
    let concat = cx.helper(runtime::Helper::Concat);
    let (comma_space, empty) = (cx.str(", "), cx.str(""));
    const E: u32 = 0;
    const N: u32 = 1;
    const INDENT: u32 = 2;
    const NUMBERS: u32 = 3;
    const I: u32 = 4;
    const J: u32 = 5;
    const MAXLEN: u32 = 6;
    const COLS: u32 = 7;
    const LINE: u32 = 8;
    const ROWS: u32 = 9;
    const TOTAL: u32 = 10; // f64
    const ACTUAL: u32 = 11; // f64
    const BIASED: u32 = 12; // f64
    const WIDTHS: u32 = 13; // (ref null $i32s)
    const OUT: u32 = 14;
    const STR: u32 = 15;
    let mut f = Function::new([
        (6, ValType::I32),
        (3, ValType::F64),
        (1, rt(true, runtime::T_I32S)),
        (1, rt(true, T_KEYS)),
        (1, rt(true, T_STRING)),
    ]);
    let mut s = f.instructions();
    let len = |s: &mut InstructionSink, idx: u32| {
        s.local_get(E)
            .local_get(idx)
            .array_get(T_KEYS)
            .ref_as_non_null()
            .array_len();
    };
    // totalLength = Σ (len + 2); maxLength.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(TOTAL);
    len(&mut s, I);
    s.i32_const(2)
        .i32_add()
        .f64_convert_i32_u()
        .f64_add()
        .local_set(TOTAL);
    len(&mut s, I);
    s.local_get(MAXLEN).i32_gt_u();
    s.if_(BlockType::Empty);
    len(&mut s, I);
    s.local_set(MAXLEN);
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(MAXLEN)
        .i32_const(2)
        .i32_add()
        .f64_convert_i32_u()
        .local_set(ACTUAL);
    // actualMax * 3 + indentationLvl < breakLength && (totalLength / actualMax > 5 || maxLength <= 6)
    s.local_get(ACTUAL).f64_const(3.0.into()).f64_mul();
    s.local_get(INDENT).f64_convert_i32_s().f64_add();
    s.f64_const((BREAK_LENGTH as f64).into()).f64_lt();
    s.local_get(TOTAL)
        .local_get(ACTUAL)
        .f64_div()
        .f64_const(5.0.into())
        .f64_gt();
    s.local_get(MAXLEN).i32_const(6).i32_le_u().i32_or();
    s.i32_and().i32_eqz();
    s.if_(BlockType::Empty);
    s.ref_null(wasm_encoder::HeapType::Concrete(T_KEYS))
        .return_();
    s.end();
    // biasedMax = max(actualMax - 3 - sqrt(actualMax - totalLength / entries), 1)
    s.local_get(ACTUAL).f64_const(3.0.into()).f64_sub();
    s.local_get(ACTUAL);
    s.local_get(TOTAL)
        .local_get(N)
        .f64_convert_i32_u()
        .f64_div();
    s.f64_sub().f64_sqrt();
    s.f64_sub();
    s.f64_const(1.0.into()).f64_max().local_set(BIASED);
    // columns = min(round(sqrt(2.5 * biasedMax * entries) / biasedMax),
    //               floor((80 - indentationLvl) / actualMax), compact * 4 (12), 15)
    s.f64_const(2.5.into())
        .local_get(BIASED)
        .f64_mul()
        .local_get(N)
        .f64_convert_i32_u()
        .f64_mul()
        .f64_sqrt()
        .local_get(BIASED)
        .f64_div();
    s.f64_const(0.5.into()).f64_add().f64_floor();
    s.f64_const((BREAK_LENGTH as f64).into());
    s.local_get(INDENT).f64_convert_i32_s().f64_sub();
    s.local_get(ACTUAL).f64_div().f64_floor();
    s.f64_min();
    s.f64_const(12.0.into()).f64_min();
    s.i32_trunc_sat_f64_s().local_set(COLS);
    s.local_get(COLS).i32_const(1).i32_le_s();
    s.if_(BlockType::Empty);
    s.ref_null(wasm_encoder::HeapType::Concrete(T_KEYS))
        .return_();
    s.end();
    // maxLineLength per column.
    s.local_get(COLS)
        .array_new_default(runtime::T_I32S)
        .local_set(WIDTHS);
    s.i32_const(0).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(COLS).i32_ge_u().br_if(1);
    s.i32_const(0).local_set(LINE);
    s.local_get(I).local_set(J);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(J).local_get(N).i32_ge_u().br_if(1);
    len(&mut s, J);
    s.local_get(LINE).i32_gt_u();
    s.if_(BlockType::Empty);
    len(&mut s, J);
    s.local_set(LINE);
    s.end();
    s.local_get(J).local_get(COLS).i32_add().local_set(J);
    s.br(0).end().end();
    s.local_get(WIDTHS)
        .local_get(I)
        .local_get(LINE)
        .i32_const(2)
        .i32_add()
        .array_set(runtime::T_I32S);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    // The lines.
    s.local_get(N)
        .local_get(COLS)
        .i32_add()
        .i32_const(1)
        .i32_sub()
        .local_get(COLS)
        .i32_div_u()
        .local_tee(ROWS)
        .array_new_default(T_KEYS)
        .local_set(OUT);
    s.i32_const(0).local_set(I);
    s.i32_const(0).local_set(ROWS);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    // max = min(i + columns, n)
    s.local_get(I).local_get(COLS).i32_add().local_tee(LINE);
    s.local_get(N);
    s.local_get(LINE).local_get(N).i32_lt_u();
    s.select().local_set(LINE);
    s.global_get(empty).local_set(STR);
    s.local_get(I).local_set(J);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(J)
        .local_get(LINE)
        .i32_const(1)
        .i32_sub()
        .i32_ge_u()
        .br_if(1);
    s.local_get(STR).ref_as_non_null();
    s.local_get(E)
        .local_get(J)
        .array_get(T_KEYS)
        .ref_as_non_null();
    s.global_get(comma_space).call(concat);
    s.local_get(WIDTHS)
        .local_get(J)
        .local_get(I)
        .i32_sub()
        .array_get(runtime::T_I32S);
    s.local_get(NUMBERS).call(pad);
    s.call(concat).local_set(STR);
    s.local_get(J).i32_const(1).i32_add().local_set(J);
    s.br(0).end().end();
    s.local_get(STR).ref_as_non_null();
    s.local_get(NUMBERS);
    s.if_(BlockType::Result(string_ref()));
    s.local_get(E)
        .local_get(J)
        .array_get(T_KEYS)
        .ref_as_non_null();
    s.local_get(WIDTHS)
        .local_get(J)
        .local_get(I)
        .i32_sub()
        .array_get(runtime::T_I32S)
        .i32_const(2)
        .i32_sub();
    s.i32_const(1).call(pad);
    s.else_();
    s.local_get(E)
        .local_get(J)
        .array_get(T_KEYS)
        .ref_as_non_null();
    s.end();
    s.call(concat).local_set(STR);
    s.local_get(OUT)
        .local_get(ROWS)
        .local_get(STR)
        .array_set(T_KEYS);
    s.local_get(ROWS).i32_const(1).i32_add().local_set(ROWS);
    s.local_get(I).local_get(COLS).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(OUT);
    s.end();
    f
}

fn spaces() -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.i32_const(' ' as i32).local_get(0).array_new(T_STRING);
    s.end();
    f
}

fn pad(cx: &mut dyn Cx) -> Function {
    let spaces = ih(cx, Helper::Spaces);
    let concat = cx.helper(runtime::Helper::Concat);
    const S: u32 = 0;
    const W: u32 = 1;
    const START: u32 = 2;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(S).array_len().local_get(W).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(S).return_();
    s.end();
    s.local_get(START);
    s.if_(BlockType::Result(string_ref()));
    s.local_get(W)
        .local_get(S)
        .array_len()
        .i32_sub()
        .call(spaces);
    s.local_get(S).call(concat);
    s.else_();
    s.local_get(S);
    s.local_get(W)
        .local_get(S)
        .array_len()
        .i32_sub()
        .call(spaces);
    s.call(concat);
    s.end();
    s.end();
    f
}

fn join(cx: &mut dyn Cx) -> Function {
    let concat = cx.helper(runtime::Helper::Concat);
    const E: u32 = 0;
    const N: u32 = 1;
    const SEP: u32 = 2;
    const I: u32 = 3;
    const OUT: u32 = 4;
    let mut f = Function::new([(1, ValType::I32), (1, rt(true, T_STRING))]);
    let mut s = f.instructions();
    s.local_get(E).i32_const(0).array_get(T_KEYS).local_set(OUT);
    s.i32_const(1).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(OUT)
        .ref_as_non_null()
        .local_get(SEP)
        .call(concat);
    s.local_get(E)
        .local_get(I)
        .array_get(T_KEYS)
        .ref_as_non_null()
        .call(concat);
    s.local_set(OUT);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(OUT).ref_as_non_null();
    s.end();
    f
}
