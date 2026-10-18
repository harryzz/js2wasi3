//! Function objects and calls through values (T14, design D1).
//!
//! # Layout
//! `$function` is a final subtype of `$object`: the seven object fields, then `code: i32` and
//! `env: (ref null $vals)`. Every object helper takes a function unchanged (`ref.test $object` matches
//! it); only `typeof`, `console.log` and calls look at the extra fields.
//! - `code` is a slot in the module's code table (T89: `Cx::code_slot`, called with `call_indirect`; a
//!   funcref in the GC struct cost a wasmtime libcall per call) of a function of the uniform type
//!   `$code = (self, this, args: eqref) -> eqref`: `self` is the function
//!   object, `args` a `$vals` array with exactly the arguments passed. The lowering generates the code
//!   of each function used as a value (an adapter calling the typed function, see `crates/lower`).
//! - `env` holds what the function captured: cells (`$cell_f64`, `$cell`) and plain values.
//! - Own properties, made with the object: `length` and `name` (non-enumerable), and for a
//!   `function` (not an arrow function or a method) `prototype`, a new object whose `constructor` is
//!   the function. (Read-only attributes: F36.)
//!
//! # Function.prototype
//! Built like the other prototypes ([`crate::object`]), but a `$function` itself (F47): callable
//! ([`Helper::Empty`]), not a constructor; `length: 0`, `name: ""`, real `call`, `apply`, `bind`.

use crate::runtime::JsTest;
use js2wasi3_ir::{Builtin, ErrorKind};
use wasm_encoder::{BlockType, Function, HeapType, ValType};

use crate::object;
use crate::runtime::{
    self, Cx, G_NULL, G_UNDEFINED, T_BYTES, T_CODE, T_FUNCTION, T_I32S, T_KEYS, T_OBJECT, T_VALS,
    jsval, rt, string_ref,
};

/// `$function` field indices after the object's.
pub const F_CODE: u32 = 7;
pub const F_ENV: u32 = 8;
/// `construct: i32` (T16; T89: a code table slot): what `new` runs, `(self, new_target, args)`; -1: not
/// a constructor. `code` is a slot too.
pub const F_CONSTRUCT: u32 = 9;
/// `kind: i32` (T16): [`KIND_FUNCTION`] or [`KIND_CLASS`].
pub const F_KIND: u32 = 10;
/// `source: (mut (ref null $string))` (T25): the source text for `Function.prototype.toString`;
/// null: a built-in (`function f() { [native code] }`).
pub const F_SOURCE: u32 = 11;
/// `learn: (mut eqref)` (T128): the layout `new` learns for an ordinary function without a build-time one
/// (V8's initial map with in-object slack tracking, per function object): null at first; while learning a
/// `$vals [first object, i31 count]`; then either a `$vals [index, keys, attrs]` (a layout as T109/T117's, made
/// at run time from the first object's keys after [`LEARN_AFTER`] constructions; `construct` is then
/// [`Helper::ConstructLearned`]), or null again with `construct` [`Helper::ConstructOrdinary`] (none).
pub const F_LEARN: u32 = 12;
/// T128: constructions before a function's layout is taken from its first object (V8: 7), so keys its objects
/// get after the constructor (Prototype.js `initialize`, IntersectionInfo-style later fields) are in it.
pub const LEARN_AFTER: i32 = 8;

pub const KIND_FUNCTION: i32 = 0;
pub const KIND_CLASS: i32 = 1;
/// A generator function (T18): `console.log` shows `[GeneratorFunction: f]`.
pub const KIND_GENERATOR: i32 = 2;
/// An async function (T19): `console.log` shows `[AsyncFunction: f]`.
pub const KIND_ASYNC: i32 = 3;
/// A bound function (T25): `env` is `[target, bound this, ...bound args]`.
pub const KIND_BOUND: i32 = 4;
/// `Array.prototype.values` / `[Symbol.iterator]`, `String.prototype[Symbol.iterator]` (T25): while
/// a value's `[Symbol.iterator]` is one, for-of takes the fast path.
pub const KIND_VALUES: i32 = 5;
/// An async generator function (T20): `console.log` shows `[AsyncGeneratorFunction: f]`.
pub const KIND_ASYNC_GENERATOR: i32 = 6;

/// [`Helper::New`] flags: an ordinary `function` (a `prototype` object, the ordinary construct).
pub const NEW_ORDINARY: i32 = 1;
/// [`Helper::New`] flags: a class (its `prototype` comes from [`Helper::ClassSetup`]).
pub const NEW_CLASS: i32 = 2;
/// [`Helper::New`] flags: a generator function (T18; not a constructor).
pub const NEW_GENERATOR: i32 = 4;
/// [`Helper::New`] flags: an async function (T19; not a constructor).
pub const NEW_ASYNC: i32 = 8;
/// [`Helper::New`] flags: a bound function (T25; no `prototype`).
pub const NEW_BOUND: i32 = 16;
/// [`Helper::New`] flags: a built-in `values` iterator method ([`KIND_VALUES`], T25).
pub const NEW_VALUES: i32 = 32;
/// [`Helper::New`] flags: an async generator function (T20; not a constructor).
pub const NEW_ASYNC_GENERATOR: i32 = 64;

pub fn function() -> HeapType {
    HeapType::Concrete(T_FUNCTION)
}

pub fn vals() -> HeapType {
    HeapType::Concrete(T_VALS)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(v: jsval) -> i32`: IsConstructor (T35).
    IsConstructor,
    /// `(code: i32, construct: i32 (code-table slots, T89; construct -1: none), env: (ref null $vals), name: (ref $string),
    /// length: i32, flags: i32) -> jsval`: a new function object (see the module docs and
    /// [`NEW_ORDINARY`], [`NEW_CLASS`]).
    New,
    /// `(f, args: (ref $vals), new_target: jsval, what: (ref $string)) -> jsval`: `new f(...args)`
    /// (T16), or the TypeError "`what` is not a constructor".
    Construct,
    /// `(f, args: (ref $vals), what: (ref $string)) -> jsval`: `new f(...args)` with `f` as the
    /// new target.
    ConstructSame,
    /// `(f, args: jsval (an array-like), new_target, what: (ref $string)) -> jsval`.
    ConstructList,
    /// The construct code (a `$code`, called `(self, new_target, args)`) of ordinary functions.
    ConstructOrdinary,
    /// `(new_target) -> jsval`: a new object whose prototype is `new_target.prototype` (or
    /// `Object.prototype` if that isn't an object).
    CreateThis,
    /// `(new_target, n: i32) -> jsval` (T109): [`Helper::CreateThis`] with room for `n` properties.
    CreateThisSized,
    /// `(new_target, n: i32, index: (ref $i32s)) -> jsval` (T109): [`Helper::CreateThisSized`] with a
    /// layout's shared index (`object::layout_index`).
    CreateThisLayout,
    /// T128: the construct codes of an ordinary function while it learns its layout, and once it has one.
    ConstructLearning,
    ConstructLearned,
    /// `(f: (ref $function), this: jsval)` (T128): one more construction of `f` while it learns its layout
    /// ([`F_LEARN`]).
    LearnLayout,
    /// `(f: jsval) -> jsval`: `f.prototype`, directly for an ordinary function's own (entry 2), else
    /// `object::Helper::Get` (T89).
    FnPrototype,
    /// `(parent) -> jsval`: check a class heritage (node's TypeErrors); the prototype's parent
    /// (`null` for `extends null`).
    ClassHeritage,
    /// `(class, parent: jsval (undefined: no heritage), proto_parent) -> jsval`: the class's
    /// `prototype` object; `class.__proto__ = parent` if it's a constructor.
    ClassSetup,
    /// `(v, f) -> i32`: `v instanceof f` for any value `f`.
    InstanceOfValue,
    /// `(value, this) -> jsval`: the result of a constructor returning `value` (derived: `undefined`
    /// gives `this`, which must be initialized; a non-object throws).
    CtorReturn { derived: bool },
    /// `() -> jsval`: the native error constructor as a function object (made on first use, with
    /// its prototype's `constructor`).
    ErrorCtor(ErrorKind),
    /// Its code (a `$code`): `E(message, options)`.
    ErrorCall(ErrorKind),
    /// Its construct code: a new error whose prototype comes from `new_target` (subclasses).
    ErrorConstruct(ErrorKind),
    /// `(f, this: jsval, args: (ref $vals), what: (ref $string)) -> jsval`: call `f`, or throw
    /// the TypeError "`what` is not a function".
    CallValue,
    /// `(args: jsval, i: i32) -> jsval`: argument `i`, `undefined` past the end.
    ArgGet,
    /// The code of `Function.prototype.call` (a `$code`): call `this` with `args[0]` as `this`
    /// and the other arguments.
    Call,
    /// `Function.prototype.apply` (T15): call `this` with `args[0]` as `this` and the elements of the
    /// array-like `args[1]` as the arguments.
    Apply,
    /// `(f, this, args: jsval (an array-like), what: (ref $string)) -> jsval`: `f(...args)`.
    CallList,
    /// `(v: jsval) -> (ref $vals)`: CreateListFromArrayLike (`undefined`/`null`: empty).
    ListFromArrayLike,
    /// The code of `Function.prototype.bind` (T25): a bound function ([`KIND_BOUND`]) named
    /// `bound <name>` with the remaining `length`, a constructor if the target is one.
    Bind,
    /// A bound function's code: call the target with the bound `this` and the bound arguments
    /// first.
    BoundCall,
    /// A bound function's construct code: `new` the target (`new_target` the target when it's the
    /// bound function itself).
    BoundConstruct,
    /// The code of `Function.prototype` itself (F47): returns `undefined`.
    Empty,
    /// `(f: jsval (a function), source: (ref $string)) -> jsval`: set `f`'s source text (T25).
    SetSource,
    /// `(f, key, prefix: jsval (a string)) -> jsval`: `f.name = prefix + key` (a symbol key:
    /// `[description]`, none for an empty one), F59.
    SetName,
    /// `() -> jsval`: a built-in constructor as a value (T25), made on first use: its prelude
    /// code, `prototype` (whose `constructor` it becomes) and prelude statics.
    BuiltinCtor(Builtin),
    /// `() -> jsval`: `Math`/`JSON` as a value (T25), made on first use: its constants and the
    /// prelude functions in use, all non-enumerable.
    Namespace(js2wasi3_ir::Namespace),
    /// The code of `Map`/`Set` called without `new`: node's TypeError.
    RequiresNew(Builtin),
    /// The construct code of `BigInt` (T40): node's TypeError, "X is not a constructor".
    NotConstructor(Builtin),
    /// The construct code of `Map`/`Set`/`ArrayBuffer`/a typed array (T47, F116; T126: `Array`/`Object`/`RegExp`): the prelude
    /// code, then OrdinaryCreateFromConstructor's prototype, `newTarget.prototype` if it's an
    /// object (subclasses, `Reflect.construct`). Their prelude functions don't read `this`.
    ConstructFrom(Builtin),
    /// The code of a constructor stub (see `ir::Module::ctor_stubs`): traps.
    StubTrap,
    /// The code of `Function.prototype.toString` (T25): the source text, or
    /// `function name() { [native code] }`.
    ToString,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::New => "new_function",
            Helper::Construct => "construct",
            Helper::ConstructSame => "construct_same",
            Helper::ConstructList => "construct_list",
            Helper::ConstructOrdinary => "construct_ordinary",
            Helper::ConstructLearning => "construct_learning",
            Helper::ConstructLearned => "construct_learned",
            Helper::CreateThis => "create_this",
            Helper::CreateThisSized => "create_this_sized",
            Helper::CreateThisLayout => "create_this_layout",
            Helper::LearnLayout => "learn_layout",
            Helper::FnPrototype => "fn_prototype",
            Helper::ClassHeritage => "class_heritage",
            Helper::ClassSetup => "class_setup",
            Helper::InstanceOfValue => "instanceof_value",
            Helper::CtorReturn { derived: false } => "base_ctor_return",
            Helper::CtorReturn { derived: true } => "derived_ctor_return",
            Helper::ErrorCtor(_) => "error_constructor",
            Helper::ErrorCall(_) => "error_call",
            Helper::ErrorConstruct(_) => "error_construct",
            Helper::CallValue => "call_value",
            Helper::ArgGet => "arg_get",
            Helper::Call => "Function.prototype.call",
            Helper::Apply => "Function.prototype.apply",
            Helper::CallList => "call_list",
            Helper::ListFromArrayLike => "list_from_array_like",
            Helper::Bind => "Function.prototype.bind",
            Helper::BoundCall => "bound_call",
            Helper::BoundConstruct => "bound_construct",
            Helper::Empty => "Function.prototype",
            Helper::SetSource => "set_function_source",
            Helper::SetName => "set_function_name",
            Helper::IsConstructor => "is_constructor",
            Helper::BuiltinCtor(_) => "builtin_constructor",
            Helper::Namespace(_) => "namespace",
            Helper::RequiresNew(_) => "requires_new",
            Helper::NotConstructor(_) => "not_a_constructor",
            Helper::ConstructFrom(_) => "construct_from_constructor",
            Helper::StubTrap => "constructor_stub_not_implemented",
            Helper::ToString => "Function.prototype.toString",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        let s = string_ref();
        match self {
            Helper::New => (
                vec![
                    ValType::I32,
                    ValType::I32,
                    rt(true, T_VALS),
                    s,
                    ValType::I32,
                    ValType::I32,
                ],
                vec![v],
            ),
            Helper::Construct => (vec![v, rt(false, T_VALS), v, s], vec![v]),
            Helper::ConstructSame => (vec![v, rt(false, T_VALS), s], vec![v]),
            Helper::ConstructList => (vec![v, v, v, s], vec![v]),
            Helper::ConstructOrdinary | Helper::ConstructLearning | Helper::ConstructLearned => {
                (vec![v, v, v], vec![v])
            }
            Helper::CreateThis | Helper::ClassHeritage | Helper::FnPrototype => (vec![v], vec![v]),
            Helper::CreateThisSized => (vec![v, ValType::I32], vec![v]),
            Helper::CreateThisLayout => (
                vec![v, ValType::I32, rt(false, runtime::T_I32S), rt(false, runtime::T_KEYS), rt(false, runtime::T_BYTES)],
                vec![v],
            ),
            Helper::LearnLayout => (vec![rt(false, T_FUNCTION), v], vec![]),
            Helper::ClassSetup => (vec![v, v, v], vec![v]),
            Helper::InstanceOfValue => (vec![v, v], vec![ValType::I32]),
            Helper::CtorReturn { .. } => (vec![v, v], vec![v]),
            Helper::ErrorCtor(_) => (vec![], vec![v]),
            Helper::ErrorCall(_) | Helper::ErrorConstruct(_) => (vec![v, v, v], vec![v]),
            Helper::CallValue => (vec![v, v, rt(false, T_VALS), s], vec![v]),
            Helper::ArgGet => (vec![v, ValType::I32], vec![v]),
            Helper::Call
            | Helper::Apply
            | Helper::Bind
            | Helper::BoundCall
            | Helper::BoundConstruct
            | Helper::Empty
            | Helper::ToString
            | Helper::RequiresNew(_)
            | Helper::NotConstructor(_)
            | Helper::ConstructFrom(_)
            | Helper::StubTrap => (vec![v, v, v], vec![v]),
            Helper::BuiltinCtor(_) | Helper::Namespace(_) => (vec![], vec![v]),
            Helper::SetSource => (vec![v, s], vec![v]),
            Helper::SetName => (vec![v, v, v], vec![v]),
            Helper::IsConstructor => (vec![v], vec![ValType::I32]),
            Helper::CallList => (vec![v, v, v, s], vec![v]),
            Helper::ListFromArrayLike => (vec![v], vec![rt(false, T_VALS)]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::New => new(cx),
            Helper::Construct => construct(cx),
            Helper::ConstructSame => {
                let construct = fh(cx, Helper::Construct);
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(0).local_get(1).local_get(0).local_get(2);
                s.call(construct);
                s.end();
                f
            }
            Helper::ConstructList => construct_list(cx),
            Helper::ConstructOrdinary => construct_ordinary(cx),
            Helper::ConstructLearning => construct_learning(cx),
            Helper::ConstructLearned => construct_learned(cx),
            Helper::CreateThis => create_this(cx, false),
            Helper::CreateThisSized => create_this(cx, true),
            Helper::CreateThisLayout => create_this_layout(cx),
            Helper::LearnLayout => learn_layout(cx),
            Helper::FnPrototype => fn_prototype(cx),
            Helper::ClassHeritage => class_heritage(cx),
            Helper::ClassSetup => class_setup(cx),
            Helper::InstanceOfValue => instance_of_value(cx),
            Helper::CtorReturn { derived } => ctor_return(cx, derived),
            Helper::ErrorCtor(k) => error_ctor(cx, k),
            Helper::ErrorCall(k) | Helper::ErrorConstruct(k) => {
                let new_error = oh(cx, object::Helper::NewError(k));
                let arg_get = fh(cx, Helper::ArgGet);
                let get = oh(cx, object::Helper::Get);
                let prototype = cx.str("prototype");
                const SELF: u32 = 0;
                const NT: u32 = 1;
                const ARGS: u32 = 2;
                const E: u32 = 3;
                const P: u32 = 4;
                let mut f = Function::new([(2, jsval())]);
                let mut s = f.instructions();
                let n = if matches!(k, ErrorKind::AggregateError | ErrorKind::SuppressedError) { 3 } else { 2 };
                for i in 0..n {
                    s.local_get(ARGS).i32_const(i).call(arg_get);
                }
                s.call(new_error).local_set(E);
                if matches!(self, Helper::ErrorConstruct(_)) {
                    s.local_get(NT).local_get(SELF).ref_eq().i32_eqz();
                    s.if_(BlockType::Empty);
                    s.local_get(NT).global_get(prototype).call(get).local_tee(P);
                    s.is_object();
                    s.if_(BlockType::Empty);
                    s.local_get(E).cast_object();
                    s.local_get(P).cast_object();
                    s.struct_set(T_OBJECT, object::F_PROTO);
                    s.end();
                    s.end();
                }
                s.local_get(E);
                s.end();
                f
            }
            Helper::CallValue => call_value(cx),
            Helper::ArgGet => arg_get(),
            Helper::Call => call(cx),
            Helper::Apply => apply(cx),
            Helper::CallList => call_list(cx),
            Helper::ListFromArrayLike => list_from_array_like(cx),
            Helper::Bind => bind(cx),
            Helper::BoundCall => bound_call(cx, false),
            Helper::BoundConstruct => bound_call(cx, true),
            Helper::Empty => {
                let mut f = Function::new([]);
                f.instructions().global_get(G_UNDEFINED).end();
                f
            }
            Helper::SetSource => {
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(0).ref_cast_non_null(function()).local_get(1);
                s.struct_set(T_FUNCTION, F_SOURCE);
                s.local_get(0).end();
                f
            }
            Helper::ToString => to_string(cx),
            Helper::SetName => set_name(cx),
            Helper::IsConstructor => {
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(0).ref_test_non_null(function());
                s.if_(BlockType::Result(ValType::I32));
                s.local_get(0)
                    .ref_cast_non_null(function())
                    .struct_get(T_FUNCTION, F_CONSTRUCT)
                    .i32_const(-1)
                    .i32_eq()
                    .i32_eqz();
                s.else_();
                s.i32_const(0);
                s.end();
                s.end();
                f
            }
            Helper::BuiltinCtor(b) => builtin_ctor(cx, b),
            Helper::ConstructFrom(b) => {
                let (code, _) = cx.builtin_ctor(b).expect("a linked constructor");
                let get = oh(cx, object::Helper::Get);
                let set_proto = oh(cx, object::Helper::SetProto);
                let prototype = cx.str("prototype");
                const NT: u32 = 1;
                const R: u32 = 3;
                const P: u32 = 4;
                let mut f = Function::new([(2, jsval())]);
                let mut s = f.instructions();
                s.local_get(0).local_get(NT).local_get(2).call(code).local_set(R);
                s.test_object(NT);
                s.if_(BlockType::Empty);
                s.local_get(NT).global_get(prototype).call(get).local_set(P);
                // Not an object (`null` too): the built-in's own prototype stays.
                s.test_object(P);
                s.if_(BlockType::Empty);
                s.local_get(R).local_get(P).call(set_proto);
                s.end();
                s.end();
                s.local_get(R);
                s.end();
                f
            }
            Helper::Namespace(ns) => namespace(cx, ns),
            Helper::RequiresNew(b) | Helper::NotConstructor(b) => {
                let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
                let msg = cx.str(&if matches!(self, Helper::RequiresNew(_)) {
                    format!("Constructor {} requires 'new'", b.name())
                } else {
                    format!("{} is not a constructor", b.name())
                });
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.global_get(msg).call(throw);
                s.unreachable().end();
                f
            }
            Helper::StubTrap => {
                let trap = oh(cx, object::Helper::BuiltinTrap);
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.call(trap).unreachable().end();
                f
            }
        }
    }
}

fn oh(cx: &mut dyn Cx, h: object::Helper) -> u32 {
    cx.helper(runtime::Helper::Obj(h))
}

fn fh(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Func(h))
}

fn new(cx: &mut dyn Cx) -> Function {
    let proto = oh(cx, object::Helper::Proto(Builtin::Function));
    let obj_proto = oh(cx, object::Helper::Proto(Builtin::Object));
    let alloc = oh(cx, object::Helper::Alloc);
    let define = oh(cx, object::Helper::Define);
    // T128: ordinary functions start learning their layout.
    let ordinary = fh(cx, Helper::ConstructLearning);
    let ordinary_slot = cx.code_slot(ordinary);
    let (length, name, prototype, constructor) = (
        cx.str("length"),
        cx.str("name"),
        cx.str("prototype"),
        cx.str("constructor"),
    );
    const CODE: u32 = 0;
    const CONSTRUCT: u32 = 1;
    const ENV: u32 = 2;
    const NAME: u32 = 3;
    const LEN: u32 = 4;
    const FLAGS: u32 = 5;
    const F: u32 = 6;
    const P: u32 = 7;
    let mut f = Function::new([(1, rt(true, T_FUNCTION)), (1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.i32_const(object::CLASS_PLAIN).call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    s.local_get(CODE).local_get(ENV);
    // An ordinary function's construct entry: the shared one unless it has its own (T38).
    s.local_get(FLAGS).i32_const(NEW_ORDINARY).i32_and();
    s.local_get(CONSTRUCT).i32_const(-1).i32_eq().i32_and();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(ordinary_slot);
    s.else_();
    s.local_get(CONSTRUCT);
    s.end();
    s.i32_const(KIND_CLASS);
    s.i32_const(KIND_ASYNC_GENERATOR);
    s.i32_const(KIND_GENERATOR);
    s.i32_const(KIND_ASYNC);
    s.i32_const(KIND_VALUES);
    s.i32_const(KIND_BOUND).i32_const(KIND_FUNCTION);
    s.local_get(FLAGS).i32_const(NEW_BOUND).i32_and();
    s.select();
    s.local_get(FLAGS).i32_const(NEW_VALUES).i32_and();
    s.select();
    s.local_get(FLAGS).i32_const(NEW_ASYNC).i32_and();
    s.select();
    s.local_get(FLAGS).i32_const(NEW_GENERATOR).i32_and();
    s.select();
    s.local_get(FLAGS).i32_const(NEW_ASYNC_GENERATOR).i32_and();
    s.select();
    s.local_get(FLAGS).i32_const(NEW_CLASS).i32_and();
    s.select();
    s.ref_null(HeapType::Concrete(runtime::T_STRING));
    s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
    s.struct_new(T_FUNCTION).local_set(F);
    // `length` and `name`: read-only, configurable.
    s.local_get(F).ref_as_non_null().global_get(length);
    s.local_get(LEN).ref_i31();
    s.i32_const(object::HIDDEN | object::READONLY).call(define);
    s.local_get(F)
        .ref_as_non_null()
        .global_get(name)
        .local_get(NAME);
    s.i32_const(object::HIDDEN | object::READONLY).call(define);
    s.local_get(FLAGS).i32_const(NEW_ORDINARY).i32_and();
    s.if_(BlockType::Empty);
    s.i32_const(object::CLASS_PLAIN)
        .call(obj_proto)
        .call(alloc)
        .local_tee(P)
        .ref_as_non_null();
    s.global_get(constructor).local_get(F);
    s.i32_const(object::HIDDEN).call(define);
    // `prototype`: writable, not configurable.
    s.local_get(F).ref_as_non_null().global_get(prototype);
    s.local_get(P);
    s.i32_const(object::HIDDEN | object::FIXED).call(define);
    s.end();
    s.local_get(F);
    s.end();
    f
}

/// Throw "`what` is not a constructor" unless local `f` is one.
fn check_constructor(cx: &mut dyn Cx, s: &mut wasm_encoder::InstructionSink, f: u32, what: u32) {
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let not_ctor = cx.str(" is not a constructor");
    s.local_get(f).ref_test_non_null(function());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(f)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_CONSTRUCT)
        .i32_const(-1)
        .i32_eq();
    s.else_();
    s.i32_const(1);
    s.end();
    s.if_(BlockType::Empty);
    s.local_get(what)
        .global_get(not_ctor)
        .call(concat)
        .call(throw);
    s.end();
}

fn construct(cx: &mut dyn Cx) -> Function {
    const F: u32 = 0;
    const ARGS: u32 = 1;
    const NT: u32 = 2;
    const WHAT: u32 = 3;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    check_constructor(cx, &mut s, F, WHAT);
    s.local_get(F).local_get(NT).local_get(ARGS);
    s.local_get(F)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_CONSTRUCT);
    s.call_indirect(0, T_CODE);
    s.end();
    f
}

fn construct_list(cx: &mut dyn Cx) -> Function {
    let list = fh(cx, Helper::ListFromArrayLike);
    let construct = fh(cx, Helper::Construct);
    let mut f = Function::new([]);
    let mut s = f.instructions();
    // The callee is checked before the arguments are spread.
    check_constructor(cx, &mut s, 0, 3);
    s.local_get(0);
    s.local_get(1).call(list);
    s.local_get(2).local_get(3).call(construct);
    s.end();
    f
}

fn create_this(cx: &mut dyn Cx, sized: bool) -> Function {
    let fn_prototype = fh(cx, Helper::FnPrototype);
    let alloc = oh(
        cx,
        if sized {
            object::Helper::AllocSized
        } else {
            object::Helper::Alloc
        },
    );
    let obj_proto = oh(cx, object::Helper::Proto(Builtin::Object));
    const NT: u32 = 0;
    let p: u32 = if sized { 2 } else { 1 };
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.i32_const(object::CLASS_PLAIN);
    s.local_get(NT).call(fn_prototype).local_set(p);
    // (`test_object`: no `is_subtype` libcall for the non-final `$object`, F350.)
    s.test_object(p);
    s.if_(BlockType::Result(rt(true, T_OBJECT)));
    s.local_get(p).cast_object();
    s.else_();
    s.call(obj_proto);
    s.end();
    if sized {
        s.local_get(1);
    }
    s.call(alloc);
    s.end();
    f
}

/// `create_this_layout(new_target, n, index, keys, attrs)`: `create_this_sized`'s object with its layout's shared
/// index, keys and attributes (T109, T117).
fn create_this_layout(cx: &mut dyn Cx) -> Function {
    let fn_prototype = fh(cx, Helper::FnPrototype);
    // T133: an object of the layout's shape type.
    let alloc = oh(cx, object::Helper::AllocShape);
    let obj_proto = oh(cx, object::Helper::Proto(Builtin::Object));
    const NT: u32 = 0;
    const P: u32 = 5;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.i32_const(object::CLASS_PLAIN);
    s.local_get(NT).call(fn_prototype).local_set(P);
    s.test_object(P);
    s.if_(BlockType::Result(rt(true, T_OBJECT)));
    s.local_get(P).cast_object();
    s.else_();
    s.call(obj_proto);
    s.end();
    s.local_get(1).local_get(2).local_get(3).local_get(4).call(alloc);
    s.end();
    f
}

/// `fn_prototype(f)`: `f.prototype` (see [`Helper::FnPrototype`]). An ordinary function's `prototype`
/// is its entry 2 (after `length`, `name`: `new`), non-configurable, so it stays a data property there.
fn fn_prototype(cx: &mut dyn Cx) -> Function {
    let get = oh(cx, object::Helper::Get);
    let value = cx.helper(runtime::Helper::Prop(crate::props::Helper::Value));
    let prototype = cx.str("prototype");
    const V: u32 = 0;
    const F: u32 = 1;
    const R: u32 = 2;
    let mut f = Function::new([(1, rt(true, T_FUNCTION)), (1, jsval())]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(function());
    s.if_(BlockType::Empty);
    s.local_get(V).ref_cast_non_null(function()).local_tee(F);
    s.struct_get(T_FUNCTION, object::F_COUNT).i32_const(2).i32_gt_s();
    s.if_(BlockType::Empty);
    s.local_get(F)
        .struct_get(T_FUNCTION, object::F_KEYS)
        .i32_const(2)
        .array_get(runtime::T_KEYS)
        .global_get(prototype)
        .ref_eq();
    s.if_(BlockType::Empty);
    s.local_get(F)
        .struct_get(T_FUNCTION, object::F_VALS)
        .i32_const(2)
        .array_get(runtime::T_VALS)
        .local_tee(R);
    // (An accessor or a built-in sentinel can't be there, but resolve them anyway.)
    s.ref_test_non_null(HeapType::Concrete(runtime::T_ACCESSOR));
    s.local_get(R).global_get(runtime::G_BUILTIN).ref_eq();
    s.i32_or();
    s.if_(BlockType::Empty);
    s.local_get(R).local_get(V).call(value).return_();
    s.end();
    s.local_get(R).return_();
    s.end();
    s.end();
    s.end();
    s.local_get(V).global_get(prototype).call(get);
    s.end();
    f
}

/// `(self, new_target, args)`: `this` from `new_target.prototype`, the code called with it; an
/// object result replaces it. T128: also what an ordinary function runs once it has learned it gets no layout.
fn construct_ordinary(cx: &mut dyn Cx) -> Function {
    let create_this = fh(cx, Helper::CreateThis);
    construct_with(cx, |s| {
        s.local_get(1).call(create_this);
    }, None)
}

/// T128: an ordinary function's first constructions ([`LEARN_AFTER`]), each given to [`Helper::LearnLayout`],
/// which then switches the function's `construct` to [`Helper::ConstructOrdinary`] or [`Helper::ConstructLearned`]
/// (V8 swaps a function's construct stub the same way when slack tracking ends).
fn construct_learning(cx: &mut dyn Cx) -> Function {
    let create_this = fh(cx, Helper::CreateThis);
    let learn = fh(cx, Helper::LearnLayout);
    construct_with(cx, |s| {
        s.local_get(1).call(create_this);
    }, Some(learn))
}

/// T128: `this` with the function object's learned layout ([`F_LEARN`]: `[index, keys, attrs]`).
fn construct_learned(cx: &mut dyn Cx) -> Function {
    let create_this_layout = fh(cx, Helper::CreateThisLayout);
    let on = cx.counting();
    construct_with(cx, |s| {
        runtime::count(s, on, "construct.learned");
        let field = |s: &mut wasm_encoder::InstructionSink, i: i32, t: u32| {
            s.local_get(5).i32_const(i).array_get(T_VALS).ref_cast_non_null(HeapType::Concrete(t));
        };
        s.local_get(0)
            .ref_cast_non_null(function())
            .struct_get(T_FUNCTION, F_LEARN)
            .ref_cast_non_null(HeapType::Concrete(T_VALS))
            .local_set(5);
        s.local_get(1);
        field(s, 1, T_KEYS);
        s.local_tee(6).array_len();
        field(s, 0, T_I32S);
        s.local_get(6).ref_as_non_null();
        field(s, 2, T_BYTES);
        s.call(create_this_layout);
    }, None)
}

/// The construct codes' shared body: `this` from `make_this` (params `self`, `new_target`, `args`; scratch
/// locals 5: `(ref null $vals)`, 6: `(ref null $keys)`), the code called with it, then `learn(f, this)` when
/// given; an object result replaces `this`.
fn construct_with(
    cx: &mut dyn Cx,
    make_this: impl FnOnce(&mut wasm_encoder::InstructionSink),
    learn: Option<u32>,
) -> Function {
    let _ = cx;
    const SELF: u32 = 0;
    const ARGS: u32 = 2;
    const THIS: u32 = 3;
    const R: u32 = 4;
    let mut f = Function::new([(2, jsval()), (1, rt(true, T_VALS)), (1, rt(true, T_KEYS))]);
    let mut s = f.instructions();
    make_this(&mut s);
    s.local_set(THIS);
    s.local_get(SELF).local_get(THIS).local_get(ARGS);
    s.local_get(SELF)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_CODE);
    s.call_indirect(0, T_CODE).local_set(R);
    if let Some(learn) = learn {
        s.local_get(SELF).ref_cast_non_null(function()).local_get(THIS).call(learn);
    }
    // T93: `test_object`, not `ref.test (ref $object)`: the usual result, `undefined`, isn't an exact
    // `$object`, and the plain test called wasmtime's `is_subtype` on every `new`.
    s.test_object(R);
    s.if_(BlockType::Result(jsval()));
    s.local_get(R);
    s.else_();
    s.local_get(THIS);
    s.end();
    s.end();
    f
}

/// `learn_layout(f, this)` (see [`Helper::LearnLayout`]): the first construction keeps its object; the
/// [`LEARN_AFTER`]th takes the layout from that object's keys now: a plain object with 1 to `SMALL` entries, no
/// own index, every entry a live key with the default attributes (a writable data property). Its index is made
/// as `object::layout_index` makes a build-time layout's; else the function gets none, for good.
fn learn_layout(cx: &mut dyn Cx) -> Function {
    let hash = oh(cx, object::Helper::Hash);
    let ordinary = fh(cx, Helper::ConstructOrdinary);
    let ordinary = cx.code_slot(ordinary);
    let learned = fh(cx, Helper::ConstructLearned);
    let learned = cx.code_slot(learned);
    let on = cx.counting();
    const F: u32 = 0;
    const THIS: u32 = 1;
    const L: u32 = 2;
    const C: u32 = 3;
    const N: u32 = 4;
    const I: u32 = 5;
    const CAP: u32 = 6;
    const J: u32 = 7;
    const O: u32 = 8;
    const KEYS: u32 = 9;
    const IDX: u32 = 10;
    let mut f = Function::new([
        (1, rt(true, T_VALS)),
        (5, ValType::I32),
        (1, rt(true, runtime::T_PLAIN)),
        (1, rt(true, T_KEYS)),
        (1, rt(true, T_I32S)),
    ]);
    let mut s = f.instructions();
    s.local_get(F).struct_get(T_FUNCTION, F_LEARN).ref_is_null();
    s.if_(BlockType::Empty);
    s.local_get(F);
    s.local_get(THIS).i32_const(1).ref_i31().array_new_fixed(T_VALS, 2);
    s.struct_set(T_FUNCTION, F_LEARN);
    s.return_();
    s.end();
    s.local_get(F)
        .struct_get(T_FUNCTION, F_LEARN)
        .ref_cast_non_null(HeapType::Concrete(T_VALS))
        .local_tee(L);
    s.i32_const(1)
        .array_get(T_VALS)
        .ref_cast_non_null(HeapType::I31)
        .i31_get_u()
        .i32_const(1)
        .i32_add()
        .local_tee(C);
    s.i32_const(LEARN_AFTER).i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(L).i32_const(1).local_get(C).ref_i31().array_set(T_VALS);
    s.return_();
    s.end();
    s.block(BlockType::Empty);
    // The first object.
    s.local_get(L).i32_const(0).array_get(T_VALS).local_tee(THIS);
    s.ref_test_non_null(HeapType::Concrete(runtime::T_PLAIN)).i32_eqz().br_if(0);
    s.local_get(THIS).ref_cast_non_null(HeapType::Concrete(runtime::T_PLAIN)).local_tee(O);
    s.struct_get(T_OBJECT, object::F_INDEX).ref_is_null().i32_eqz().br_if(0);
    s.local_get(O).struct_get(T_OBJECT, object::F_COUNT).local_tee(N).i32_eqz().br_if(0);
    s.local_get(N).i32_const(object::SMALL).i32_gt_u().br_if(0);
    // Live keys, default attributes.
    s.i32_const(0).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(O)
        .struct_get(T_OBJECT, object::F_KEYS)
        .local_get(I)
        .array_get(T_KEYS)
        .ref_is_null()
        .br_if(2);
    s.local_get(O)
        .struct_get(T_OBJECT, object::F_ATTRS)
        .local_get(I)
        .array_get_u(T_BYTES)
        .br_if(2);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    // The keys, then the index: a power of 2 at least 2 * n (as `layout_index`), entry `e + 1` at the
    // first free slot from `hash & mask`.
    s.local_get(N).array_new_default(T_KEYS).local_tee(KEYS);
    s.i32_const(0);
    s.local_get(O).struct_get(T_OBJECT, object::F_KEYS);
    s.i32_const(0).local_get(N);
    s.array_copy(T_KEYS, T_KEYS);
    s.i32_const(2).local_set(CAP);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(CAP).local_get(N).i32_const(1).i32_shl().i32_ge_u().br_if(1);
    s.local_get(CAP).i32_const(1).i32_shl().local_set(CAP);
    s.br(0).end().end();
    s.local_get(CAP).array_new_default(T_I32S).local_set(IDX);
    s.i32_const(0).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(KEYS)
        .local_get(I)
        .array_get(T_KEYS)
        .ref_as_non_null()
        .call(hash)
        .local_get(CAP)
        .i32_const(1)
        .i32_sub()
        .i32_and()
        .local_set(J);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(IDX).local_get(J).array_get(T_I32S).i32_eqz().br_if(1);
    s.local_get(J)
        .i32_const(1)
        .i32_add()
        .local_get(CAP)
        .i32_const(1)
        .i32_sub()
        .i32_and()
        .local_set(J);
    s.br(0).end().end();
    s.local_get(IDX)
        .local_get(J)
        .local_get(I)
        .i32_const(1)
        .i32_add()
        .array_set(T_I32S);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    runtime::count(&mut s, on, "learn.layout");
    s.local_get(F);
    s.local_get(IDX);
    s.local_get(KEYS);
    s.local_get(N).array_new_default(T_BYTES);
    s.array_new_fixed(T_VALS, 3);
    s.struct_set(T_FUNCTION, F_LEARN);
    s.local_get(F).i32_const(learned).struct_set(T_FUNCTION, F_CONSTRUCT);
    s.return_();
    s.end();
    runtime::count(&mut s, on, "learn.none");
    s.local_get(F).ref_null(wasm_encoder::RefType::EQREF.heap_type).struct_set(T_FUNCTION, F_LEARN);
    s.local_get(F).i32_const(ordinary).struct_set(T_FUNCTION, F_CONSTRUCT);
    s.end();
    f
}

fn class_heritage(cx: &mut dyn Cx) -> Function {
    let get = oh(cx, object::Helper::Get);
    let concat = cx.helper(runtime::Helper::Concat);
    let to_string = cx.helper(runtime::Helper::ToString);
    let desc = cx.helper(runtime::Helper::Prop(crate::props::Helper::Desc));
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let prototype = cx.str("prototype");
    let (extends, not_ctor, bad_proto) = (
        cx.str("Class extends value "),
        cx.str(" is not a constructor or null"),
        cx.str("Class extends value does not have valid prototype property "),
    );
    const V: u32 = 0;
    const P: u32 = 1;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    // How node shows the value: objects as `#<Name>`.
    let show = |s: &mut wasm_encoder::InstructionSink, v: u32| {
        s.test_object(v);
        s.if_(BlockType::Result(runtime::string_ref()));
        s.local_get(v)
            .cast_object()
            .call(desc);
        s.else_();
        s.local_get(v).call(to_string);
        s.end();
    };
    s.local_get(V).global_get(G_NULL).ref_eq();
    s.if_(BlockType::Empty);
    s.local_get(V).return_();
    s.end();
    s.local_get(V).ref_test_non_null(function());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(V)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_CONSTRUCT)
        .i32_const(-1)
        .i32_eq();
    s.else_();
    s.i32_const(1);
    s.end();
    s.if_(BlockType::Empty);
    s.global_get(extends);
    show(&mut s, V);
    s.call(concat);
    s.global_get(not_ctor).call(concat);
    s.call(throw);
    s.end();
    s.local_get(V).global_get(prototype).call(get).local_tee(P);
    s.is_object();
    s.local_get(P).global_get(G_NULL).ref_eq();
    s.i32_or().i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(bad_proto);
    show(&mut s, P);
    s.call(concat);
    s.call(throw);
    s.end();
    s.local_get(P);
    s.end();
    f
}

fn class_setup(cx: &mut dyn Cx) -> Function {
    let alloc = oh(cx, object::Helper::Alloc);
    let define = oh(cx, object::Helper::Define);
    let (prototype, constructor) = (cx.str("prototype"), cx.str("constructor"));
    const C: u32 = 0;
    const PARENT: u32 = 1;
    const PP: u32 = 2;
    const P: u32 = 3;
    let mut f = Function::new([(1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.local_get(PARENT).ref_test_non_null(function());
    s.if_(BlockType::Empty);
    s.local_get(C).cast_object();
    s.local_get(PARENT).cast_object();
    s.struct_set(T_OBJECT, object::F_PROTO);
    s.end();
    s.i32_const(object::CLASS_PLAIN);
    s.test_object(PP);
    s.if_(BlockType::Result(rt(true, T_OBJECT)));
    s.local_get(PP).cast_object();
    s.else_();
    s.ref_null(HeapType::Concrete(T_OBJECT));
    s.end();
    s.call(alloc).local_set(P);
    s.local_get(P).ref_as_non_null().global_get(constructor);
    s.local_get(C).i32_const(object::HIDDEN).call(define);
    s.local_get(C).cast_object();
    s.global_get(prototype).local_get(P);
    s.i32_const(object::HIDDEN | object::READONLY | object::FIXED)
        .call(define);
    s.local_get(P);
    s.end();
    f
}

fn instance_of_value(cx: &mut dyn Cx) -> Function {
    let get = oh(cx, object::Helper::Get);
    let fn_prototype = fh(cx, Helper::FnPrototype);
    let concat = cx.helper(runtime::Helper::Concat);
    let to_string = cx.helper(runtime::Helper::ToString);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let not_callable = cx.str("Right-hand side of 'instanceof' is not callable");
    let not_object = cx.str("Right-hand side of 'instanceof' is not an object");
    let (non_object, check) = (
        cx.str("Function has non-object prototype '"),
        cx.str("' in instanceof check"),
    );
    const V: u32 = 0;
    const F: u32 = 1;
    const P: u32 = 2;
    const O: u32 = 3;
    let mut f = Function::new([(1, jsval()), (1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    if cx.names("hasInstance") {
        // `f[Symbol.hasInstance](v)` first (T25; only when the code names it).
        let is_nullish = cx.helper(runtime::Helper::IsNullish);
        let to_boolean = cx.helper(runtime::Helper::ToBoolean);
        let call_value = fh(cx, Helper::CallValue);
        let sym = cx.helper(runtime::Helper::WellKnownSymbol(
            js2wasi3_ir::well_known_symbol("hasInstance").expect("well-known"),
        ));
        let what = cx.str("[Symbol.hasInstance]");
        s.test_object(F);
        s.if_(BlockType::Empty);
        s.local_get(F).call(sym).call(get).local_tee(P);
        s.call(is_nullish).i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(P).local_get(F);
        s.local_get(V).array_new_fixed(T_VALS, 1);
        s.global_get(what).call(call_value);
        s.call(to_boolean).return_();
        s.end();
        s.end();
    }
    s.local_get(F).ref_test_non_null(function()).i32_eqz();
    s.if_(BlockType::Empty);
    // A primitive: "not an object" (V8); an object that isn't a function: "not callable".
    s.global_get(not_callable);
    s.global_get(not_object);
    s.test_object(F);
    s.typed_select(rt(false, runtime::T_STRING));
    s.call(throw);
    s.end();
    // A bound function: its target's (InstanceofOperator on [[BoundTargetFunction]]).
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(F)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_KIND)
        .i32_const(KIND_BOUND)
        .i32_ne()
        .br_if(1);
    s.local_get(F)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_ENV)
        .ref_as_non_null()
        .i32_const(0)
        .array_get(T_VALS)
        .local_set(F);
    s.br(0).end().end();
    // V as an object in `O` (T93: one chain, no `is_subtype` for an array or function instance), or false.
    runtime::object_or(&mut s, V, |s| {
        s.i32_const(0).return_();
    });
    s.local_set(O);
    s.local_get(F).call(fn_prototype).local_set(P);
    s.test_object(P).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(non_object);
    s.local_get(P).call(to_string).call(concat);
    s.global_get(check).call(concat);
    s.call(throw);
    s.end();
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, object::F_PROTO)
        .local_set(O);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(O).ref_is_null().br_if(1);
    s.local_get(O).local_get(P).ref_eq();
    s.if_(BlockType::Empty);
    s.i32_const(1).return_();
    s.end();
    s.local_get(O)
        .ref_as_non_null()
        .struct_get(T_OBJECT, object::F_PROTO)
        .local_set(O);
    s.br(0).end().end();
    s.i32_const(0);
    s.end();
    f
}

fn ctor_return(cx: &mut dyn Cx, derived: bool) -> Function {
    let throw_type = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let throw_ref = oh(cx, object::Helper::Throw(ErrorKind::ReferenceError));
    let only = cx.str("Derived constructors may only return object or undefined");
    let uninit = cx.str(UNINIT_THIS);
    const V: u32 = 0;
    const THIS: u32 = 1;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.test_object(V);
    s.if_(BlockType::Empty);
    s.local_get(V).return_();
    s.end();
    if derived {
        s.local_get(V).global_get(G_UNDEFINED).ref_eq().i32_eqz();
        s.if_(BlockType::Empty);
        s.global_get(only).call(throw_type);
        s.end();
        s.local_get(THIS).ref_is_null();
        s.if_(BlockType::Empty);
        s.global_get(uninit).call(throw_ref);
        s.end();
    }
    s.local_get(THIS);
    s.end();
    f
}

fn error_ctor(cx: &mut dyn Cx, k: ErrorKind) -> Function {
    let new = fh(cx, Helper::New);
    let proto = oh(cx, object::Helper::Proto(Builtin::Error(k)));
    let define = oh(cx, object::Helper::Define);
    let call = fh(cx, Helper::ErrorCall(k));
    let construct = fh(cx, Helper::ErrorConstruct(k));
    cx.declare_func(call);
    cx.declare_func(construct);
    let (name, prototype, constructor) =
        (cx.str(k.name()), cx.str("prototype"), cx.str("constructor"));
    let g = match ErrorKind::ALL.iter().position(|x| *x == k) {
        Some(i) => runtime::G_ERROR_CTORS + i as u32,
        None => runtime::G_SUPPRESSED_CTOR, // T45
    };
    const F: u32 = 0;
    const P: u32 = 1;
    let mut f = Function::new([(1, jsval()), (1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.global_get(g).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(g).return_();
    s.end();
    s.i32_const(cx.code_slot(call)).i32_const(cx.code_slot(construct));
    s.ref_null(HeapType::Concrete(T_VALS));
    let length = match k {
        ErrorKind::AggregateError => 2,
        ErrorKind::SuppressedError => 3,
        _ => 1,
    };
    s.global_get(name).i32_const(length).i32_const(0);
    s.call(new).local_tee(F);
    s.cast_object().global_set(g);
    s.call(proto).local_set(P);
    s.local_get(F).cast_object();
    s.global_get(prototype).local_get(P);
    s.i32_const(object::HIDDEN | object::READONLY | object::FIXED)
        .call(define);
    s.local_get(P).ref_as_non_null().global_get(constructor);
    s.local_get(F).i32_const(object::HIDDEN).call(define);
    s.local_get(F);
    s.end();
    f
}

/// node's ReferenceError for `this` before `super()` in a derived constructor.
pub const UNINIT_THIS: &str = "Must call super constructor in derived class before accessing 'this' or returning from derived constructor";

fn call_value(cx: &mut dyn Cx) -> Function {
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let not_fn = cx.str(" is not a function");
    const F: u32 = 0;
    const THIS: u32 = 1;
    const ARGS: u32 = 2;
    const WHAT: u32 = 3;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(F).ref_test_non_null(function()).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(WHAT)
        .global_get(not_fn)
        .call(concat)
        .call(throw);
    s.unreachable();
    s.end();
    s.local_get(F).local_get(THIS).local_get(ARGS);
    s.local_get(F)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_CODE);
    s.call_indirect(0, T_CODE);
    s.end();
    f
}

fn arg_get() -> Function {
    const ARGS: u32 = 0;
    const I: u32 = 1;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(I);
    s.local_get(ARGS).ref_cast_non_null(vals()).array_len();
    s.i32_lt_u();
    s.if_(BlockType::Result(jsval()));
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .local_get(I)
        .array_get(T_VALS);
    s.else_();
    s.global_get(G_UNDEFINED);
    s.end();
    s.end();
    f
}

/// `Function.prototype.call`: `this` is the function to call.
fn call(cx: &mut dyn Cx) -> Function {
    let call_value = fh(cx, Helper::CallValue);
    let what = cx.str("Function.prototype.call called on a value that");
    const THIS: u32 = 1;
    const ARGS: u32 = 2;
    const N: u32 = 3;
    const REST: u32 = 4;
    let mut f = Function::new([(1, ValType::I32), (1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .array_len()
        .local_set(N);
    // rest = args[1..]
    s.i32_const(0);
    s.local_get(N).i32_const(1).i32_sub();
    s.local_get(N).i32_eqz().select();
    s.array_new_default(T_VALS).local_set(REST);
    s.local_get(N).i32_const(1).i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(REST).ref_as_non_null().i32_const(0);
    s.local_get(ARGS).ref_cast_non_null(vals()).i32_const(1);
    s.local_get(N).i32_const(1).i32_sub();
    s.array_copy(T_VALS, T_VALS);
    s.end();
    s.local_get(THIS);
    s.local_get(N).i32_eqz();
    s.if_(BlockType::Result(jsval()));
    s.global_get(G_UNDEFINED);
    s.else_();
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .i32_const(0)
        .array_get(T_VALS);
    s.end();
    s.local_get(REST).ref_as_non_null();
    s.global_get(what);
    s.call(call_value);
    s.end();
    f
}

fn apply(cx: &mut dyn Cx) -> Function {
    let call_list = fh(cx, Helper::CallList);
    let what = cx.str("Function.prototype.apply was called on a value that");
    const THIS: u32 = 1;
    const ARGS: u32 = 2;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(THIS);
    for i in 0..2 {
        s.i32_const(i);
        s.local_get(ARGS).ref_cast_non_null(vals()).array_len();
        s.i32_lt_u();
        s.if_(BlockType::Result(jsval()));
        s.local_get(ARGS)
            .ref_cast_non_null(vals())
            .i32_const(i)
            .array_get(T_VALS);
        s.else_();
        s.global_get(G_UNDEFINED);
        s.end();
    }
    s.global_get(what);
    s.call(call_list);
    s.end();
    f
}

fn call_list(cx: &mut dyn Cx) -> Function {
    let list = fh(cx, Helper::ListFromArrayLike);
    let call_value = fh(cx, Helper::CallValue);
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).local_get(1);
    s.local_get(2).call(list);
    s.local_get(3).call(call_value);
    s.end();
    f
}

/// Arrays copy their elements (holes: `undefined`); other objects: `length` and indices.
fn list_from_array_like(cx: &mut dyn Cx) -> Function {
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let get = oh(cx, object::Helper::Get);
    let to_number = cx.helper(runtime::Helper::ToNumber);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let not_obj = cx.str("CreateListFromArrayLike called on non-object");
    let length = cx.str("length");
    const V: u32 = 0;
    const N: u32 = 1;
    const I: u32 = 2;
    const OUT: u32 = 3;
    const E: u32 = 4;
    let mut f = Function::new([(2, ValType::I32), (1, rt(true, T_VALS)), (1, jsval())]);
    let mut s = f.instructions();
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.i32_const(0).array_new_default(T_VALS).return_();
    s.end();
    s.test_object(V)
        .i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(not_obj).call(throw);
    s.end();
    s.local_get(V).ref_test_non_null(crate::array::array());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(V)
        .ref_cast_non_null(crate::array::array())
        .struct_get(runtime::T_ARRAY, crate::array::F_LENGTH);
    s.else_();
    s.local_get(V).global_get(length).call(get).call(to_number);
    s.i32_trunc_sat_f64_u();
    s.end();
    s.local_tee(N).array_new_default(T_VALS).local_set(OUT);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(OUT).ref_as_non_null().local_get(I);
    s.local_get(V).local_get(I).ref_i31().call(get).local_tee(E);
    s.array_set(T_VALS);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(OUT).ref_as_non_null();
    s.end();
    f
}

/// `Function.prototype.bind(thisArg, ...args)` (T25): `this` is the target.
fn bind(cx: &mut dyn Cx) -> Function {
    let new = fh(cx, Helper::New);
    let call = fh(cx, Helper::BoundCall);
    let construct = fh(cx, Helper::BoundConstruct);
    cx.declare_func(call);
    cx.declare_func(construct);
    let get = oh(cx, object::Helper::Get);
    let is_number = cx.helper(runtime::Helper::IsNumber);
    let to_number = cx.helper(runtime::Helper::ToNumber);
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let (length, name, bound, not_fn) = (
        cx.str("length"),
        cx.str("name"),
        cx.str("bound "),
        cx.str("Bind must be called on a function"),
    );
    const THIS: u32 = 1;
    const ARGS: u32 = 2;
    const N: u32 = 3;
    const ENV: u32 = 4;
    const L: u32 = 5;
    const X: u32 = 6;
    const NAME: u32 = 7;
    let mut f = Function::new([
        (1, ValType::I32),
        (1, rt(true, T_VALS)),
        (1, ValType::F64),
        (1, jsval()),
        (1, rt(true, runtime::T_STRING)),
    ]);
    let mut s = f.instructions();
    s.local_get(THIS).ref_test_non_null(function()).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(not_fn).call(throw);
    s.end();
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .array_len()
        .local_set(N);
    // env = [target, thisArg, ...args[1..]]
    s.local_get(N).i32_const(1).i32_add();
    s.i32_const(2);
    s.local_get(N).i32_const(1).i32_gt_u();
    s.select();
    s.array_new_default(T_VALS).local_tee(ENV);
    s.i32_const(0).local_get(THIS).array_set(T_VALS);
    s.local_get(ENV).i32_const(1);
    s.local_get(N);
    s.if_(BlockType::Result(jsval()));
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .i32_const(0)
        .array_get(T_VALS);
    s.else_();
    s.global_get(G_UNDEFINED);
    s.end();
    s.array_set(T_VALS);
    s.local_get(N).i32_const(1).i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(ENV).ref_as_non_null().i32_const(2);
    s.local_get(ARGS).ref_cast_non_null(vals()).i32_const(1);
    s.local_get(N).i32_const(1).i32_sub();
    s.array_copy(T_VALS, T_VALS);
    s.end();
    // length: max(0, ToIntegerOrInfinity(target.length) - bound args), if it's a number.
    s.f64_const(0.0.into()).local_set(L);
    s.local_get(THIS).global_get(length).call(get).local_tee(X);
    s.call(is_number);
    s.if_(BlockType::Empty);
    s.local_get(X).call(to_number).f64_trunc();
    s.local_get(N).i32_const(1).i32_sub().i32_const(0);
    s.local_get(N).i32_const(1).i32_gt_u();
    s.select();
    s.f64_convert_i32_u().f64_sub();
    s.f64_const(0.0.into()).f64_max();
    // (A NaN length is 0; the result is kept as an `i31`.)
    s.f64_const(1073741823.0.into()).f64_min();
    s.local_set(L);
    s.end();
    // name: "bound " + target.name (if a string).
    s.local_get(THIS).global_get(name).call(get).local_set(X);
    runtime::is_string(&mut s, X);
    s.if_(BlockType::Result(string_ref()));
    s.global_get(bound);
    s.local_get(X)
        .ref_cast_non_null(runtime::string())
        .call(concat);
    s.else_();
    s.global_get(bound);
    s.end();
    s.local_set(NAME);
    s.i32_const(cx.code_slot(call));
    s.local_get(THIS)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_CONSTRUCT)
        .i32_const(-1)
        .i32_eq();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(-1);
    s.else_();
    s.i32_const(cx.code_slot(construct));
    s.end();
    s.local_get(ENV);
    s.local_get(NAME).ref_as_non_null();
    s.local_get(L).i32_trunc_sat_f64_u();
    s.i32_const(NEW_BOUND);
    s.call(new);
    s.end();
    f
}

/// A bound function's code (`construct`: its construct code): the bound arguments, then these.
fn bound_call(cx: &mut dyn Cx, construct: bool) -> Function {
    let call_value = fh(cx, Helper::CallValue);
    let construct_fn = fh(cx, Helper::Construct);
    let what = cx.str("bound function");
    const SELF: u32 = 0;
    const THIS: u32 = 1;
    const ARGS: u32 = 2;
    const ENV: u32 = 3;
    const ALL: u32 = 4;
    const NB: u32 = 5;
    const NA: u32 = 6;
    let mut f = Function::new([(2, rt(true, T_VALS)), (2, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(SELF)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_ENV)
        .local_tee(ENV);
    s.ref_as_non_null()
        .array_len()
        .i32_const(2)
        .i32_sub()
        .local_set(NB);
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .array_len()
        .local_set(NA);
    s.local_get(NB).local_get(NA).i32_add();
    s.array_new_default(T_VALS).local_set(ALL);
    s.local_get(ALL).ref_as_non_null().i32_const(0);
    s.local_get(ENV).ref_as_non_null().i32_const(2);
    s.local_get(NB);
    s.array_copy(T_VALS, T_VALS);
    s.local_get(ALL).ref_as_non_null().local_get(NB);
    s.local_get(ARGS).ref_cast_non_null(vals()).i32_const(0);
    s.local_get(NA);
    s.array_copy(T_VALS, T_VALS);
    // The target.
    s.local_get(ENV)
        .ref_as_non_null()
        .i32_const(0)
        .array_get(T_VALS);
    if construct {
        s.local_get(ALL).ref_as_non_null();
        // new_target: the target if it's this bound function.
        s.local_get(THIS).local_get(SELF).ref_eq();
        s.if_(BlockType::Result(jsval()));
        s.local_get(ENV)
            .ref_as_non_null()
            .i32_const(0)
            .array_get(T_VALS);
        s.else_();
        s.local_get(THIS);
        s.end();
        s.global_get(what);
        s.call(construct_fn);
    } else {
        s.local_get(ENV)
            .ref_as_non_null()
            .i32_const(1)
            .array_get(T_VALS);
        s.local_get(ALL).ref_as_non_null();
        s.global_get(what);
        s.call(call_value);
    }
    s.end();
    f
}

/// `Function.prototype.toString` (a `$code`, T25): `this`'s source text, else (built-ins, bound
/// functions) `function name() { [native code] }` with its `name` (none for a bound function).
fn to_string(cx: &mut dyn Cx) -> Function {
    let get = oh(cx, object::Helper::Get);
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let not_function = cx.str("Function.prototype.toString requires that 'this' be a Function");
    let (open, close, anonymous) = (
        cx.str("function "),
        cx.str("() { [native code] }"),
        cx.str("function () { [native code] }"),
    );
    let name = cx.str("name");
    const THIS: u32 = 1;
    const F: u32 = 3;
    const N: u32 = 4;
    let mut f = Function::new([(1, rt(true, T_FUNCTION)), (1, jsval())]);
    let mut s = f.instructions();
    s.local_get(THIS).ref_test_non_null(function()).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(not_function).call(throw);
    s.end();
    s.local_get(THIS).ref_cast_non_null(function()).local_tee(F);
    s.struct_get(T_FUNCTION, F_SOURCE);
    s.br_on_non_null(0);
    s.local_get(F)
        .struct_get(T_FUNCTION, F_KIND)
        .i32_const(KIND_BOUND)
        .i32_eq();
    s.if_(BlockType::Empty);
    s.global_get(anonymous).return_();
    s.end();
    s.local_get(THIS).global_get(name).call(get).local_set(N);
    runtime::is_string(&mut s, N);
    s.if_(BlockType::Result(string_ref()));
    s.global_get(open);
    s.local_get(N)
        .ref_cast_non_null(runtime::string())
        .call(concat);
    s.global_get(close).call(concat);
    s.else_();
    s.global_get(anonymous);
    s.end();
    s.end();
    f
}

/// SetFunctionName (F59): `f.name = prefix + key`, a symbol key as `[description]` (empty for no
/// description).
fn set_name(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(runtime::Helper::ToString);
    let concat = cx.helper(runtime::Helper::Concat);
    let define = oh(cx, object::Helper::Define);
    let name = cx.str("name");
    let (open, close) = (cx.str("["), cx.str("]"));
    const F: u32 = 0;
    const KEY: u32 = 1;
    const PREFIX: u32 = 2;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(F).cast_object();
    s.global_get(name);
    s.local_get(PREFIX).ref_cast_non_null(runtime::string());
    s.local_get(KEY).ref_test_non_null(runtime::symbol());
    s.if_(BlockType::Result(string_ref()));
    {
        s.local_get(KEY)
            .ref_cast_non_null(runtime::string())
            .array_len()
            .i32_eqz();
        s.if_(BlockType::Result(string_ref()));
        s.array_new_fixed(runtime::T_STRING, 0);
        s.else_();
        s.global_get(open);
        s.local_get(KEY)
            .ref_cast_non_null(runtime::string())
            .call(concat);
        s.global_get(close).call(concat);
        s.end();
    }
    s.else_();
    s.local_get(KEY).call(to_string);
    s.end();
    s.call(concat);
    s.i32_const(object::HIDDEN | object::READONLY).call(define);
    s.local_get(F).end();
    f
}

/// A built-in constructor as a value (T25): calls run its prelude code (`Map`/`Set` throw without
/// `new`), `new` runs it too (`String`/`Number`/`Boolean` make wrapper objects, T30); `prototype` is the
/// built-in prototype, whose `constructor` it becomes; the prelude statics are own methods.
fn builtin_ctor(cx: &mut dyn Cx, b: Builtin) -> Function {
    // A stub (only reached through `constructor`): its code traps.
    let linked = cx.builtin_ctor(b).is_some();
    let (code, length) = cx.builtin_ctor(b).unwrap_or_else(|| {
        (
            fh(cx, Helper::StubTrap),
            b.ctor_length().expect("a built-in constructor"),
        )
    });
    let new = fh(cx, Helper::New);
    let proto = oh(cx, object::Helper::Proto(b));
    let define = oh(cx, object::Helper::Define);
    cx.declare_func(code);
    let call = match b {
        Builtin::Map
        | Builtin::Set
        | Builtin::ArrayBuffer
        | Builtin::Typed(_)
        | Builtin::DisposableStack
        | Builtin::AsyncDisposableStack => fh(cx, Helper::RequiresNew(b)),
        // `Date()` gives a string (T26).
        Builtin::Date if linked => cx.date_call().expect("`$$DateCall` with `$$Date`"),
        _ => code,
    };
    let construct = match b {
        Builtin::String | Builtin::Number | Builtin::Boolean if linked => Some(cx.helper(
            runtime::Helper::Wrap(crate::wrapper::Helper::Construct(b)),
        )),
        // `new BigInt` throws (T40; it has [[Construct]]: `Reflect.construct(f, [], BigInt)`).
        // T49: `new Symbol` too.
        Builtin::BigInt | Builtin::Symbol => Some(fh(cx, Helper::NotConstructor(b))),
        // T126: `Array`/`Object`/`RegExp` too (`class X extends Array`: `super()` gets `new.target`'s prototype).
        Builtin::Map
        | Builtin::Set
        | Builtin::ArrayBuffer
        | Builtin::Typed(_)
        | Builtin::Array
        | Builtin::Object
        | Builtin::RegExp
            if linked && cx.subclassing() =>
        {
            Some(fh(cx, Helper::ConstructFrom(b)))
        }
        _ => Some(code),
    };
    cx.declare_func(call);
    if let Some(c) = construct {
        cx.declare_func(c);
    }
    let statics: Vec<(u32, u32, u32, Option<u32>)> = cx
        .builtin_statics(b)
        .into_iter()
        .map(|(key, code, len)| {
            cx.declare_func(code);
            match key.strip_prefix('$').and_then(js2wasi3_ir::well_known_symbol) {
                Some(k) => {
                    let sym = cx.helper(runtime::Helper::WellKnownSymbol(k));
                    (cx.str(&format!("get [Symbol.{}]", &key[1..])), code, len, Some(sym))
                }
                None => (cx.str(&key), code, len, None),
            }
        })
        .collect();
    // T121: `%TypedArray%` is named `TypedArray`; a typed-array constructor's [[Prototype]] is it.
    let (name, prototype, constructor) = (
        cx.str(if b == Builtin::TypedArray { "TypedArray" } else { b.name() }),
        cx.str("prototype"),
        cx.str("constructor"),
    );
    let typed_parent = matches!(b, Builtin::Typed(_)).then(|| fh(cx, Helper::BuiltinCtor(Builtin::TypedArray)));
    let g = runtime::ctor_global(b);
    const F: u32 = 0;
    const P: u32 = 1;
    let mut f = Function::new([(1, jsval()), (1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.global_get(g).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(g).return_();
    s.end();
    s.i32_const(cx.code_slot(call));
    match construct {
        Some(c) => s.i32_const(cx.code_slot(c)),
        None => s.i32_const(-1),
    };
    s.ref_null(HeapType::Concrete(T_VALS));
    s.global_get(name).i32_const(length as i32).i32_const(0);
    s.call(new).local_tee(F);
    s.cast_object().global_set(g);
    if let Some(parent) = typed_parent {
        s.local_get(F).cast_object();
        s.call(parent).cast_object();
        s.struct_set(T_OBJECT, object::F_PROTO);
    }
    s.call(proto).local_set(P);
    s.local_get(F).cast_object();
    s.global_get(prototype).local_get(P);
    s.i32_const(object::HIDDEN | object::READONLY | object::FIXED)
        .call(define);
    s.local_get(P).ref_as_non_null().global_get(constructor);
    s.local_get(F).i32_const(object::HIDDEN).call(define);
    if b == Builtin::Number {
        define_constants(cx, &mut s, F, &js2wasi3_ir::NUMBER_CONSTANTS);
    }
    if let Builtin::Typed(k) = b {
        // T20.
        let size = js2wasi3_ir::TYPED_SIZES[k as usize] as f64;
        define_constants(cx, &mut s, F, &[("BYTES_PER_ELEMENT", size)]);
    }
    if b == Builtin::Symbol {
        // T49: the well-known symbols the code names; read-only, non-enumerable, fixed.
        for name in cx.symbol_statics() {
            let key = cx.str(&name);
            let value = match js2wasi3_ir::well_known_symbol(&name) {
                Some(k) => Some(cx.helper(runtime::Helper::WellKnownSymbol(k))),
                None => None,
            };
            s.local_get(F)
                .cast_object()
                .global_get(key);
            match value {
                Some(h) => s.call(h),
                None => s.global_get(runtime::G_SYM_ITERATOR),
            };
            s.i32_const(object::HIDDEN | object::READONLY | object::FIXED)
                .call(define);
        }
    }
    for (key, code, len, symbol) in statics {
        if let Some(sym) = symbol {
            // T121: a `$name` static is a getter at `Symbol.name` (`%TypedArray%[Symbol.species]`).
            let define_accessor = cx.helper(runtime::Helper::Prop(crate::props::Helper::DefineAccessor));
            s.local_get(F).cast_object();
            s.call(sym);
            s.i32_const(cx.code_slot(code));
            s.i32_const(-1);
            s.ref_null(HeapType::Concrete(T_VALS));
            s.global_get(key).i32_const(len as i32).i32_const(0);
            s.call(new);
            s.i32_const(0).i32_const(object::HIDDEN).call(define_accessor);
            continue;
        }
        s.local_get(F)
            .cast_object()
            .global_get(key);
        s.i32_const(cx.code_slot(code));
        s.i32_const(-1);
        s.ref_null(HeapType::Concrete(T_VALS));
        s.global_get(key).i32_const(len as i32).i32_const(0);
        s.call(new);
        s.i32_const(object::HIDDEN).call(define);
    }
    s.local_get(F);
    s.end();
    f
}

/// Define `constants` on the object in local `o`: read-only, non-enumerable, non-configurable.
pub(crate) fn define_constants(
    cx: &mut dyn Cx,
    s: &mut wasm_encoder::InstructionSink,
    o: u32,
    constants: &[(&str, f64)],
) {
    let define = oh(cx, object::Helper::Define);
    let box_ = cx.helper(runtime::Helper::Box);
    for (key, v) in constants {
        let key = cx.str(key);
        s.local_get(o)
            .cast_object()
            .global_get(key);
        s.f64_const((*v).into()).call(box_);
        s.i32_const(object::HIDDEN | object::READONLY | object::FIXED)
            .call(define);
    }
}

/// `Math`/`JSON` as a value (T25): a plain object (class [`object::CLASS_MATH`] /
/// [`object::CLASS_JSON`] for `console.log`) with `Math`'s constants and the prelude functions in
/// use, all non-enumerable.
fn namespace(cx: &mut dyn Cx, ns: js2wasi3_ir::Namespace) -> Function {
    use js2wasi3_ir::Namespace;
    let new = fh(cx, Helper::New);
    let alloc = oh(cx, object::Helper::Alloc);
    let obj_proto = oh(cx, object::Helper::Proto(Builtin::Object));
    let define = oh(cx, object::Helper::Define);
    let methods: Vec<(u32, u32, u32)> = cx
        .namespace_methods(ns)
        .into_iter()
        .map(|(key, code, len)| {
            cx.declare_func(code);
            (cx.str(&key), code, len)
        })
        .collect();
    let g = runtime::G_NAMESPACES + ns.index();
    const O: u32 = 0;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.global_get(g).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(g).return_();
    s.end();
    s.i32_const(match ns {
        Namespace::Math => object::CLASS_MATH,
        Namespace::Json => object::CLASS_JSON,
        Namespace::Reflect => object::CLASS_REFLECT,
    });
    s.call(obj_proto).call(alloc).local_tee(O);
    s.cast_object().global_set(g);
    if ns == Namespace::Math {
        define_constants(cx, &mut s, O, &js2wasi3_ir::MATH_CONSTANTS);
    }
    if cx.names("toStringTag") {
        object::define_tag(cx, &mut s, O, ns.name());
    }
    for (key, code, len) in methods {
        s.local_get(O)
            .cast_object()
            .global_get(key);
        s.i32_const(cx.code_slot(code));
        s.i32_const(-1);
        s.ref_null(HeapType::Concrete(T_VALS));
        s.global_get(key).i32_const(len as i32).i32_const(0);
        s.call(new);
        s.i32_const(object::HIDDEN).call(define);
    }
    s.local_get(O);
    s.end();
    f
}
