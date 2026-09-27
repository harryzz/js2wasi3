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
use crate::{func, mapset, object, promise, props};

const BREAK_LENGTH: i32 = 80;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(v, depth: i32, indent: i32, nested: i32) -> (ref $string)`: [`Helper::FormatValue`] with
    /// node's cycle detection (T25): an object already being formatted is `[Circular *n]`.
    Format,
    /// The same, without the cycle check.
    FormatValue,
    /// `(open: (ref $string), o: jsval) -> (ref $string)`: `<ref *n> ` + `open` if `o` was found
    /// in a cycle (T25).
    RefOpen,
    /// `(it: (ref $mapiter), depth, indent) -> (ref $string)`: `[Map Iterator] { 'a' }`,
    /// `[Map Entries] { [ 'a', 1 ] }`, `[Set Iterator] { 1 }`: the entries left, without
    /// consuming them (T25).
    MapIter,
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
    /// `(m: (ref $map), depth, indent) -> (ref $string)`: `Map(n) { k => v, … }` / `Set(n) { … }`
    /// (T25).
    MapSet,
    /// `(o: (ref $object), depth, indent, skip_indices: i32) -> (ref $keys)`: `key: value` for its
    /// own enumerable string keys (array indices skipped if `skip_indices`), then its enumerable
    /// symbol keys (`Symbol(x): value`), T25.
    Props,
    /// `(v) -> jsval`: `util.inspect(v)` from depth `d`, strings quoted, cycle tracking reset
    /// (T25: `console.log` format strings).
    At(u8),
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Format => "inspect_format",
            Helper::FormatValue => "inspect_format_value",
            Helper::RefOpen => "inspect_ref_open",
            Helper::MapIter => "inspect_map_iterator",
            Helper::At(_) => "inspect_at",
            Helper::Array => "inspect_array",
            Helper::Object => "inspect_object",
            Helper::Quote => "inspect_quote",
            Helper::Key => "inspect_key",
            Helper::Layout => "inspect_layout",
            Helper::Group => "inspect_group",
            Helper::Spaces => "spaces",
            Helper::Pad => "pad",
            Helper::Join => "join_strings",
            Helper::MapSet => "inspect_map",
            Helper::Props => "inspect_props",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::I32;
        let v = jsval();
        let s = string_ref();
        let k = rt(false, T_KEYS);
        match self {
            Helper::Format | Helper::FormatValue => (vec![v, I32, I32, I32], vec![s]),
            Helper::RefOpen => (vec![s, v], vec![s]),
            Helper::MapIter => (vec![rt(false, runtime::T_MAPITER), I32, I32], vec![s]),
            Helper::At(_) => (vec![v], vec![v]),
            Helper::Array => (vec![rt(false, T_ARRAY), I32, I32], vec![s]),
            Helper::MapSet => (vec![rt(false, runtime::T_MAP), I32, I32], vec![s]),
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
            Helper::Props => (vec![rt(false, runtime::T_OBJECT), I32, I32, I32], vec![k]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Format => format_checked(cx),
            Helper::FormatValue => format(cx),
            Helper::RefOpen => ref_open(cx),
            Helper::MapIter => format_map_iter(cx),
            Helper::At(d) => {
                let format = ih(cx, Helper::Format);
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.i32_const(0).global_set(runtime::G_INSPECT_SEEN_N);
                s.i32_const(0).global_set(runtime::G_INSPECT_CIRC_N);
                s.local_get(0).i32_const(d as i32).i32_const(0).i32_const(1);
                s.call(format);
                s.end();
                f
            }
            Helper::Array => format_array(cx),
            Helper::Object => format_object(cx),
            Helper::Quote => quote(cx),
            Helper::Key => key(cx),
            Helper::Layout => layout(cx),
            Helper::Group => group(cx),
            Helper::Spaces => spaces(),
            Helper::Pad => pad(cx),
            Helper::Join => join(cx),
            Helper::MapSet => format_mapset(cx),
            Helper::Props => format_props(cx),
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
    let f_mapset = cx.has_maps().then(|| ih(cx, Helper::MapSet));
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
    let (gen_named, gen_anonymous) = (
        cx.str("[GeneratorFunction: "),
        cx.str("[GeneratorFunction (anonymous)]"),
    );
    let (async_named, async_anonymous) = (
        cx.str("[AsyncFunction: "),
        cx.str("[AsyncFunction (anonymous)]"),
    );
    let empty = cx.str("");
    let object_suffix = cx.str(" Object");
    const V: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const NESTED: u32 = 3;
    const N: u32 = 4;
    const P: u32 = 5;
    const B: u32 = 6;
    // AggregateError (T25): its own `stack` (set when V8 makes it without frames) and `errors`.
    const STK: u32 = 7;
    const ERRS: u32 = 8;
    let aggregate = cx.names("any") || cx.names("AggregateError");
    let mut f = Function::new([
        (1, jsval()),
        (1, rt(true, runtime::T_OBJECT)),
        (1, rt(true, T_STRING)),
        (2, jsval()),
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
            // (`Function.prototype` is a function too, F47.)
            s.local_get(P)
                .global_get(runtime::G_PROTOS + js2wasi3_ir::Builtin::Function.index())
                .ref_eq()
                .i32_eqz()
                .i32_and();
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
            let is_kind = |s: &mut InstructionSink, kind: i32| {
                s.local_get(V)
                    .ref_cast_non_null(func::function())
                    .struct_get(runtime::T_FUNCTION, func::F_KIND)
                    .i32_const(kind)
                    .i32_eq();
            };
            // `[Function: `, `[GeneratorFunction: ` or `[AsyncFunction: `.
            // T20: `[AsyncGeneratorFunction: ` (only with async generators in the code).
            let async_gen = cx.names("$asyncgen").then(|| {
                (
                    cx.str("[AsyncGeneratorFunction: "),
                    cx.str("[AsyncGeneratorFunction (anonymous)]"),
                )
            });
            let pick = |s: &mut InstructionSink,
                        plain: u32,
                        generator: u32,
                        async_: u32,
                        async_gen: Option<u32>| {
                // (`select` keeps its first operand when the condition holds.)
                if let Some(g) = async_gen {
                    s.global_get(g);
                }
                s.global_get(async_);
                s.global_get(generator).global_get(plain);
                is_kind(s, func::KIND_GENERATOR);
                s.typed_select(string_ref());
                is_kind(s, func::KIND_ASYNC);
                s.typed_select(string_ref());
                if async_gen.is_some() {
                    is_kind(s, func::KIND_ASYNC_GENERATOR);
                    s.typed_select(string_ref());
                }
            };
            s.if_(BlockType::Result(string_ref()));
            pick(
                &mut s,
                named,
                gen_named,
                async_named,
                async_gen.map(|g| g.0),
            );
            s.local_get(N).ref_cast_non_null(string()).call(concat);
            s.global_get(close).call(concat);
            s.else_();
            pick(
                &mut s,
                anonymous,
                gen_anonymous,
                async_anonymous,
                async_gen.map(|g| g.1),
            );
            s.end();
        }
        s.end();
        s.local_set(B);
        // A function whose prototype is `Object.prototype` (`Function.prototype` itself, F47): node
        // names its constructor, `Object`.
        s.local_get(V)
            .ref_cast_non_null(runtime::object())
            .struct_get(runtime::T_OBJECT, object::F_PROTO)
            .global_get(runtime::G_PROTOS + js2wasi3_ir::Builtin::Object.index())
            .ref_eq();
        s.if_(BlockType::Empty);
        s.local_get(B).ref_as_non_null();
        s.global_get(object_suffix).call(concat).local_set(B);
        s.end();
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
    if cx.names("$typed") {
        // Typed arrays and `ArrayBuffer`s (T20).
        let f_typed = cx.helper(runtime::Helper::Typed(crate::typed::Helper::Format));
        let f_buffer = cx.helper(runtime::Helper::Typed(crate::typed::Helper::FormatBuffer));
        s.local_get(V).ref_test_non_null(crate::typed::typed());
        s.if_(BlockType::Empty);
        s.local_get(V).ref_cast_non_null(crate::typed::typed());
        s.local_get(DEPTH).local_get(INDENT).call(f_typed).return_();
        s.end();
        s.local_get(V).ref_test_non_null(crate::typed::buffer());
        s.if_(BlockType::Empty);
        s.local_get(V).ref_cast_non_null(crate::typed::buffer());
        s.local_get(DEPTH)
            .local_get(INDENT)
            .call(f_buffer)
            .return_();
        s.end();
    }
    if let Some(f_mapset) = f_mapset {
        s.local_get(V).ref_test_non_null(mapset::map());
        s.if_(BlockType::Empty);
        s.local_get(V).ref_cast_non_null(mapset::map());
        s.local_get(DEPTH)
            .local_get(INDENT)
            .call(f_mapset)
            .return_();
        s.end();
        // Map/Set iterators (T25).
        let f_iter = ih(cx, Helper::MapIter);
        s.local_get(V).ref_test_non_null(mapset::mapiter());
        s.if_(BlockType::Empty);
        s.local_get(V).ref_cast_non_null(mapset::mapiter());
        s.local_get(DEPTH).local_get(INDENT).call(f_iter).return_();
        s.end();
    }
    s.local_get(V).ref_test_non_null(runtime::object());
    s.if_(BlockType::Empty);
    object::is_error(&mut s, V);
    s.if_(BlockType::Empty);
    if aggregate {
        // A `stack` without frames (`Promise.any`'s error): node prints it in brackets, also
        // nested; `errors` as `{ [errors]: [ … ] }`.
        let own_get = oh(cx, object::Helper::OwnGet);
        let layout = ih(cx, Helper::Layout);
        let format = ih(cx, Helper::Format);
        let (stack, errors) = (cx.str("stack"), cx.str("errors"));
        let (errors_key, brace, rbrace) = (cx.str("[errors]: "), cx.str(" {"), cx.str("}"));
        s.local_get(V).ref_cast_non_null(runtime::object());
        s.global_get(stack).call(own_get).local_set(STK);
        runtime::is_string(&mut s, STK);
        s.i32_eqz().local_get(NESTED).i32_and();
        s.if_(BlockType::Empty);
        trap(cx, &mut s);
        s.end();
        s.global_get(open);
        runtime::is_string(&mut s, STK);
        s.if_(BlockType::Result(string_ref()));
        s.local_get(STK).ref_cast_non_null(string());
        s.else_();
        s.local_get(V).call(to_string);
        s.end();
        s.call(concat).global_get(close).call(concat).local_set(B);
        s.local_get(V).ref_cast_non_null(runtime::object());
        s.global_get(errors).call(own_get).local_tee(ERRS);
        s.ref_test_non_null(array::array());
        s.if_(BlockType::Empty);
        s.global_get(errors_key);
        s.local_get(ERRS);
        s.local_get(DEPTH).i32_const(1).i32_add();
        s.local_get(INDENT).i32_const(2).i32_add();
        s.i32_const(1).call(format).call(concat);
        s.array_new_fixed(T_KEYS, 1).i32_const(1).local_get(INDENT);
        s.local_get(B)
            .ref_as_non_null()
            .global_get(brace)
            .call(concat);
        s.global_get(rbrace);
        s.i32_const(0).i32_const(0).call(layout).return_();
        s.end();
        s.local_get(B).ref_as_non_null().return_();
    } else {
        // An error inside an array or object: node prints its stack (F31).
        s.local_get(NESTED);
        s.if_(BlockType::Empty);
        trap(cx, &mut s);
        s.end();
        s.global_get(open);
        s.local_get(V).call(to_string).call(concat);
        s.global_get(close).call(concat).return_();
    }
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
    let ref_open = ih(cx, Helper::RefOpen);
    let props = ih(cx, Helper::Props);
    let (more, more_item, more_items) =
        (cx.str("... "), cx.str(" more item"), cx.str(" more items"));
    let format = ih(cx, Helper::Format);
    let layout = ih(cx, Helper::Layout);
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let is_number = cx.helper(runtime::Helper::IsNumber);
    let labels = accessor_labels(cx);
    let elem_attrs = cx.helper(runtime::Helper::Arr(array::Helper::Attrs));
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
    const EXTRA: u32 = 10;
    const SPECIAL: u32 = 11;
    let mut f = Function::new([
        (5, ValType::I32),
        (1, rt(true, T_KEYS)),
        (1, jsval()),
        (1, rt(true, T_KEYS)),
        (1, ValType::I32),
    ]);
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
    s.local_get(A).struct_get(T_ARRAY, F_LENGTH).local_set(LEN);
    // Its other properties (T25), after the elements and `... n more items`.
    s.local_get(A);
    s.local_get(DEPTH).local_get(INDENT).i32_const(1);
    s.call(props).local_set(EXTRA);
    // At most 100 entries, one more for `... n more items`.
    s.local_get(LEN)
        .i32_const(100)
        .local_get(LEN)
        .i32_const(100)
        .i32_lt_u()
        .select();
    s.i32_const(1).i32_add();
    s.local_get(EXTRA).ref_as_non_null().array_len().i32_add();
    s.array_new_default(T_KEYS).local_set(OUT);
    s.i32_const(1).local_set(NUMBERS);
    let elem = |s: &mut InstructionSink, idx: u32| {
        s.local_get(A)
            .struct_get(T_ARRAY, F_ELEMS)
            .local_get(idx)
            .array_get(T_VALS);
    };
    // Is element `idx` shown as a hole? After the first hole node lists the elements by
    // `Object.keys` (`formatSpecialArray`), so from there a non-enumerable one is a hole too (T32).
    let hole = |s: &mut InstructionSink, idx: u32| {
        elem(s, idx);
        s.ref_is_null();
        s.local_get(SPECIAL);
        s.if_(BlockType::Result(ValType::I32));
        s.local_get(A)
            .local_get(idx)
            .call(elem_attrs)
            .i32_const(object::HIDDEN)
            .i32_and()
            .i32_const(0)
            .i32_ne();
        s.else_();
        s.i32_const(0);
        s.end();
        s.i32_or();
    };
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    s.local_get(N).i32_const(100).i32_ge_u().br_if(1);
    elem(&mut s, I);
    s.local_set(E);
    hole(&mut s, I);
    s.if_(BlockType::Empty);
    {
        // A run of holes: `<k empty item(s)>`.
        s.local_get(A)
            .struct_get(T_ARRAY, array::F_EATTRS)
            .ref_is_null()
            .i32_eqz()
            .local_set(SPECIAL);
        s.local_get(I).local_set(J);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(J).local_get(LEN).i32_ge_u().br_if(1);
        hole(&mut s, J);
        s.i32_eqz().br_if(1);
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
        // An accessor element (T32): `[Getter]` etc.
        s.local_get(E).ref_test_non_null(props::accessor());
        s.if_(BlockType::Result(string_ref()));
        accessor_label(&mut s, labels, E);
        s.else_();
        s.local_get(E);
        s.local_get(DEPTH).i32_const(1).i32_add();
        s.local_get(INDENT).i32_const(2).i32_add();
        s.i32_const(1).call(format);
        s.end();
        s.array_set(T_KEYS);
        s.local_get(I).i32_const(1).i32_add().local_set(I);
    }
    s.end();
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.br(0).end().end();
    // `... n more item(s)`.
    s.local_get(I).local_get(LEN).i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(OUT).ref_as_non_null().local_get(N);
    s.global_get(more);
    s.local_get(LEN)
        .local_get(I)
        .i32_sub()
        .f64_convert_i32_u()
        .call(number_to_string)
        .call(concat);
    s.global_get(more_item).global_get(more_items);
    s.local_get(LEN)
        .local_get(I)
        .i32_sub()
        .i32_const(1)
        .i32_eq();
    s.typed_select(string_ref());
    s.call(concat);
    s.array_set(T_KEYS);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.end();
    s.local_get(OUT).ref_as_non_null().local_get(N);
    s.local_get(EXTRA).ref_as_non_null().i32_const(0);
    s.local_get(EXTRA).ref_as_non_null().array_len();
    s.array_copy(T_KEYS, T_KEYS);
    // node's number alignment checks `value[i]` for every output position (T25): the positions
    // past the shown elements too (`... n more items`, the properties).
    s.local_get(N)
        .local_get(EXTRA)
        .ref_as_non_null()
        .array_len()
        .i32_add()
        .local_set(J);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(J).i32_ge_u().br_if(1);
    s.local_get(I).local_get(LEN).i32_ge_u();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(1);
    s.else_();
    elem(&mut s, I);
    s.call(is_number).i32_eqz();
    s.end();
    s.if_(BlockType::Empty);
    s.i32_const(0).local_set(NUMBERS);
    s.br(2);
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(OUT)
        .ref_as_non_null()
        .local_get(N)
        .local_get(EXTRA)
        .ref_as_non_null()
        .array_len()
        .i32_add()
        .local_get(INDENT);
    s.global_get(open);
    s.local_get(A).call(ref_open);
    s.global_get(close);
    s.i32_const(1).local_get(NUMBERS).call(layout);
    s.end();
    f
}

/// node's `formatMap` / `formatSet` in `formatRaw`: `Map(n) {`, the entries in order (keys and
/// values formatted one level deeper), laid out like an object's. Past depth 2, `[Map]` / `[Set]`
/// unless empty. At most 100 entries (`... n more items`), then its own properties (T25).
fn format_mapset(cx: &mut dyn Cx) -> Function {
    let ref_open = ih(cx, Helper::RefOpen);
    use crate::mapset::{T_KEYS_F, T_SIZE, T_USED, T_VALS_F};
    let format = ih(cx, Helper::Format);
    let layout = ih(cx, Helper::Layout);
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let props = ih(cx, Helper::Props);
    let (more, more_item, more_items) =
        (cx.str("... "), cx.str(" more item"), cx.str(" more items"));
    let (map_open, set_open) = (cx.str("Map("), cx.str("Set("));
    let (map_deep, set_deep) = (cx.str("[Map]"), cx.str("[Set]"));
    let (paren, close, arrow) = (cx.str(") {"), cx.str("}"), cx.str(" => "));
    let empty = cx.str("");
    const M: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const SET: u32 = 3;
    const E: u32 = 4;
    const N: u32 = 5;
    const T: u32 = 6;
    const OUT: u32 = 7;
    const K: u32 = 8;
    const EXTRA: u32 = 9;
    const SIZE: u32 = 10;
    let mut f = Function::new([
        (3, ValType::I32),
        (1, rt(true, runtime::T_MTABLE)),
        (1, rt(true, T_KEYS)),
        (1, jsval()),
        (1, rt(true, T_KEYS)),
        (1, ValType::I32),
    ]);
    let mut s = f.instructions();
    s.local_get(M)
        .struct_get(runtime::T_MAP, object::F_CLASS)
        .i32_const(object::CLASS_MASK)
        .i32_and()
        .i32_const(mapset::CLASS_SET)
        .i32_eq()
        .local_set(SET);
    s.local_get(M)
        .struct_get(runtime::T_MAP, mapset::M_TABLE)
        .local_tee(T);
    s.struct_get(runtime::T_MTABLE, T_SIZE).local_set(N);
    // Too deep (node: only when there is something to show).
    s.local_get(DEPTH).i32_const(2).i32_gt_s();
    s.local_get(N).i32_const(0).i32_ne();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.global_get(set_deep).global_get(map_deep);
    s.local_get(SET).typed_select(string_ref());
    s.return_();
    s.end();
    // Its own properties after the entries (T25).
    s.local_get(M);
    s.local_get(DEPTH).local_get(INDENT).i32_const(0);
    s.call(props).local_set(EXTRA);
    s.local_get(N).local_set(SIZE);
    // At most 100 entries, then `... n more items`.
    s.local_get(N)
        .i32_const(100)
        .local_get(N)
        .i32_const(100)
        .i32_lt_u()
        .select();
    s.i32_const(1).i32_add();
    s.local_get(EXTRA).ref_as_non_null().array_len().i32_add();
    s.array_new_default(T_KEYS).local_set(OUT);
    s.i32_const(0).local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(E)
        .local_get(T)
        .struct_get(runtime::T_MTABLE, T_USED)
        .i32_ge_u()
        .br_if(1);
    s.local_get(N).i32_const(100).i32_ge_u().br_if(1);
    s.local_get(T)
        .struct_get(runtime::T_MTABLE, T_KEYS_F)
        .local_get(E)
        .array_get(T_VALS)
        .local_tee(K);
    s.ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(OUT).local_get(N);
    s.local_get(K);
    s.local_get(DEPTH).i32_const(1).i32_add();
    s.local_get(INDENT).i32_const(2).i32_add();
    s.i32_const(1).call(format);
    // A map's ` => value`.
    s.local_get(SET);
    s.if_(BlockType::Result(string_ref()));
    s.global_get(empty);
    s.else_();
    s.global_get(arrow);
    s.local_get(T)
        .struct_get(runtime::T_MTABLE, T_VALS_F)
        .local_get(E)
        .array_get(T_VALS);
    s.local_get(DEPTH).i32_const(1).i32_add();
    s.local_get(INDENT).i32_const(2).i32_add();
    s.i32_const(1).call(format);
    s.call(concat);
    s.end();
    s.call(concat);
    s.array_set(T_KEYS);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.end();
    s.local_get(E).i32_const(1).i32_add().local_set(E);
    s.br(0).end().end();
    // `... n more item(s)`, then the properties.
    s.local_get(SIZE).local_get(N).i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(OUT).ref_as_non_null().local_get(N);
    s.global_get(more);
    s.local_get(SIZE)
        .local_get(N)
        .i32_sub()
        .f64_convert_i32_u()
        .call(number_to_string)
        .call(concat);
    s.global_get(more_item).global_get(more_items);
    s.local_get(SIZE)
        .local_get(N)
        .i32_sub()
        .i32_const(1)
        .i32_eq();
    s.typed_select(string_ref());
    s.call(concat);
    s.array_set(T_KEYS);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.end();
    s.local_get(OUT).ref_as_non_null().local_get(N);
    s.local_get(EXTRA).ref_as_non_null().i32_const(0);
    s.local_get(EXTRA).ref_as_non_null().array_len();
    s.array_copy(T_KEYS, T_KEYS);
    s.local_get(OUT)
        .ref_as_non_null()
        .local_get(N)
        .local_get(EXTRA)
        .ref_as_non_null()
        .array_len()
        .i32_add()
        .local_get(INDENT);
    s.global_get(set_open).global_get(map_open);
    s.local_get(SET).typed_select(string_ref());
    s.local_get(SIZE).f64_convert_i32_u().call(number_to_string);
    s.call(concat);
    s.global_get(paren).call(concat);
    s.local_get(M).call(ref_open);
    s.global_get(close);
    s.i32_const(0).i32_const(0).call(layout);
    s.end();
    f
}

fn format_object(cx: &mut dyn Cx) -> Function {
    let ref_open = ih(cx, Helper::RefOpen);
    let props = ih(cx, Helper::Props);
    let format = ih(cx, Helper::Format);
    let layout = ih(cx, Helper::Layout);
    let concat = cx.helper(runtime::Helper::Concat);
    let own_keys = oh(cx, object::Helper::OwnKeys);
    let obj_proto = oh(cx, object::Helper::Proto(js2wasi3_ir::Builtin::Object));
    let symbols = oh(cx, object::Helper::SymbolKeys);
    let ctor_name = cx.helper(runtime::Helper::Prop(props::Helper::CtorName));
    let (open, close, deep) = (cx.str("{"), cx.str("}"), cx.str("[Object]"));
    let (null_proto, deep_null) = (
        cx.str("[Object: null prototype] "),
        cx.str("[Object: null prototype]"),
    );
    let generator_prefix = cx.str("Object [Generator] ");
    let arguments_prefix = cx.str("[Arguments] ");
    let (lbracket, rbracket, space, object_) =
        (cx.str("["), cx.str("]"), cx.str(" "), cx.str("Object"));
    let str_eq = cx.helper(runtime::Helper::StrEq);
    let (pending, rejected) = (cx.str("<pending>"), cx.str("<rejected> "));
    const O: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const PREFIX: u32 = 3;
    const KS: u32 = 4;
    const N: u32 = 5;
    // (6: unused)
    const OUT: u32 = 7;
    const NAME: u32 = 8;
    const Q: u32 = 9;
    // (10: unused)
    const PLAIN: u32 = 11;
    // 1 for a promise: its state comes first (`Promise { <pending> }`, T17).
    const EXTRA: u32 = 12;
    const Q_STATE: u32 = 13;
    const Q_TAG: u32 = 14;
    const Q_SYMS: u32 = 15;
    const W_BASE: u32 = 16;
    const W_SKIP: u32 = 17;
    const W_PRIM: u32 = 18;
    let mut f = Function::new([
        (1, rt(true, T_KEYS)),
        (2, ValType::I32),
        (1, rt(true, T_KEYS)),
        (1, rt(true, T_STRING)),
        (1, rt(true, runtime::T_OBJECT)),
        (1, jsval()),
        (3, ValType::I32),
        (1, jsval()),
        (1, rt(true, T_VALS)),
        (1, rt(true, T_STRING)),
        (1, ValType::I32),
        (1, jsval()),
    ]);
    let mut s = f.instructions();
    // A wrapper object (T30): node's `getBoxedBase`, `[String: 'ab']`, `[Number (Foo): 1]`; alone
    // unless it has other keys than a string's indices (`[String: 'ab'] { x: 1 }`).
    {
        let tag = cx.helper(runtime::Helper::Wrap(crate::wrapper::Helper::Tag));
        let (null_p, open_p, colon) = (cx.str(" (null prototype)"), cx.str(" ("), cx.str(": "));
        let rparen = cx.str(")");
        s.local_get(O).ref_test_non_null(crate::wrapper::wrapper());
        s.if_(BlockType::Empty);
        s.local_get(O)
            .ref_cast_non_null(crate::wrapper::wrapper())
            .struct_get(runtime::T_WRAPPER, crate::wrapper::F_PRIM)
            .local_set(W_PRIM);
        s.global_get(lbracket);
        s.local_get(O).call(tag).ref_cast_non_null(string()).local_tee(Q_TAG);
        s.ref_cast_non_null(string()).call(concat).local_set(W_BASE);
        s.local_get(O).call(ctor_name).local_set(NAME);
        // On its own built-in prototype the name is the type's (`Symbol.prototype` may have no
        // `constructor` of its own here).
        for b in [
            js2wasi3_ir::Builtin::String,
            js2wasi3_ir::Builtin::Number,
            js2wasi3_ir::Builtin::Boolean,
            js2wasi3_ir::Builtin::Symbol,
        ] {
            s.local_get(O)
                .struct_get(runtime::T_OBJECT, object::F_PROTO)
                .global_get(runtime::G_PROTOS + b.index())
                .ref_eq();
            s.local_get(O)
                .struct_get(runtime::T_OBJECT, object::F_PROTO)
                .ref_is_null()
                .i32_eqz()
                .i32_and();
            s.if_(BlockType::Empty);
            s.local_get(Q_TAG).ref_cast_non_null(string()).local_set(NAME);
            s.end();
        }
        s.local_get(NAME).ref_is_null();
        s.if_(BlockType::Empty);
        s.local_get(W_BASE).ref_as_non_null();
        s.global_get(null_p).call(concat).local_set(W_BASE);
        s.else_();
        s.local_get(NAME)
            .ref_as_non_null()
            .local_get(Q_TAG)
            .ref_cast_non_null(string())
            .call(str_eq)
            .i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(W_BASE).ref_as_non_null();
        s.global_get(open_p).call(concat);
        s.local_get(NAME).ref_as_non_null().call(concat);
        s.global_get(rparen).call(concat).local_set(W_BASE);
        s.end();
        s.end();
        s.local_get(W_BASE).ref_as_non_null();
        s.global_get(colon).call(concat);
        s.local_get(W_PRIM)
            .local_get(DEPTH)
            .local_get(INDENT)
            .i32_const(1)
            .call(format)
            .call(concat);
        s.global_get(rbracket).call(concat).local_set(W_BASE);
        runtime::is_string(&mut s, W_PRIM);
        s.local_set(W_SKIP);
        // Other keys: the enumerable ones past a string's indices, and symbols.
        s.local_get(O).call(own_keys).array_len();
        s.local_get(W_SKIP);
        s.if_(BlockType::Result(ValType::I32));
        s.local_get(W_PRIM).ref_cast_non_null(string()).array_len();
        s.else_();
        s.i32_const(0);
        s.end();
        s.i32_sub();
        s.local_get(O).call(symbols).i32_or().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(W_BASE).ref_as_non_null().return_();
        s.end();
        s.local_get(W_BASE)
            .ref_as_non_null()
            .global_get(space)
            .call(concat)
            .local_set(PREFIX);
        s.end();
    }
    // A generator object (T18): `Object [Generator] ` (node's `Symbol.toStringTag` form).
    s.local_get(PREFIX).ref_is_null();
    s.local_get(O)
        .ref_test_non_null(crate::generator::generator());
    s.i32_and();
    s.if_(BlockType::Empty);
    s.global_get(generator_prefix).local_set(PREFIX);
    s.end();
    if cx.names("$asyncgen") {
        // An async generator object (T20): `Object [AsyncGenerator] `.
        let prefix = cx.str("Object [AsyncGenerator] ");
        s.local_get(O)
            .struct_get(runtime::T_OBJECT, object::F_CLASS)
            .i32_const(object::CLASS_MASK)
            .i32_and()
            .i32_const(object::CLASS_ASYNC_GEN)
            .i32_eq();
        s.if_(BlockType::Empty);
        s.global_get(prefix).local_set(PREFIX);
        s.end();
    }
    // An array iterator (T25): `Object [Array Iterator] ` (only if the code can make one).
    if ["values", "keys", "entries", "iterator"]
        .into_iter()
        .any(|k| cx.names(k))
    {
        let array_iterator_prefix = cx.str("Object [Array Iterator] ");
        s.local_get(PREFIX).ref_is_null();
        s.local_get(O)
            .struct_get(runtime::T_OBJECT, object::F_PROTO)
            .global_get(runtime::G_PROTOS + js2wasi3_ir::Builtin::ArrayIterator.index())
            .ref_eq();
        s.i32_and();
        s.local_get(O)
            .struct_get(runtime::T_OBJECT, object::F_PROTO)
            .ref_is_null()
            .i32_eqz()
            .i32_and();
        s.if_(BlockType::Empty);
        s.global_get(array_iterator_prefix).local_set(PREFIX);
        s.end();
    }
    // An `arguments` object (T25): `[Arguments] `.
    s.local_get(PREFIX).ref_is_null();
    s.local_get(O)
        .struct_get(runtime::T_OBJECT, object::F_CLASS)
        .i32_const(object::CLASS_MASK)
        .i32_and()
        .i32_const(object::CLASS_ARGUMENTS)
        .i32_eq();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.global_get(arguments_prefix).local_set(PREFIX);
    s.end();
    // `Math`/`JSON` (T25): node's `Symbol.toStringTag` form.
    let namespaces = if cx.has_namespaces() {
        vec![
            (object::CLASS_MATH, cx.str("Object [Math] ")),
            (object::CLASS_JSON, cx.str("Object [JSON] ")),
        ]
    } else {
        vec![]
    };
    for (class, prefix) in namespaces {
        s.local_get(PREFIX).ref_is_null();
        s.local_get(O)
            .struct_get(runtime::T_OBJECT, object::F_CLASS)
            .i32_const(object::CLASS_MASK)
            .i32_and()
            .i32_const(class)
            .i32_eq();
        s.i32_and();
        s.if_(BlockType::Empty);
        s.global_get(prefix).local_set(PREFIX);
        s.end();
    }
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
        if cx.names("toStringTag") {
            // A `Symbol.toStringTag` that isn't an own enumerable property and differs from the
            // constructor's name (T25): `Name [tag] ` / `Object [tag] `.
            let get = oh(cx, object::Helper::Get);
            let own_symbols = oh(cx, object::Helper::OwnSymbols);
            let sym = cx.helper(runtime::Helper::WellKnownSymbol(
                js2wasi3_ir::well_known_symbol("toStringTag").expect("well-known"),
            ));
            let lb = cx.str("[");
            let rb = cx.str("] ");
            s.local_get(O).call(sym).call(get).local_set(Q_TAG);
            runtime::is_string(&mut s, Q_TAG);
            s.if_(BlockType::Empty);
            {
                // Not empty, not own enumerable, not the constructor's name.
                s.local_get(Q_TAG)
                    .ref_cast_non_null(string())
                    .array_len()
                    .i32_eqz();
                s.local_get(O).i32_const(0).call(own_symbols);
                s.ref_cast_non_null(array::array())
                    .struct_get(T_ARRAY, F_ELEMS)
                    .local_set(Q_SYMS);
                s.i32_const(0).local_set(Q_STATE);
                s.block(BlockType::Empty).loop_(BlockType::Empty);
                s.local_get(Q_STATE)
                    .local_get(Q_SYMS)
                    .ref_as_non_null()
                    .array_len()
                    .i32_ge_u()
                    .br_if(1);
                s.local_get(Q_SYMS)
                    .ref_as_non_null()
                    .local_get(Q_STATE)
                    .array_get(T_VALS)
                    .call(sym)
                    .ref_eq();
                s.if_(BlockType::Empty);
                s.i32_const(-1).local_set(Q_STATE);
                s.br(2);
                s.end();
                s.local_get(Q_STATE)
                    .i32_const(1)
                    .i32_add()
                    .local_set(Q_STATE);
                s.br(0).end().end();
                s.local_get(Q_STATE).i32_const(-1).i32_eq();
                s.i32_or();
                s.local_get(NAME).ref_is_null();
                s.if_(BlockType::Result(ValType::I32));
                s.i32_const(0);
                s.else_();
                s.local_get(NAME)
                    .ref_as_non_null()
                    .local_get(Q_TAG)
                    .ref_cast_non_null(string())
                    .call(str_eq);
                s.end();
                s.i32_or();
                s.i32_eqz();
                s.if_(BlockType::Empty);
                s.local_get(NAME).ref_is_null();
                s.if_(BlockType::Result(string_ref()));
                // (No constructor: after `[Object: null prototype] `.)
                s.local_get(PREFIX).ref_is_null();
                s.if_(BlockType::Result(string_ref()));
                s.global_get(object_).global_get(space).call(concat);
                s.else_();
                s.local_get(PREFIX).ref_as_non_null();
                s.end();
                s.else_();
                s.local_get(NAME)
                    .ref_as_non_null()
                    .global_get(space)
                    .call(concat);
                s.end();
                s.global_get(lb).call(concat);
                s.local_get(Q_TAG).ref_cast_non_null(string()).call(concat);
                s.global_get(rb).call(concat);
                s.local_set(PREFIX);
                s.end();
            }
            s.end();
        }
    }
    s.end();
    s.local_get(O)
        .ref_test_non_null(promise::promise())
        .local_set(EXTRA);
    // Too deep (node: only when there is something to show).
    s.local_get(DEPTH).i32_const(2).i32_gt_s();
    s.local_get(O)
        .call(own_keys)
        .array_len()
        .local_get(O)
        .call(symbols)
        .i32_or()
        .local_get(EXTRA)
        .i32_or()
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
    s.local_get(O);
    s.local_get(DEPTH).local_get(INDENT).local_get(W_SKIP);
    s.call(props).local_tee(KS).array_len().local_tee(N);
    s.local_get(EXTRA).i32_add();
    s.array_new_default(T_KEYS).local_set(OUT);
    s.local_get(EXTRA);
    s.if_(BlockType::Empty);
    {
        // `<pending>`, the value, or `<rejected> ` and the reason.
        s.local_get(OUT).i32_const(0);
        s.local_get(O)
            .ref_cast_non_null(promise::promise())
            .struct_get(runtime::T_PROMISE, promise::P_STATE)
            .local_tee(Q_STATE);
        s.i32_const(promise::PENDING).i32_eq();
        s.if_(BlockType::Result(string_ref()));
        s.global_get(pending);
        s.else_();
        s.local_get(O)
            .ref_cast_non_null(promise::promise())
            .struct_get(runtime::T_PROMISE, promise::P_RESULT);
        s.local_get(DEPTH).i32_const(1).i32_add();
        s.local_get(INDENT).i32_const(2).i32_add();
        s.i32_const(1).call(format).local_set(NAME);
        s.local_get(Q_STATE).i32_const(promise::REJECTED).i32_eq();
        s.if_(BlockType::Result(string_ref()));
        s.global_get(rejected)
            .local_get(NAME)
            .ref_as_non_null()
            .call(concat);
        s.else_();
        s.local_get(NAME).ref_as_non_null();
        s.end();
        s.end();
        s.array_set(T_KEYS);
    }
    s.end();
    // The properties after the promise state (T25: `inspect_props`).
    s.local_get(OUT)
        .ref_as_non_null()
        .local_get(EXTRA)
        .local_get(KS)
        .ref_as_non_null()
        .i32_const(0)
        .local_get(N);
    s.array_copy(T_KEYS, T_KEYS);
    s.local_get(OUT)
        .ref_as_non_null()
        .local_get(N)
        .local_get(EXTRA)
        .i32_add()
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
    s.local_get(O).call(ref_open);
    s.global_get(close);
    s.i32_const(0).i32_const(0).call(layout);
    s.end();
    let _ = lbracket;
    f
}

/// node's `strEscape`: `'s'`, else `"s"` if `s` has a `'` but no `"`, else `` `s` `` if it has no
/// `` ` `` or `${`, else `'s'` with `\'`. Escapes: `\b \t \n \f \r`, `\xHH` for the other
/// control characters and U+007F–U+009F, `\\`, `\uxxxx` for a lone surrogate (T25).
fn quote(cx: &mut dyn Cx) -> Function {
    let _ = cx;
    const S: u32 = 0;
    const I: u32 = 1;
    const N: u32 = 2;
    const C: u32 = 3;
    const HAS_SQ: u32 = 4;
    const HAS_DQ: u32 = 5;
    const HAS_BQ: u32 = 6;
    const QC: u32 = 7;
    const ESC_Q: u32 = 8;
    const J: u32 = 9;
    const BUF: u32 = 10;
    let mut f = Function::new([(9, ValType::I32), (1, rt(true, T_STRING))]);
    let mut s = f.instructions();
    let unit = |s: &mut InstructionSink, at: &dyn Fn(&mut InstructionSink)| {
        s.local_get(S);
        at(s);
        s.array_get_u(T_STRING);
    };
    s.local_get(S).array_len().local_set(N);
    // Pass 1: which quotes occur (and `${`).
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    unit(&mut s, &|s| {
        s.local_get(I);
    });
    s.local_set(C);
    for (ch, flag) in [('\'', HAS_SQ), ('"', HAS_DQ), ('`', HAS_BQ)] {
        s.local_get(C).i32_const(ch as i32).i32_eq();
        s.if_(BlockType::Empty);
        s.i32_const(1).local_set(flag);
        s.end();
    }
    s.local_get(C).i32_const('$' as i32).i32_eq();
    s.local_get(I)
        .i32_const(1)
        .i32_add()
        .local_get(N)
        .i32_lt_u();
    s.i32_and();
    s.if_(BlockType::Empty);
    unit(&mut s, &|s| {
        s.local_get(I).i32_const(1).i32_add();
    });
    s.i32_const('{' as i32).i32_eq();
    s.if_(BlockType::Empty);
    s.i32_const(1).local_set(HAS_BQ);
    s.end();
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    // The quote.
    s.i32_const('\'' as i32).local_set(QC);
    s.local_get(HAS_SQ);
    s.if_(BlockType::Empty);
    s.local_get(HAS_DQ).i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const('"' as i32).local_set(QC);
    s.else_();
    s.local_get(HAS_BQ).i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const('`' as i32).local_set(QC);
    s.else_();
    s.i32_const(1).local_set(ESC_Q);
    s.end();
    s.end();
    s.end();
    // Pass 2: at most 6 units per unit (`\uxxxx`), plus the quotes.
    s.local_get(N)
        .i32_const(6)
        .i32_mul()
        .i32_const(2)
        .i32_add()
        .array_new_default(T_STRING)
        .local_set(BUF);
    let put = |s: &mut InstructionSink, v: &dyn Fn(&mut InstructionSink)| {
        s.local_get(BUF).ref_as_non_null().local_get(J);
        v(s);
        s.array_set(T_STRING);
        s.local_get(J).i32_const(1).i32_add().local_set(J);
    };
    let put_c = |s: &mut InstructionSink, c: char| {
        put(s, &|s| {
            s.i32_const(c as i32);
        });
    };
    // A hex digit of C: (C >> shift) & 15, in upper or lower case.
    let hex = |s: &mut InstructionSink, shift: i32, upper: bool| {
        put(s, &|s| {
            s.local_get(C)
                .i32_const(shift)
                .i32_shr_u()
                .i32_const(15)
                .i32_and();
            s.local_tee(HAS_SQ);
            s.i32_const(48).i32_add();
            s.local_get(HAS_SQ)
                .i32_const(if upper { 55 } else { 87 })
                .i32_add();
            s.local_get(HAS_SQ).i32_const(10).i32_lt_u();
            s.select();
        });
    };
    put(&mut s, &|s| {
        s.local_get(QC);
    });
    s.i32_const(0).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    unit(&mut s, &|s| {
        s.local_get(I);
    });
    s.local_set(C);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.block(BlockType::Empty);
    {
        // A surrogate pair: as is.
        s.local_get(C)
            .i32_const(0xFC00)
            .i32_and()
            .i32_const(0xD800)
            .i32_eq();
        s.local_get(I).local_get(N).i32_lt_u().i32_and();
        s.if_(BlockType::Empty);
        unit(&mut s, &|s| {
            s.local_get(I);
        });
        s.i32_const(0xFC00).i32_and().i32_const(0xDC00).i32_eq();
        s.if_(BlockType::Empty);
        put(&mut s, &|s| {
            s.local_get(C);
        });
        put(&mut s, &|s| {
            s.local_get(S).local_get(I).array_get_u(T_STRING);
        });
        s.local_get(I).i32_const(1).i32_add().local_set(I);
        s.br(2);
        s.end();
        s.end();
        // A lone surrogate: `\uxxxx`.
        s.local_get(C)
            .i32_const(0xF800)
            .i32_and()
            .i32_const(0xD800)
            .i32_eq();
        s.if_(BlockType::Empty);
        put_c(&mut s, '\\');
        put_c(&mut s, 'u');
        for shift in [12, 8, 4, 0] {
            hex(&mut s, shift, false);
        }
        s.br(1);
        s.end();
        // `\` and the quote (only `'` needs it).
        s.local_get(C).i32_const('\\' as i32).i32_eq();
        s.local_get(C)
            .i32_const('\'' as i32)
            .i32_eq()
            .local_get(ESC_Q)
            .i32_and();
        s.i32_or();
        s.if_(BlockType::Empty);
        put_c(&mut s, '\\');
        put(&mut s, &|s| {
            s.local_get(C);
        });
        s.br(1);
        s.end();
        // Control characters.
        s.local_get(C).i32_const(0x20).i32_lt_u();
        s.local_get(C)
            .i32_const(0x7f)
            .i32_sub()
            .i32_const(0x21)
            .i32_lt_u();
        s.i32_or();
        s.if_(BlockType::Empty);
        {
            for (code, letter) in [(8, 'b'), (9, 't'), (10, 'n'), (12, 'f'), (13, 'r')] {
                s.local_get(C).i32_const(code).i32_eq();
                s.if_(BlockType::Empty);
                put_c(&mut s, '\\');
                put_c(&mut s, letter);
                s.br(3);
                s.end();
            }
            put_c(&mut s, '\\');
            put_c(&mut s, 'x');
            hex(&mut s, 4, true);
            hex(&mut s, 0, true);
            s.br(1);
        }
        s.end();
        put(&mut s, &|s| {
            s.local_get(C);
        });
    }
    s.end();
    s.br(0).end().end();
    put(&mut s, &|s| {
        s.local_get(QC);
    });
    // Exactly J units.
    s.local_get(J).array_new_default(T_STRING).local_tee(S);
    s.i32_const(0);
    s.local_get(BUF).ref_as_non_null().i32_const(0).local_get(J);
    s.array_copy(T_STRING, T_STRING);
    s.local_get(S);
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
    const NL_KEYS: u32 = 13;
    let mut f = Function::new([
        (1, rt(true, T_KEYS)),
        (4, ValType::I32),
        (1, rt(true, T_STRING)),
        (1, rt(true, T_KEYS)),
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
    {
        // node's `groupArrayElements`: past 100 entries the last one (`... n more items`) isn't
        // grouped but appended.
        s.local_get(E);
        s.local_get(N)
            .local_get(N)
            .i32_const(100)
            .i32_gt_u()
            .i32_sub();
        s.local_get(INDENT)
            .local_get(NUMBERS)
            .call(group)
            .local_set(G);
        s.local_get(G).ref_is_null().i32_eqz();
        s.local_get(N).i32_const(100).i32_gt_u().i32_and();
        s.if_(BlockType::Empty);
        s.local_get(G).ref_as_non_null().array_len().local_tee(I);
        s.i32_const(1)
            .i32_add()
            .array_new_default(T_KEYS)
            .local_set(NL_KEYS);
        s.local_get(NL_KEYS).ref_as_non_null().i32_const(0);
        s.local_get(G).ref_as_non_null().i32_const(0).local_get(I);
        s.array_copy(T_KEYS, T_KEYS);
        s.local_get(NL_KEYS).ref_as_non_null().local_get(I);
        s.local_get(E)
            .local_get(N)
            .i32_const(1)
            .i32_sub()
            .array_get(T_KEYS);
        s.array_set(T_KEYS);
        s.local_get(NL_KEYS).local_set(G);
        s.i32_const(0).local_set(I);
        s.end();
    }
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

/// The globals of `[Getter]`, `[Setter]` and `[Getter/Setter]`.
fn accessor_labels(cx: &mut dyn Cx) -> [u32; 3] {
    [
        cx.str("[Getter]"),
        cx.str("[Setter]"),
        cx.str("[Getter/Setter]"),
    ]
}

/// Push how node shows the accessor in local `r` (a `$accessor`).
fn accessor_label(s: &mut InstructionSink, [getter, setter, both]: [u32; 3], r: u32) {
    let get = |s: &mut InstructionSink, field: u32| {
        s.local_get(r)
            .ref_cast_non_null(props::accessor())
            .struct_get(runtime::T_ACCESSOR, field)
            .global_get(runtime::G_UNDEFINED)
            .ref_eq();
    };
    s.global_get(getter).global_get(setter);
    get(s, 0);
    s.i32_eqz();
    s.typed_select(string_ref());
    s.global_get(both);
    get(s, 0);
    get(s, 1);
    s.i32_or();
    s.typed_select(string_ref());
}

/// `$inspect_props(o, depth, indent, skip_indices)` (T25), see [`Helper::Props`].
fn format_props(cx: &mut dyn Cx) -> Function {
    let format = ih(cx, Helper::Format);
    let key = ih(cx, Helper::Key);
    let concat = cx.helper(runtime::Helper::Concat);
    let own_keys = oh(cx, object::Helper::OwnKeys);
    let own_symbols = oh(cx, object::Helper::OwnSymbols);
    let own = oh(cx, object::Helper::OwnGet);
    let index = oh(cx, object::Helper::ArrayIndex);
    let colon = cx.str(": ");
    let labels = accessor_labels(cx);
    const O: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const SKIP: u32 = 3;
    const KS: u32 = 4;
    const SYMS: u32 = 5;
    const OUT: u32 = 6;
    const N: u32 = 7;
    const I: u32 = 8;
    const M: u32 = 9;
    const R: u32 = 10;
    const K: u32 = 11;
    let mut f = Function::new([
        (1, rt(true, T_KEYS)),
        (1, rt(true, T_VALS)),
        (1, rt(true, T_KEYS)),
        (3, ValType::I32),
        (1, jsval()),
        (1, rt(true, T_STRING)),
    ]);
    let mut s = f.instructions();
    // The value part: `[Getter]`, `[Setter]`, `[Getter/Setter]` or the value one level deeper.
    let value = |s: &mut InstructionSink| {
        s.local_get(O)
            .local_get(K)
            .ref_as_non_null()
            .call(own)
            .local_tee(R);
        s.ref_test_non_null(props::accessor());
        s.if_(BlockType::Result(string_ref()));
        accessor_label(s, labels, R);
        s.else_();
        s.local_get(R);
        s.local_get(DEPTH).i32_const(1).i32_add();
        s.local_get(INDENT).i32_const(2).i32_add();
        s.i32_const(1).call(format);
        s.end();
    };
    s.local_get(O).call(own_keys).local_set(KS);
    s.local_get(O).i32_const(0).call(own_symbols);
    s.ref_cast_non_null(array::array())
        .struct_get(T_ARRAY, F_ELEMS)
        .local_set(SYMS);
    s.local_get(KS).ref_as_non_null().array_len();
    s.local_get(SYMS).ref_as_non_null().array_len();
    s.i32_add().array_new_default(T_KEYS).local_set(OUT);
    s.local_get(KS).ref_as_non_null().array_len().local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(KS)
        .ref_as_non_null()
        .local_get(I)
        .array_get(T_KEYS)
        .local_set(K);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    // An array index (skipped for arrays).
    s.local_get(SKIP);
    s.local_get(K)
        .ref_as_non_null()
        .call(index)
        .i64_const(0)
        .i64_ge_s();
    s.i32_and().br_if(0);
    s.local_get(OUT).ref_as_non_null().local_get(M);
    s.local_get(K).ref_as_non_null().call(key);
    s.global_get(colon).call(concat);
    value(&mut s);
    s.call(concat);
    s.array_set(T_KEYS);
    s.local_get(M).i32_const(1).i32_add().local_set(M);
    s.br(0).end().end();
    // Symbol keys: `Symbol(x): value`.
    s.local_get(SYMS).ref_as_non_null().array_len().local_set(N);
    s.i32_const(0).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(SYMS)
        .ref_as_non_null()
        .local_get(I)
        .array_get(T_VALS)
        .ref_cast_non_null(string())
        .local_set(K);
    s.local_get(OUT).ref_as_non_null().local_get(M);
    s.local_get(K).ref_as_non_null();
    s.i32_const(0).i32_const(0).i32_const(0).call(format);
    s.global_get(colon).call(concat);
    value(&mut s);
    s.call(concat);
    s.array_set(T_KEYS);
    s.local_get(M).i32_const(1).i32_add().local_set(M);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    // Exactly M entries.
    s.local_get(M).array_new_default(T_KEYS).local_tee(KS);
    s.i32_const(0);
    s.local_get(OUT).ref_as_non_null().i32_const(0).local_get(M);
    s.array_copy(T_KEYS, T_KEYS);
    s.local_get(KS).ref_as_non_null();
    s.end();
    f
}

/// node's cycle detection in `formatValue` (T25): `[Circular *n]` for an object that is already
/// being formatted (`n` numbers the cycle targets in the order found); the target's `<ref *n>`
/// comes from [`Helper::RefOpen`]. The top-level call (depth 0, not nested) starts afresh.
fn format_checked(cx: &mut dyn Cx) -> Function {
    use runtime::{G_INSPECT_CIRC, G_INSPECT_CIRC_N, G_INSPECT_SEEN, G_INSPECT_SEEN_N};
    let value = ih(cx, Helper::FormatValue);
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let (circ_open, circ_close) = (cx.str("[Circular *"), cx.str("]"));
    const V: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const NESTED: u32 = 3;
    const K: u32 = 4;
    const R: u32 = 5;
    const TMP: u32 = 6;
    let mut f = Function::new([
        (1, ValType::I32),
        (1, rt(true, T_STRING)),
        (1, rt(true, T_VALS)),
    ]);
    let mut s = f.instructions();
    let call_value = |s: &mut InstructionSink| {
        s.local_get(V)
            .local_get(DEPTH)
            .local_get(INDENT)
            .local_get(NESTED)
            .call(value);
    };
    s.local_get(V)
        .ref_test_non_null(runtime::object())
        .i32_eqz();
    s.if_(BlockType::Empty);
    call_value(&mut s);
    s.return_();
    s.end();
    s.local_get(DEPTH)
        .i32_eqz()
        .local_get(NESTED)
        .i32_eqz()
        .i32_and();
    s.if_(BlockType::Empty);
    s.i32_const(0).global_set(G_INSPECT_SEEN_N);
    s.i32_const(0).global_set(G_INSPECT_CIRC_N);
    s.end();
    // Already being formatted: `[Circular *n]`.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(K)
        .global_get(G_INSPECT_SEEN_N)
        .i32_ge_u()
        .br_if(1);
    s.global_get(G_INSPECT_SEEN)
        .ref_as_non_null()
        .local_get(K)
        .array_get(T_VALS)
        .local_get(V)
        .ref_eq();
    s.if_(BlockType::Empty);
    {
        // Its number: its place among the targets found so far, or a new one.
        s.i32_const(0).local_set(K);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(K)
            .global_get(G_INSPECT_CIRC_N)
            .i32_ge_u()
            .br_if(1);
        s.global_get(G_INSPECT_CIRC)
            .ref_as_non_null()
            .local_get(K)
            .array_get(T_VALS)
            .local_get(V)
            .ref_eq()
            .br_if(1);
        s.local_get(K).i32_const(1).i32_add().local_set(K);
        s.br(0).end().end();
        s.local_get(K).global_get(G_INSPECT_CIRC_N).i32_eq();
        s.if_(BlockType::Empty);
        push(&mut s, G_INSPECT_CIRC, G_INSPECT_CIRC_N, V, TMP);
        s.end();
        s.global_get(circ_open);
        s.local_get(K)
            .i32_const(1)
            .i32_add()
            .f64_convert_i32_u()
            .call(number_to_string)
            .call(concat);
        s.global_get(circ_close).call(concat).return_();
    }
    s.end();
    s.local_get(K).i32_const(1).i32_add().local_set(K);
    s.br(0).end().end();
    push(&mut s, G_INSPECT_SEEN, G_INSPECT_SEEN_N, V, TMP);
    call_value(&mut s);
    s.local_set(R);
    s.global_get(G_INSPECT_SEEN_N)
        .i32_const(1)
        .i32_sub()
        .global_set(G_INSPECT_SEEN_N);
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

/// Append local `v` to the growable `$vals` in globals `arr`/`n` (doubling, from 8); `tmp` is a
/// `(ref null $vals)` local.
fn push(s: &mut InstructionSink, arr: u32, n: u32, v: u32, tmp: u32) {
    s.global_get(arr).ref_is_null();
    s.if_(BlockType::Empty);
    s.i32_const(8).array_new_default(T_VALS).global_set(arr);
    s.end();
    s.global_get(n)
        .global_get(arr)
        .ref_as_non_null()
        .array_len()
        .i32_eq();
    s.if_(BlockType::Empty);
    s.global_get(n)
        .i32_const(2)
        .i32_mul()
        .array_new_default(T_VALS)
        .local_tee(tmp);
    s.i32_const(0);
    s.global_get(arr)
        .ref_as_non_null()
        .i32_const(0)
        .global_get(n);
    s.array_copy(T_VALS, T_VALS);
    s.local_get(tmp).global_set(arr);
    s.end();
    s.global_get(arr)
        .ref_as_non_null()
        .global_get(n)
        .local_get(v);
    s.array_set(T_VALS);
    s.global_get(n).i32_const(1).i32_add().global_set(n);
}

/// `$inspect_ref_open(open, o)` (T25), see [`Helper::RefOpen`].
fn ref_open(cx: &mut dyn Cx) -> Function {
    use runtime::{G_INSPECT_CIRC, G_INSPECT_CIRC_N};
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let (ref_open, ref_close) = (cx.str("<ref *"), cx.str("> "));
    const OPEN: u32 = 0;
    const O: u32 = 1;
    const K: u32 = 2;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(K)
        .global_get(G_INSPECT_CIRC_N)
        .i32_ge_u()
        .br_if(1);
    s.global_get(G_INSPECT_CIRC)
        .ref_as_non_null()
        .local_get(K)
        .array_get(T_VALS)
        .local_get(O)
        .ref_eq();
    s.if_(BlockType::Empty);
    s.global_get(ref_open);
    s.local_get(K)
        .i32_const(1)
        .i32_add()
        .f64_convert_i32_u()
        .call(number_to_string)
        .call(concat);
    s.global_get(ref_close).call(concat);
    s.local_get(OPEN).call(concat).return_();
    s.end();
    s.local_get(K).i32_const(1).i32_add().local_set(K);
    s.br(0).end().end();
    s.local_get(OPEN);
    s.end();
    f
}

/// `$inspect_map_iterator(it, depth, indent)` (T25), see [`Helper::MapIter`]: node's
/// `formatIterator` / `formatMapIterInner`, at most 100 entries.
fn format_map_iter(cx: &mut dyn Cx) -> Function {
    use crate::mapset::{I_MODE, I_POS, I_TABLE};
    use js2wasi3_ir::MapOp;
    let format = ih(cx, Helper::Format);
    let layout = ih(cx, Helper::Layout);
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let op = |cx: &mut dyn Cx, o| cx.helper(runtime::Helper::Map(mapset::Helper::Op(o)));
    let (step, key, value) = (
        op(cx, MapOp::Step),
        op(cx, MapOp::Key),
        op(cx, MapOp::Value),
    );
    let heads = [
        cx.str("[Map Iterator] {"),
        cx.str("[Map Entries] {"),
        cx.str("[Set Iterator] {"),
        cx.str("[Set Entries] {"),
    ];
    let (open, close, brace) = (cx.str("["), cx.str("]"), cx.str("}"));
    let empty_close = cx.str("  }");
    let (more, more_item, more_items) =
        (cx.str("... "), cx.str(" more item"), cx.str(" more items"));
    const IT: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const C: u32 = 3;
    const MODE: u32 = 4;
    const I: u32 = 5;
    const N: u32 = 6;
    const TOTAL: u32 = 7;
    const OUT: u32 = 8;
    const PAIR: u32 = 9;
    let mut f = Function::new([
        (1, rt(true, runtime::T_MAPITER)),
        (4, ValType::I32),
        (2, rt(true, T_KEYS)),
    ]);
    let mut s = f.instructions();
    // A copy to step: the same table, position and mode.
    s.i32_const(object::CLASS_PLAIN);
    s.ref_null(wasm_encoder::HeapType::Concrete(runtime::T_OBJECT));
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(runtime::T_BYTES);
    s.i32_const(0);
    s.ref_null(wasm_encoder::HeapType::Concrete(runtime::T_I32S));
    s.local_get(IT).struct_get(runtime::T_MAPITER, I_TABLE);
    s.local_get(IT).struct_get(runtime::T_MAPITER, I_POS);
    s.local_get(IT)
        .struct_get(runtime::T_MAPITER, I_MODE)
        .local_tee(MODE);
    s.struct_new(runtime::T_MAPITER).local_set(C);
    s.i32_const(101).array_new_default(T_KEYS).local_set(OUT);
    let fmt = |s: &mut InstructionSink, get: u32| {
        s.local_get(C).ref_as_non_null().local_get(I).call(get);
        s.local_get(DEPTH).i32_const(1).i32_add();
        s.local_get(INDENT).i32_const(2).i32_add();
        s.i32_const(1).call(format);
    };
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(C).ref_as_non_null().call(step).local_tee(I);
    s.i32_const(0).i32_lt_s().br_if(1);
    s.local_get(TOTAL).i32_const(1).i32_add().local_set(TOTAL);
    s.local_get(N).i32_const(100).i32_ge_u().br_if(0);
    s.local_get(OUT).ref_as_non_null().local_get(N);
    // Mode % 3: 0 keys, 1 values (a set's are its keys), 2 entries `[ k, v ]`.
    s.local_get(MODE)
        .i32_const(3)
        .i32_rem_u()
        .i32_const(2)
        .i32_eq();
    s.if_(BlockType::Result(string_ref()));
    {
        s.i32_const(2).array_new_default(T_KEYS).local_set(PAIR);
        s.local_get(PAIR).ref_as_non_null().i32_const(0);
        fmt(&mut s, key);
        s.array_set(T_KEYS);
        s.local_get(PAIR).ref_as_non_null().i32_const(1);
        s.local_get(MODE).i32_const(3).i32_ge_u();
        s.if_(BlockType::Result(string_ref()));
        fmt(&mut s, key);
        s.else_();
        fmt(&mut s, value);
        s.end();
        s.array_set(T_KEYS);
        s.local_get(PAIR).ref_as_non_null().i32_const(2);
        s.local_get(INDENT).i32_const(2).i32_add();
        s.global_get(open).global_get(close);
        s.i32_const(1).i32_const(0).call(layout);
    }
    s.else_();
    s.local_get(MODE).i32_const(1).i32_eq();
    s.if_(BlockType::Result(string_ref()));
    fmt(&mut s, value);
    s.else_();
    fmt(&mut s, key);
    s.end();
    s.end();
    s.array_set(T_KEYS);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.br(0).end().end();
    s.local_get(TOTAL).local_get(N).i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(OUT).ref_as_non_null().local_get(N);
    s.global_get(more);
    s.local_get(TOTAL)
        .local_get(N)
        .i32_sub()
        .f64_convert_i32_u()
        .call(number_to_string)
        .call(concat);
    s.global_get(more_item).global_get(more_items);
    s.local_get(TOTAL)
        .local_get(N)
        .i32_sub()
        .i32_const(1)
        .i32_eq();
    s.typed_select(string_ref());
    s.call(concat);
    s.array_set(T_KEYS);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.end();
    // `[Map Iterator] {`, `[Map Entries] {`, `[Set Iterator] {`, `[Set Entries] {`.
    let head = |s: &mut InstructionSink| {
        // (`select` picks its first operand when the condition holds: the set's head first.)
        s.global_get(heads[3]).global_get(heads[2]);
        s.local_get(MODE)
            .i32_const(3)
            .i32_rem_u()
            .i32_const(2)
            .i32_eq();
        s.typed_select(string_ref());
        s.global_get(heads[1]).global_get(heads[0]);
        s.local_get(MODE)
            .i32_const(3)
            .i32_rem_u()
            .i32_const(2)
            .i32_eq();
        s.typed_select(string_ref());
        s.local_get(MODE).i32_const(3).i32_ge_u();
        s.typed_select(string_ref());
    };
    // No entries: `{  }` (node has no empty case for iterators).
    s.local_get(N).i32_eqz();
    s.if_(BlockType::Empty);
    head(&mut s);
    s.global_get(empty_close).call(concat).return_();
    s.end();
    s.local_get(OUT)
        .ref_as_non_null()
        .local_get(N)
        .local_get(INDENT);
    head(&mut s);
    s.global_get(brace);
    s.i32_const(0).i32_const(0).call(layout);
    s.end();
    f
}
