//! Promises and the job queue (T17, design D3).
//!
//! # Layout
//! `$promise` is a final subtype of `$object`: the seven object fields, then `state` ([`PENDING`],
//! [`FULFILLED`], [`REJECTED`]), `result`, `reactions` (a `$vals` of `$reaction`s, first `count` used,
//! Wasm null when empty) and `handled`. A `$reaction { derived, on_fulfilled, on_rejected }` is one
//! PromiseReaction pair: `derived` is the promise `then` returned (Wasm null: none), a handler that
//! isn't callable is `undefined`.
//!
//! # Jobs
//! One queue for every job (`G_JOBS`, 4 slots per job: kind, then up to three values), run in
//! order by [`Helper::Drain`] until empty, including jobs queued meanwhile:
//! - [`JOB_FULFILL`] / [`JOB_REJECT`] (reaction, argument): PromiseReactionJob;
//! - [`JOB_THENABLE`] (promise, thenable, then): PromiseResolveThenableJob;
//! - [`JOB_CALLBACK`] (callback): `queueMicrotask`. It isn't caught: an exception ends the drain
//!   and reaches `run` (an uncaught exception, like node).
//!
//! # Unhandled rejections
//! A promise rejected while it has no handler is recorded (`G_REJECTED`); [`Helper::Unhandled`]
//! gives the reason of the first recorded one that still has none (node checks after draining the
//! microtasks, then exits with the error).
//!
//! Not modelled: subclassing through species (`then` and the statics always make `%Promise%`
//! promises), `Symbol.toStringTag`, `Promise.any` (AggregateError).

use js2wasi3_ir::{Builtin, ErrorKind};
use wasm_encoder::{BlockType, Catch, Function, HeapType, InstructionSink, ValType};

use crate::func::{self, F_ENV, function, vals};
use crate::runtime::{
    self, Cx, G_FALSE, G_JOB_HEAD, G_JOB_TAIL, G_JOBS, G_PROMISE_CTOR, G_QUEUE_MICROTASK,
    G_REJECTED, G_REJECTED_N, G_TRUE, G_UNDEFINED, T_BYTES, T_CODE, T_FUNCTION, T_I32S, T_KEYS,
    T_OBJECT, T_PROMISE, T_REACTION, T_VALS, TAG, jsval, rt, string_ref,
};
use crate::{array, iter, object, props};

/// `$promise` field indices after the object's.
pub const P_STATE: u32 = 7;
pub const P_RESULT: u32 = 8;
pub const P_REACTIONS: u32 = 9;
pub const P_COUNT: u32 = 10;
pub const P_HANDLED: u32 = 11;

pub const PENDING: i32 = 0;
pub const FULFILLED: i32 = 1;
pub const REJECTED: i32 = 2;

/// `$reaction` fields.
const R_DERIVED: u32 = 0;
const R_FULFILLED: u32 = 1;
const R_REJECTED: u32 = 2;

/// Job kinds.
pub const JOB_FULFILL: i32 = 0;
pub const JOB_REJECT: i32 = 1;
pub const JOB_THENABLE: i32 = 2;
pub const JOB_CALLBACK: i32 = 3;

pub fn promise() -> HeapType {
    HeapType::Concrete(T_PROMISE)
}

/// The combinators of the `Promise` constructor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Combinator {
    All,
    AllSettled,
    Race,
}

impl Combinator {
    fn name(self) -> &'static str {
        match self {
            Combinator::All => "all",
            Combinator::AllSettled => "allSettled",
            Combinator::Race => "race",
        }
    }
}

/// The per-element functions of the combinators.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Element {
    /// `Promise.all` Resolve Element Functions.
    All,
    /// `Promise.allSettled` Resolve / Reject Element Functions.
    Fulfilled,
    Rejected,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `() -> jsval`: the `Promise` constructor (made on first use, with its statics and its
    /// prototype's `constructor`).
    Ctor,
    /// Its code (a `$code`): the TypeError for calling it without `new`.
    CtorCall,
    /// Its construct code: `new Promise(executor)`.
    CtorConstruct,
    /// `() -> (ref $promise)`: a new pending promise (prototype `Promise.prototype`).
    New,
    /// `(p: (ref $promise)) -> (resolve, reject)`: CreateResolvingFunctions.
    Resolvers,
    /// The code of a resolve function (`env`: the promise, then `true` once used, shared with
    /// the reject function).
    ResolveFn,
    /// The code of a reject function.
    RejectFn,
    /// `(p: (ref $promise), resolution)`: resolve `p` (the steps after the "already resolved" check).
    Resolve,
    /// `(p: (ref $promise), v)`: FulfillPromise (`rejected: false`) / RejectPromise.
    Settle { rejected: bool },
    /// `(p: (ref $promise), on_fulfilled, on_rejected, derived: (ref null $promise))`:
    /// PerformPromiseThen.
    PerformThen,
    /// The code of `Promise.prototype.then`.
    Then,
    /// `Promise.prototype.catch`.
    Catch,
    /// `Promise.prototype.finally`.
    Finally,
    /// Then Finally / Catch Finally functions (`env`: `onFinally`).
    FinallyFn { catch: bool },
    /// The value thunk / thrower of `finally` (`env`: the value).
    Thunk { throw: bool },
    /// `(x) -> jsval`: PromiseResolve(%Promise%, x).
    PromiseResolve,
    /// The code of `Promise.resolve`.
    StaticResolve,
    /// `Promise.reject`.
    StaticReject,
    /// `Promise.all` / `allSettled` / `race`.
    Combine(Combinator),
    /// A combinator's element function (`env`: the shared state `[values, remaining, resolve,
    /// reject]`, the index, then `true` once called).
    Element(Element),
    /// `(o, key: (ref $string), args: (ref $vals)) -> jsval`: `o[key](...args)`.
    Invoke,
    /// `(kind: i32, a, b, c)`: queue a job.
    Enqueue,
    /// `()`: run the queued jobs until there are none.
    Drain,
    /// `(reaction: eqref, kind: i32, argument)`: PromiseReactionJob.
    ReactionJob,
    /// `(promise, thenable, then)`: PromiseResolveThenableJob.
    ThenableJob,
    /// `() -> jsval`: the `queueMicrotask` function (made on first use).
    QueueMicrotask,
    /// Its code.
    QueueMicrotaskCall,
    /// `() -> eqref`: the reason of the first recorded rejection still unhandled (Wasm null: none);
    /// forgets the records.
    Unhandled,
    /// `(reason) -> (ref $string)`: how node reports an unhandled rejection (after "Uncaught ").
    UnhandledText,
    /// `(v) -> (ref $string)`: V8's NoSideEffectsToString, as in its TypeErrors: `#<Name>` for
    /// objects, `Symbol(x)`, ToString otherwise.
    Describe,
    /// `(v) -> (ref $string)`: V8's "... is not iterable" message for a value a built-in iterates.
    NotIterable,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Ctor => "promise_constructor",
            Helper::CtorCall => "Promise",
            Helper::CtorConstruct => "promise_construct",
            Helper::New => "new_promise",
            Helper::Resolvers => "create_resolving_functions",
            Helper::ResolveFn => "promise_resolve_function",
            Helper::RejectFn => "promise_reject_function",
            Helper::Resolve => "resolve_promise",
            Helper::Settle { rejected: false } => "fulfill_promise",
            Helper::Settle { rejected: true } => "reject_promise",
            Helper::PerformThen => "perform_promise_then",
            Helper::Then => "Promise.prototype.then",
            Helper::Catch => "Promise.prototype.catch",
            Helper::Finally => "Promise.prototype.finally",
            Helper::FinallyFn { catch: false } => "then_finally",
            Helper::FinallyFn { catch: true } => "catch_finally",
            Helper::Thunk { throw: false } => "value_thunk",
            Helper::Thunk { throw: true } => "thrower",
            Helper::PromiseResolve => "promise_resolve",
            Helper::StaticResolve => "Promise.resolve",
            Helper::StaticReject => "Promise.reject",
            Helper::Combine(Combinator::All) => "Promise.all",
            Helper::Combine(Combinator::AllSettled) => "Promise.allSettled",
            Helper::Combine(Combinator::Race) => "Promise.race",
            Helper::Element(_) => "promise_element_function",
            Helper::Invoke => "invoke",
            Helper::Enqueue => "enqueue_job",
            Helper::Drain => "drain_jobs",
            Helper::ReactionJob => "promise_reaction_job",
            Helper::ThenableJob => "promise_resolve_thenable_job",
            Helper::QueueMicrotask => "queue_microtask_function",
            Helper::QueueMicrotaskCall => "queueMicrotask",
            Helper::Unhandled => "unhandled_rejection",
            Helper::UnhandledText => "unhandled_rejection_text",
            Helper::Describe => "no_side_effects_to_string",
            Helper::NotIterable => "not_iterable_message",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        let s = string_ref();
        let p = rt(false, T_PROMISE);
        let code = (vec![v, v, v], vec![v]);
        match self {
            Helper::Ctor | Helper::QueueMicrotask => (vec![], vec![v]),
            Helper::New => (vec![], vec![p]),
            Helper::Resolvers => (vec![p], vec![v, v]),
            Helper::Resolve | Helper::Settle { .. } => (vec![p, v], vec![]),
            Helper::PerformThen => (vec![p, v, v, rt(true, T_PROMISE)], vec![]),
            Helper::PromiseResolve => (vec![v], vec![v]),
            Helper::Invoke => (vec![v, s, rt(false, T_VALS)], vec![v]),
            Helper::Enqueue => (vec![ValType::I32, v, v, v], vec![]),
            Helper::Drain => (vec![], vec![]),
            Helper::ReactionJob => (vec![v, ValType::I32, v], vec![]),
            Helper::ThenableJob => (vec![v, v, v], vec![]),
            Helper::Unhandled => (vec![], vec![v]),
            Helper::UnhandledText | Helper::Describe | Helper::NotIterable => (vec![v], vec![s]),
            Helper::CtorCall
            | Helper::CtorConstruct
            | Helper::ResolveFn
            | Helper::RejectFn
            | Helper::Then
            | Helper::Catch
            | Helper::Finally
            | Helper::FinallyFn { .. }
            | Helper::Thunk { .. }
            | Helper::StaticResolve
            | Helper::StaticReject
            | Helper::Combine(_)
            | Helper::Element(_)
            | Helper::QueueMicrotaskCall => code,
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Ctor => ctor(cx),
            Helper::CtorCall => {
                let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
                let msg = cx.str("Promise constructor cannot be invoked without 'new'");
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.global_get(msg).call(throw).unreachable();
                s.end();
                f
            }
            Helper::CtorConstruct => ctor_construct(cx),
            Helper::New => new_promise(cx),
            Helper::Resolvers => resolvers(cx),
            Helper::ResolveFn | Helper::RejectFn => resolve_fn(cx, self == Helper::RejectFn),
            Helper::Resolve => resolve(cx),
            Helper::Settle { rejected } => settle(cx, rejected),
            Helper::PerformThen => perform_then(cx),
            Helper::Then => then(cx),
            Helper::Catch => {
                let invoke = ph(cx, Helper::Invoke);
                let key = cx.str("then");
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(THIS).global_get(key);
                s.global_get(G_UNDEFINED);
                arg(cx, &mut s, 0);
                s.array_new_fixed(T_VALS, 2);
                s.call(invoke);
                s.end();
                f
            }
            Helper::Finally => finally(cx),
            Helper::FinallyFn { catch } => finally_fn(cx, catch),
            Helper::Thunk { throw } => {
                let mut f = Function::new([]);
                let mut s = f.instructions();
                env(&mut s);
                s.i32_const(0).array_get(T_VALS);
                if throw {
                    s.throw(TAG);
                }
                s.end();
                f
            }
            Helper::PromiseResolve => promise_resolve(cx),
            Helper::StaticResolve => {
                let resolve = ph(cx, Helper::PromiseResolve);
                let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
                let msg = cx.str("PromiseResolve called on non-object");
                let mut f = Function::new([]);
                let mut s = f.instructions();
                check_object(&mut s, msg, throw);
                arg(cx, &mut s, 0);
                s.call(resolve);
                s.end();
                f
            }
            Helper::StaticReject => {
                let new = ph(cx, Helper::New);
                let reject = ph(cx, Helper::Settle { rejected: true });
                const P: u32 = 3;
                let mut f = Function::new([(1, rt(true, T_PROMISE))]);
                let mut s = f.instructions();
                s.call(new).local_tee(P).ref_as_non_null();
                arg(cx, &mut s, 0);
                s.call(reject);
                s.local_get(P);
                s.end();
                f
            }
            Helper::Combine(c) => combine(cx, c),
            Helper::Element(e) => element(cx, e),
            Helper::Invoke => {
                let get = oh(cx, object::Helper::Get);
                let describe = ph(cx, Helper::Describe);
                let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
                const O: u32 = 0;
                const KEY: u32 = 1;
                const ARGS: u32 = 2;
                const F: u32 = 3;
                let mut f = Function::new([(1, jsval())]);
                let mut s = f.instructions();
                s.local_get(O).local_get(KEY).call(get).local_set(F);
                s.local_get(F).local_get(O).local_get(ARGS);
                s.local_get(F).call(describe);
                s.call(call_value);
                s.end();
                f
            }
            Helper::Enqueue => enqueue(),
            Helper::Drain => drain(cx),
            Helper::ReactionJob => reaction_job(cx),
            Helper::ThenableJob => {
                let resolvers = ph(cx, Helper::Resolvers);
                let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
                let empty = cx.str("");
                const P: u32 = 0;
                const T: u32 = 1;
                const THEN: u32 = 2;
                const RES: u32 = 3;
                const REJ: u32 = 4;
                let mut f = Function::new([(2, jsval())]);
                let mut s = f.instructions();
                s.local_get(P).ref_cast_non_null(promise()).call(resolvers);
                s.local_set(REJ).local_set(RES);
                try_catch(
                    &mut s,
                    |s| {
                        s.local_get(THEN).local_get(T);
                        s.local_get(RES).local_get(REJ).array_new_fixed(T_VALS, 2);
                        s.global_get(empty).call(call_value).drop();
                    },
                    |s| {
                        let e = RES;
                        s.local_set(e);
                        s.local_get(REJ).global_get(G_UNDEFINED);
                        s.local_get(e).array_new_fixed(T_VALS, 1);
                        s.global_get(empty).call(call_value).drop();
                    },
                );
                s.end();
                f
            }
            Helper::QueueMicrotask => {
                let code = ph(cx, Helper::QueueMicrotaskCall);
                cx.declare_func(code);
                let new = cx.helper(runtime::Helper::Func(func::Helper::New));
                let name = cx.str("queueMicrotask");
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.global_get(G_QUEUE_MICROTASK).ref_is_null().i32_eqz();
                s.if_(BlockType::Empty);
                s.global_get(G_QUEUE_MICROTASK).return_();
                s.end();
                native(&mut s, new, code, None, name, 1);
                s.ref_cast_non_null(runtime::object())
                    .global_set(G_QUEUE_MICROTASK);
                s.global_get(G_QUEUE_MICROTASK);
                s.end();
                f
            }
            Helper::QueueMicrotaskCall => queue_microtask(cx),
            Helper::Unhandled => unhandled(),
            Helper::UnhandledText => unhandled_text(cx),
            Helper::Describe => {
                let desc = cx.helper(runtime::Helper::Prop(props::Helper::Desc));
                let inspect = cx.helper(runtime::Helper::Inspect);
                let to_string = cx.helper(runtime::Helper::ToString);
                const V: u32 = 0;
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(V).ref_test_non_null(runtime::object());
                s.if_(BlockType::Empty);
                s.local_get(V)
                    .ref_cast_non_null(runtime::object())
                    .call(desc);
                s.return_();
                s.end();
                s.local_get(V).ref_test_non_null(runtime::symbol());
                s.if_(BlockType::Empty);
                s.local_get(V).call(inspect).return_();
                s.end();
                s.local_get(V).call(to_string);
                s.end();
                f
            }
            Helper::NotIterable => {
                let is_nullish = cx.helper(runtime::Helper::IsNullish);
                let to_string = cx.helper(runtime::Helper::ToString);
                let typeof_string = cx.helper(runtime::Helper::TypeofString);
                let describe = ph(cx, Helper::Describe);
                let concat = cx.helper(runtime::Helper::Concat);
                let (space, tail) = (
                    cx.str(" "),
                    cx.str(" is not iterable (cannot read property Symbol(Symbol.iterator))"),
                );
                const V: u32 = 0;
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(V).call(is_nullish);
                s.if_(BlockType::Result(string_ref()));
                s.local_get(V).call(to_string);
                s.else_();
                s.local_get(V).call(typeof_string);
                s.local_get(V).ref_test_non_null(runtime::object());
                s.if_(BlockType::Result(string_ref()));
                s.global_get(tail);
                s.else_();
                s.global_get(space);
                s.local_get(V).call(describe).call(concat);
                s.global_get(tail).call(concat);
                s.end();
                s.call(concat);
                s.return_();
                s.end();
                s.global_get(tail).call(concat);
                s.end();
                f
            }
        }
    }
}

/// `$code` parameters.
const SELF: u32 = 0;
const THIS: u32 = 1;
const ARGS: u32 = 2;

fn oh(cx: &mut dyn Cx, h: object::Helper) -> u32 {
    cx.helper(runtime::Helper::Obj(h))
}

fn ph(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Prom(h))
}

/// Push argument `i` of a `$code` (`undefined` past the end).
fn arg(cx: &mut dyn Cx, s: &mut InstructionSink, i: i32) {
    let arg_get = cx.helper(runtime::Helper::Func(func::Helper::ArgGet));
    s.local_get(ARGS).i32_const(i).call(arg_get);
}

/// Push the environment of a `$code`'s function (a `(ref $vals)`).
fn env(s: &mut InstructionSink) {
    s.local_get(SELF)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_ENV)
        .ref_as_non_null();
}

/// Push a new function object (not a constructor) running `code`, with the environment in local
/// `env` (or none).
fn native(s: &mut InstructionSink, new: u32, code: u32, env: Option<u32>, name: u32, len: i32) {
    s.ref_func(code);
    s.ref_null(HeapType::Concrete(T_CODE));
    match env {
        Some(e) => s.local_get(e),
        None => s.ref_null(vals()),
    };
    s.global_get(name).i32_const(len).i32_const(0).call(new);
}

/// Throw TypeError `msg` unless `this` is an object.
fn check_object(s: &mut InstructionSink, msg: u32, throw: u32) {
    s.local_get(THIS)
        .ref_test_non_null(runtime::object())
        .i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(msg).call(throw);
    s.end();
}

/// `try { body } catch (e) { handler }`: `handler` starts with the exception on the stack. Neither
/// leaves a value; `br` depths inside `body` are 3 deeper, inside `handler` 1 deeper.
fn try_catch(
    s: &mut InstructionSink,
    body: impl FnOnce(&mut InstructionSink),
    handler: impl FnOnce(&mut InstructionSink),
) {
    s.block(BlockType::Empty);
    s.block(BlockType::Result(jsval()));
    s.try_table(BlockType::Empty, [Catch::One { tag: TAG, label: 0 }]);
    body(s);
    s.end();
    s.br(1);
    s.end();
    handler(s);
    s.end();
}

fn new_promise(cx: &mut dyn Cx) -> Function {
    let proto = oh(cx, object::Helper::Proto(Builtin::Promise));
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.i32_const(object::CLASS_PLAIN).call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    s.i32_const(PENDING).global_get(G_UNDEFINED);
    s.ref_null(vals()).i32_const(0).i32_const(0);
    s.struct_new(T_PROMISE);
    s.end();
    f
}

fn ctor(cx: &mut dyn Cx) -> Function {
    let new = cx.helper(runtime::Helper::Func(func::Helper::New));
    let proto = oh(cx, object::Helper::Proto(Builtin::Promise));
    let define = oh(cx, object::Helper::Define);
    let call = ph(cx, Helper::CtorCall);
    let construct = ph(cx, Helper::CtorConstruct);
    cx.declare_func(call);
    cx.declare_func(construct);
    let statics: Vec<(u32, u32, i32)> = [
        (Helper::Combine(Combinator::All), "all"),
        (Helper::Combine(Combinator::AllSettled), "allSettled"),
        (Helper::Combine(Combinator::Race), "race"),
        (Helper::StaticResolve, "resolve"),
        (Helper::StaticReject, "reject"),
    ]
    .into_iter()
    .map(|(h, name)| {
        let code = ph(cx, h);
        cx.declare_func(code);
        (code, cx.str(name), 1)
    })
    .collect();
    let (name, prototype, constructor) = (
        cx.str("Promise"),
        cx.str("prototype"),
        cx.str("constructor"),
    );
    const F: u32 = 0;
    const P: u32 = 1;
    let mut f = Function::new([(1, jsval()), (1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.global_get(G_PROMISE_CTOR).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(G_PROMISE_CTOR).return_();
    s.end();
    s.ref_func(call).ref_func(construct);
    s.ref_null(vals());
    s.global_get(name).i32_const(1).i32_const(0);
    s.call(new).local_tee(F);
    s.ref_cast_non_null(runtime::object())
        .global_set(G_PROMISE_CTOR);
    s.call(proto).local_set(P);
    s.local_get(F).ref_cast_non_null(runtime::object());
    s.global_get(prototype).local_get(P);
    s.i32_const(object::HIDDEN | object::READONLY | object::FIXED)
        .call(define);
    s.local_get(P).ref_as_non_null().global_get(constructor);
    s.local_get(F).i32_const(object::HIDDEN).call(define);
    for (code, name, len) in statics {
        s.local_get(F).ref_cast_non_null(runtime::object());
        s.global_get(name);
        native(&mut s, new, code, None, name, len);
        s.i32_const(object::HIDDEN).call(define);
    }
    s.local_get(F);
    s.end();
    f
}

/// Set the prototype of the promise in local `p` from `new_target` (a subclass), like
/// OrdinaryCreateFromConstructor.
fn proto_from_new_target(cx: &mut dyn Cx, s: &mut InstructionSink, p: u32, tmp: u32) {
    let get = oh(cx, object::Helper::Get);
    let prototype = cx.str("prototype");
    const NT: u32 = 1;
    s.local_get(NT).local_get(SELF).ref_eq().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(NT)
        .global_get(prototype)
        .call(get)
        .local_tee(tmp);
    s.ref_test_non_null(runtime::object());
    s.if_(BlockType::Empty);
    s.local_get(p).ref_cast_non_null(runtime::object());
    s.local_get(tmp).ref_cast_non_null(runtime::object());
    s.struct_set(T_OBJECT, object::F_PROTO);
    s.end();
    s.end();
}

fn ctor_construct(cx: &mut dyn Cx) -> Function {
    let new = ph(cx, Helper::New);
    let resolvers = ph(cx, Helper::Resolvers);
    let describe = ph(cx, Helper::Describe);
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let (resolver, not_fn, empty) = (
        cx.str("Promise resolver "),
        cx.str(" is not a function"),
        cx.str(""),
    );
    const EX: u32 = 3;
    const P: u32 = 4;
    const RES: u32 = 5;
    const REJ: u32 = 6;
    let mut f = Function::new([(1, jsval()), (1, rt(true, T_PROMISE)), (2, jsval())]);
    let mut s = f.instructions();
    arg(cx, &mut s, 0);
    s.local_tee(EX).ref_test_non_null(function()).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(resolver);
    s.local_get(EX).call(describe).call(concat);
    s.global_get(not_fn).call(concat);
    s.call(throw);
    s.end();
    s.call(new).local_set(P);
    proto_from_new_target(cx, &mut s, P, RES);
    s.local_get(P).ref_as_non_null().call(resolvers);
    s.local_set(REJ).local_set(RES);
    try_catch(
        &mut s,
        |s| {
            s.local_get(EX).global_get(G_UNDEFINED);
            s.local_get(RES).local_get(REJ).array_new_fixed(T_VALS, 2);
            s.global_get(empty).call(call_value).drop();
        },
        |s| {
            s.local_set(EX);
            s.local_get(REJ).global_get(G_UNDEFINED);
            s.local_get(EX).array_new_fixed(T_VALS, 1);
            s.global_get(empty).call(call_value).drop();
        },
    );
    s.local_get(P);
    s.end();
    f
}

fn resolvers(cx: &mut dyn Cx) -> Function {
    let new = cx.helper(runtime::Helper::Func(func::Helper::New));
    let resolve = ph(cx, Helper::ResolveFn);
    let reject = ph(cx, Helper::RejectFn);
    cx.declare_func(resolve);
    cx.declare_func(reject);
    let empty = cx.str("");
    const P: u32 = 0;
    const ENV: u32 = 1;
    let mut f = Function::new([(1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    s.local_get(P).global_get(G_FALSE);
    s.array_new_fixed(T_VALS, 2).local_set(ENV);
    native(&mut s, new, resolve, Some(ENV), empty, 1);
    native(&mut s, new, reject, Some(ENV), empty, 1);
    s.end();
    f
}

fn resolve_fn(cx: &mut dyn Cx, reject: bool) -> Function {
    let settle = if reject {
        ph(cx, Helper::Settle { rejected: true })
    } else {
        ph(cx, Helper::Resolve)
    };
    const ENV: u32 = 3;
    let mut f = Function::new([(1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    env(&mut s);
    s.local_tee(ENV).i32_const(1).array_get(T_VALS);
    s.global_get(G_TRUE).ref_eq();
    s.if_(BlockType::Empty);
    s.global_get(G_UNDEFINED).return_();
    s.end();
    s.local_get(ENV).ref_as_non_null().i32_const(1);
    s.global_get(G_TRUE).array_set(T_VALS);
    s.local_get(ENV)
        .ref_as_non_null()
        .i32_const(0)
        .array_get(T_VALS);
    s.ref_cast_non_null(promise());
    arg(cx, &mut s, 0);
    s.call(settle);
    s.global_get(G_UNDEFINED);
    s.end();
    f
}

fn resolve(cx: &mut dyn Cx) -> Function {
    let fulfill = ph(cx, Helper::Settle { rejected: false });
    let reject = ph(cx, Helper::Settle { rejected: true });
    let enqueue = ph(cx, Helper::Enqueue);
    let get = oh(cx, object::Helper::Get);
    let new_error = oh(cx, object::Helper::NewError(ErrorKind::TypeError));
    let (cycle, then) = (
        cx.str("Chaining cycle detected for promise #<Promise>"),
        cx.str("then"),
    );
    const P: u32 = 0;
    const X: u32 = 1;
    const THEN: u32 = 2;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.local_get(X).local_get(P).ref_eq();
    s.if_(BlockType::Empty);
    s.local_get(P);
    s.global_get(cycle).global_get(G_UNDEFINED).call(new_error);
    s.call(reject).return_();
    s.end();
    s.local_get(X)
        .ref_test_non_null(runtime::object())
        .i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(P).local_get(X).call(fulfill).return_();
    s.end();
    try_catch(
        &mut s,
        |s| {
            s.local_get(X).global_get(then).call(get).local_set(THEN);
        },
        |s| {
            s.local_set(THEN);
            s.local_get(P).local_get(THEN).call(reject).return_();
        },
    );
    s.local_get(THEN).ref_test_non_null(function()).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(P).local_get(X).call(fulfill).return_();
    s.end();
    s.i32_const(JOB_THENABLE);
    s.local_get(P).local_get(X).local_get(THEN);
    s.call(enqueue);
    s.end();
    f
}

fn settle(cx: &mut dyn Cx, rejected: bool) -> Function {
    let enqueue = ph(cx, Helper::Enqueue);
    const P: u32 = 0;
    const V: u32 = 1;
    const R: u32 = 2;
    const G: u32 = 3;
    const N: u32 = 4;
    const I: u32 = 5;
    const L: u32 = 6;
    let mut f = Function::new([(2, rt(true, T_VALS)), (3, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(P).struct_get(T_PROMISE, P_STATE);
    s.i32_const(PENDING).i32_ne();
    s.if_(BlockType::Empty);
    s.return_();
    s.end();
    s.local_get(P)
        .struct_get(T_PROMISE, P_REACTIONS)
        .local_set(R);
    s.local_get(P).struct_get(T_PROMISE, P_COUNT).local_set(N);
    s.local_get(P)
        .i32_const(if rejected { REJECTED } else { FULFILLED })
        .struct_set(T_PROMISE, P_STATE);
    s.local_get(P).local_get(V).struct_set(T_PROMISE, P_RESULT);
    s.local_get(P)
        .ref_null(vals())
        .struct_set(T_PROMISE, P_REACTIONS);
    s.local_get(P).i32_const(0).struct_set(T_PROMISE, P_COUNT);
    if rejected {
        // HostPromiseRejectionTracker(p, "reject"): record it.
        s.local_get(P).struct_get(T_PROMISE, P_HANDLED).i32_eqz();
        s.if_(BlockType::Empty);
        s.global_get(G_REJECTED).ref_is_null();
        s.if_(BlockType::Result(ValType::I32));
        s.i32_const(1);
        s.else_();
        s.global_get(G_REJECTED_N);
        s.global_get(G_REJECTED).ref_as_non_null().array_len();
        s.i32_eq();
        s.end();
        s.if_(BlockType::Empty);
        // Grow: double (at least 8).
        s.global_get(G_REJECTED_N)
            .i32_const(1)
            .i32_shl()
            .local_tee(L);
        s.i32_const(8).local_get(L).i32_const(8).i32_gt_u().select();
        s.array_new_default(T_VALS).local_set(G);
        s.global_get(G_REJECTED_N).i32_eqz().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(G).ref_as_non_null().i32_const(0);
        s.global_get(G_REJECTED).ref_as_non_null().i32_const(0);
        s.global_get(G_REJECTED_N);
        s.array_copy(T_VALS, T_VALS);
        s.end();
        s.local_get(G).global_set(G_REJECTED);
        s.end();
        s.global_get(G_REJECTED).ref_as_non_null();
        s.global_get(G_REJECTED_N).local_get(P).array_set(T_VALS);
        s.global_get(G_REJECTED_N)
            .i32_const(1)
            .i32_add()
            .global_set(G_REJECTED_N);
        s.end();
    }
    // TriggerPromiseReactions, in the order they were added.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.i32_const(if rejected { JOB_REJECT } else { JOB_FULFILL });
    s.local_get(R)
        .ref_as_non_null()
        .local_get(I)
        .array_get(T_VALS);
    s.local_get(V).global_get(G_UNDEFINED);
    s.call(enqueue);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.end();
    f
}

fn perform_then(cx: &mut dyn Cx) -> Function {
    let enqueue = ph(cx, Helper::Enqueue);
    const P: u32 = 0;
    const ONF: u32 = 1;
    const ONR: u32 = 2;
    const D: u32 = 3;
    const RX: u32 = 4;
    const STATE: u32 = 5;
    const N: u32 = 6;
    const R: u32 = 7;
    let mut f = Function::new([
        (1, rt(true, T_REACTION)),
        (2, ValType::I32),
        (1, rt(true, T_VALS)),
    ]);
    let mut s = f.instructions();
    s.local_get(D);
    for h in [ONF, ONR] {
        s.local_get(h).global_get(G_UNDEFINED);
        s.local_get(h).ref_test_non_null(function());
        s.typed_select(jsval());
    }
    s.struct_new(T_REACTION).local_set(RX);
    s.local_get(P)
        .struct_get(T_PROMISE, P_STATE)
        .local_tee(STATE);
    s.i32_const(PENDING).i32_eq();
    s.if_(BlockType::Empty);
    {
        s.local_get(P).struct_get(T_PROMISE, P_COUNT).local_set(N);
        s.local_get(P)
            .struct_get(T_PROMISE, P_REACTIONS)
            .local_tee(R);
        s.ref_is_null();
        s.if_(BlockType::Result(ValType::I32));
        s.i32_const(1);
        s.else_();
        s.local_get(N)
            .local_get(R)
            .ref_as_non_null()
            .array_len()
            .i32_eq();
        s.end();
        s.if_(BlockType::Empty);
        // Grow: double (at least 2).
        s.local_get(N).i32_const(1).i32_shl().i32_const(2);
        s.local_get(N).i32_const(1).i32_ge_u().select();
        s.array_new_default(T_VALS);
        s.local_set(R);
        s.local_get(N).i32_eqz().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(R).ref_as_non_null().i32_const(0);
        s.local_get(P)
            .struct_get(T_PROMISE, P_REACTIONS)
            .ref_as_non_null();
        s.i32_const(0).local_get(N);
        s.array_copy(T_VALS, T_VALS);
        s.end();
        s.local_get(P)
            .local_get(R)
            .struct_set(T_PROMISE, P_REACTIONS);
        s.end();
        s.local_get(R)
            .ref_as_non_null()
            .local_get(N)
            .local_get(RX)
            .array_set(T_VALS);
        s.local_get(P)
            .local_get(N)
            .i32_const(1)
            .i32_add()
            .struct_set(T_PROMISE, P_COUNT);
    }
    s.else_();
    {
        s.i32_const(JOB_FULFILL).i32_const(JOB_REJECT);
        s.local_get(STATE).i32_const(FULFILLED).i32_eq();
        s.select();
        s.local_get(RX);
        s.local_get(P).struct_get(T_PROMISE, P_RESULT);
        s.global_get(G_UNDEFINED);
        s.call(enqueue);
    }
    s.end();
    // HostPromiseRejectionTracker(p, "handle"): the record is skipped once handled.
    s.local_get(P).i32_const(1).struct_set(T_PROMISE, P_HANDLED);
    s.end();
    f
}

fn then(cx: &mut dyn Cx) -> Function {
    let new = ph(cx, Helper::New);
    let perform = ph(cx, Helper::PerformThen);
    let describe = ph(cx, Helper::Describe);
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let msg = cx.str("Method Promise.prototype.then called on incompatible receiver ");
    const D: u32 = 3;
    let mut f = Function::new([(1, rt(true, T_PROMISE))]);
    let mut s = f.instructions();
    s.local_get(THIS).ref_test_non_null(promise()).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(msg);
    s.local_get(THIS).call(describe).call(concat);
    s.call(throw);
    s.end();
    s.call(new).local_set(D);
    s.local_get(THIS).ref_cast_non_null(promise());
    arg(cx, &mut s, 0);
    arg(cx, &mut s, 1);
    s.local_get(D).call(perform);
    s.local_get(D);
    s.end();
    f
}

fn finally(cx: &mut dyn Cx) -> Function {
    let new = cx.helper(runtime::Helper::Func(func::Helper::New));
    let invoke = ph(cx, Helper::Invoke);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let then_finally = ph(cx, Helper::FinallyFn { catch: false });
    let catch_finally = ph(cx, Helper::FinallyFn { catch: true });
    cx.declare_func(then_finally);
    cx.declare_func(catch_finally);
    let (msg, then, empty) = (
        cx.str("Promise.prototype.finally called on non-object"),
        cx.str("then"),
        cx.str(""),
    );
    const F: u32 = 3;
    const ENV: u32 = 4;
    const TF: u32 = 5;
    const CF: u32 = 6;
    let mut f = Function::new([(1, jsval()), (1, rt(true, T_VALS)), (2, jsval())]);
    let mut s = f.instructions();
    check_object(&mut s, msg, throw);
    arg(cx, &mut s, 0);
    s.local_tee(F).local_tee(TF).local_set(CF);
    s.local_get(F).ref_test_non_null(function());
    s.if_(BlockType::Empty);
    s.local_get(F).array_new_fixed(T_VALS, 1).local_set(ENV);
    native(&mut s, new, then_finally, Some(ENV), empty, 1);
    s.local_set(TF);
    native(&mut s, new, catch_finally, Some(ENV), empty, 1);
    s.local_set(CF);
    s.end();
    s.local_get(THIS).global_get(then);
    s.local_get(TF).local_get(CF).array_new_fixed(T_VALS, 2);
    s.call(invoke);
    s.end();
    f
}

fn finally_fn(cx: &mut dyn Cx, catch: bool) -> Function {
    let new = cx.helper(runtime::Helper::Func(func::Helper::New));
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let resolve = ph(cx, Helper::PromiseResolve);
    let invoke = ph(cx, Helper::Invoke);
    let thunk = ph(cx, Helper::Thunk { throw: catch });
    cx.declare_func(thunk);
    let (then, empty) = (cx.str("then"), cx.str(""));
    const P: u32 = 3;
    const ENV: u32 = 4;
    let mut f = Function::new([(1, jsval()), (1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    env(&mut s);
    s.i32_const(0).array_get(T_VALS);
    s.global_get(G_UNDEFINED);
    s.i32_const(0).array_new_default(T_VALS);
    s.global_get(empty).call(call_value);
    s.call(resolve).local_set(P);
    arg(cx, &mut s, 0);
    s.array_new_fixed(T_VALS, 1).local_set(ENV);
    s.local_get(P).global_get(then);
    native(&mut s, new, thunk, Some(ENV), empty, 0);
    s.array_new_fixed(T_VALS, 1);
    s.call(invoke);
    s.end();
    f
}

fn promise_resolve(cx: &mut dyn Cx) -> Function {
    let new = ph(cx, Helper::New);
    let resolve = ph(cx, Helper::Resolve);
    let ctor = ph(cx, Helper::Ctor);
    let get = oh(cx, object::Helper::Get);
    let constructor = cx.str("constructor");
    const X: u32 = 0;
    const P: u32 = 1;
    let mut f = Function::new([(1, rt(true, T_PROMISE))]);
    let mut s = f.instructions();
    s.local_get(X).ref_test_non_null(promise());
    s.if_(BlockType::Empty);
    s.local_get(X).global_get(constructor).call(get);
    s.call(ctor).ref_eq();
    s.if_(BlockType::Empty);
    s.local_get(X).return_();
    s.end();
    s.end();
    s.call(new).local_tee(P).ref_as_non_null();
    s.local_get(X).call(resolve);
    s.local_get(P);
    s.end();
    f
}

fn combine(cx: &mut dyn Cx, c: Combinator) -> Function {
    let new_function = cx.helper(runtime::Helper::Func(func::Helper::New));
    let new = ph(cx, Helper::New);
    let resolvers = ph(cx, Helper::Resolvers);
    let resolve = ph(cx, Helper::PromiseResolve);
    let invoke = ph(cx, Helper::Invoke);
    let not_iterable = ph(cx, Helper::NotIterable);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let open = cx.helper(runtime::Helper::Iter(iter::Helper::Open));
    let step = cx.helper(runtime::Helper::Iter(iter::Helper::Step));
    let close = cx.helper(runtime::Helper::Iter(iter::Helper::Close));
    let new_array = cx.helper(runtime::Helper::Arr(array::Helper::New));
    let push = cx.helper(runtime::Helper::Arr(array::Helper::Push));
    let element = match c {
        Combinator::All => Some(ph(cx, Helper::Element(Element::All))),
        Combinator::AllSettled => Some(ph(cx, Helper::Element(Element::Fulfilled))),
        Combinator::Race => None,
    };
    let rejected =
        (c == Combinator::AllSettled).then(|| ph(cx, Helper::Element(Element::Rejected)));
    for code in element.iter().chain(&rejected) {
        cx.declare_func(*code);
    }
    let (msg, then, empty) = (
        cx.str(&format!("Promise.{} called on non-object", c.name())),
        cx.str("then"),
        cx.str(""),
    );
    const D: u32 = 3;
    const RES: u32 = 4;
    const REJ: u32 = 5;
    const IT: u32 = 6;
    const STATE: u32 = 7;
    const ENV: u32 = 8;
    const I: u32 = 9;
    const NP: u32 = 10;
    const E: u32 = 11;
    let mut f = Function::new([
        (1, rt(true, T_PROMISE)),
        (3, jsval()),
        (2, rt(true, T_VALS)),
        (1, ValType::I32),
        (2, jsval()),
    ]);
    let mut s = f.instructions();
    check_object(&mut s, msg, throw);
    s.call(new).local_tee(D).ref_as_non_null();
    s.call(resolvers).local_set(REJ).local_set(RES);
    try_catch(
        &mut s,
        |s| {
            s.local_get(ARGS)
                .ref_cast_non_null(vals())
                .array_len()
                .if_(BlockType::Result(jsval()));
            s.local_get(ARGS)
                .ref_cast_non_null(vals())
                .i32_const(0)
                .array_get(T_VALS);
            s.else_();
            s.global_get(G_UNDEFINED);
            s.end();
            s.local_tee(E);
            s.local_get(E).call(not_iterable);
            s.call(open).local_set(IT);
            // The shared state: [values, remaining, resolve, reject].
            s.i32_const(0).array_new_default(T_VALS).i32_const(0);
            s.call(new_array);
            s.i32_const(1).ref_i31();
            s.local_get(RES).local_get(REJ);
            s.array_new_fixed(T_VALS, 4).local_set(STATE);
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(IT).call(step).i32_eqz().br_if(1);
            try_catch(
                s,
                |s| {
                    s.local_get(IT)
                        .ref_cast_non_null(iter::iter())
                        .struct_get(runtime::T_ITER, iter::F_VALUE);
                    s.call(resolve).local_set(NP);
                    s.local_get(NP).global_get(then);
                    match element {
                        Some(code) => {
                            // values.push(undefined); remaining++
                            s.local_get(STATE).ref_as_non_null().i32_const(0);
                            s.array_get(T_VALS).ref_cast_non_null(array::array());
                            s.global_get(G_UNDEFINED).call(push);
                            s.local_get(STATE).ref_as_non_null().i32_const(1);
                            s.local_get(STATE).ref_as_non_null().i32_const(1);
                            s.array_get(T_VALS).ref_cast_non_null(HeapType::I31);
                            s.i31_get_s().i32_const(1).i32_add().ref_i31();
                            s.array_set(T_VALS);
                            s.local_get(STATE).local_get(I).ref_i31();
                            s.global_get(G_FALSE);
                            s.array_new_fixed(T_VALS, 3).local_set(ENV);
                            native(s, new_function, code, Some(ENV), empty, 1);
                            match rejected {
                                Some(code) => native(s, new_function, code, Some(ENV), empty, 1),
                                None => {
                                    s.local_get(REJ);
                                }
                            }
                        }
                        None => {
                            s.local_get(RES).local_get(REJ);
                        }
                    }
                    s.array_new_fixed(T_VALS, 2);
                    s.call(invoke).drop();
                },
                |s| {
                    s.local_set(E);
                    s.local_get(IT).i32_const(1).call(close);
                    s.local_get(E).throw(TAG);
                },
            );
            s.local_get(I).i32_const(1).i32_add().local_set(I);
            s.br(0).end().end();
            if element.is_some() {
                // remaining--; resolve(values) at 0.
                s.local_get(STATE).ref_as_non_null().i32_const(1);
                s.local_get(STATE).ref_as_non_null().i32_const(1);
                s.array_get(T_VALS).ref_cast_non_null(HeapType::I31);
                s.i31_get_s().i32_const(1).i32_sub().ref_i31();
                s.array_set(T_VALS);
                s.local_get(STATE).ref_as_non_null().i32_const(1);
                s.array_get(T_VALS).ref_cast_non_null(HeapType::I31);
                s.i31_get_s().i32_eqz();
                s.if_(BlockType::Empty);
                s.local_get(RES).global_get(G_UNDEFINED);
                s.local_get(STATE)
                    .ref_as_non_null()
                    .i32_const(0)
                    .array_get(T_VALS);
                s.array_new_fixed(T_VALS, 1);
                s.global_get(empty).call(call_value).drop();
                s.end();
            }
        },
        |s| {
            s.local_set(E);
            s.local_get(REJ).global_get(G_UNDEFINED);
            s.local_get(E).array_new_fixed(T_VALS, 1);
            s.global_get(empty).call(call_value).drop();
        },
    );
    s.local_get(D);
    s.end();
    f
}

fn element(cx: &mut dyn Cx, e: Element) -> Function {
    let set_index = cx.helper(runtime::Helper::Arr(array::Helper::SetIndex));
    let new_object = oh(cx, object::Helper::NewObject);
    let define = oh(cx, object::Helper::DefineProp);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let empty = cx.str("");
    let settled = match e {
        Element::All => None,
        Element::Fulfilled => Some((cx.str("fulfilled"), cx.str("value"))),
        Element::Rejected => Some((cx.str("rejected"), cx.str("reason"))),
    };
    let status = cx.str("status");
    const ENV: u32 = 3;
    const STATE: u32 = 4;
    const X: u32 = 5;
    const O: u32 = 6;
    let mut f = Function::new([(2, rt(true, T_VALS)), (2, jsval())]);
    let mut s = f.instructions();
    env(&mut s);
    s.local_tee(ENV).i32_const(2).array_get(T_VALS);
    s.global_get(G_TRUE).ref_eq();
    s.if_(BlockType::Empty);
    s.global_get(G_UNDEFINED).return_();
    s.end();
    s.local_get(ENV).ref_as_non_null().i32_const(2);
    s.global_get(G_TRUE).array_set(T_VALS);
    s.local_get(ENV)
        .ref_as_non_null()
        .i32_const(0)
        .array_get(T_VALS);
    s.ref_cast_non_null(vals()).local_set(STATE);
    arg(cx, &mut s, 0);
    s.local_set(X);
    if let Some((what, key)) = settled {
        s.call(new_object).local_set(O);
        s.local_get(O)
            .global_get(status)
            .global_get(what)
            .call(define);
        s.local_get(O).global_get(key).local_get(X).call(define);
        s.local_get(O).local_set(X);
    }
    s.local_get(STATE)
        .ref_as_non_null()
        .i32_const(0)
        .array_get(T_VALS);
    s.ref_cast_non_null(array::array());
    s.local_get(ENV)
        .ref_as_non_null()
        .i32_const(1)
        .array_get(T_VALS);
    s.ref_cast_non_null(HeapType::I31).i31_get_s();
    s.local_get(X).call(set_index);
    // remaining--; resolve(values) at 0.
    s.local_get(STATE).ref_as_non_null().i32_const(1);
    s.local_get(STATE).ref_as_non_null().i32_const(1);
    s.array_get(T_VALS).ref_cast_non_null(HeapType::I31);
    s.i31_get_s().i32_const(1).i32_sub().ref_i31();
    s.array_set(T_VALS);
    s.local_get(STATE).ref_as_non_null().i32_const(1);
    s.array_get(T_VALS).ref_cast_non_null(HeapType::I31);
    s.i31_get_s().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(STATE)
        .ref_as_non_null()
        .i32_const(2)
        .array_get(T_VALS);
    s.global_get(G_UNDEFINED);
    s.local_get(STATE)
        .ref_as_non_null()
        .i32_const(0)
        .array_get(T_VALS);
    s.array_new_fixed(T_VALS, 1);
    s.global_get(empty).call(call_value).drop();
    s.end();
    s.global_get(G_UNDEFINED);
    s.end();
    f
}

fn enqueue() -> Function {
    const K: u32 = 0;
    const A: u32 = 1;
    const B: u32 = 2;
    const C: u32 = 3;
    const Q: u32 = 4;
    const L: u32 = 5;
    let mut f = Function::new([(1, rt(true, T_VALS)), (1, ValType::I32)]);
    let mut s = f.instructions();
    s.global_get(G_JOBS).local_tee(Q).ref_is_null();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(1);
    s.else_();
    s.global_get(G_JOB_TAIL).i32_const(4).i32_add();
    s.local_get(Q).ref_as_non_null().array_len();
    s.i32_gt_u();
    s.end();
    s.if_(BlockType::Empty);
    {
        // Full: move the pending jobs to the front, or grow (double, at least 64 slots).
        s.global_get(G_JOB_HEAD).i32_eqz();
        s.if_(BlockType::Empty);
        s.global_get(G_JOB_TAIL).i32_const(1).i32_shl().local_tee(L);
        s.i32_const(64)
            .local_get(L)
            .i32_const(64)
            .i32_gt_u()
            .select();
        s.array_new_default(T_VALS);
        s.global_set(G_JOBS);
        s.local_get(Q).ref_is_null().i32_eqz();
        s.if_(BlockType::Empty);
        s.global_get(G_JOBS).ref_as_non_null().i32_const(0);
        s.local_get(Q).ref_as_non_null().i32_const(0);
        s.global_get(G_JOB_TAIL);
        s.array_copy(T_VALS, T_VALS);
        s.end();
        s.else_();
        s.local_get(Q).ref_as_non_null().i32_const(0);
        s.local_get(Q).ref_as_non_null().global_get(G_JOB_HEAD);
        s.global_get(G_JOB_TAIL).global_get(G_JOB_HEAD).i32_sub();
        s.array_copy(T_VALS, T_VALS);
        s.global_get(G_JOB_TAIL)
            .global_get(G_JOB_HEAD)
            .i32_sub()
            .global_set(G_JOB_TAIL);
        s.i32_const(0).global_set(G_JOB_HEAD);
        s.end();
    }
    s.end();
    for (k, v) in [(0, None), (1, Some(A)), (2, Some(B)), (3, Some(C))] {
        s.global_get(G_JOBS).ref_as_non_null();
        s.global_get(G_JOB_TAIL).i32_const(k).i32_add();
        match v {
            Some(v) => s.local_get(v),
            None => s.local_get(K).ref_i31(),
        };
        s.array_set(T_VALS);
    }
    s.global_get(G_JOB_TAIL)
        .i32_const(4)
        .i32_add()
        .global_set(G_JOB_TAIL);
    s.end();
    f
}

fn drain(cx: &mut dyn Cx) -> Function {
    let reaction_job = ph(cx, Helper::ReactionJob);
    let thenable_job = ph(cx, Helper::ThenableJob);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let empty = cx.str("");
    const Q: u32 = 0;
    const H: u32 = 1;
    const K: u32 = 2;
    const A: u32 = 3;
    const B: u32 = 4;
    const C: u32 = 5;
    let mut f = Function::new([(1, rt(true, T_VALS)), (2, ValType::I32), (3, jsval())]);
    let mut s = f.instructions();
    s.loop_(BlockType::Empty);
    s.global_get(G_JOB_HEAD).global_get(G_JOB_TAIL).i32_ge_u();
    s.if_(BlockType::Empty);
    s.i32_const(0).global_set(G_JOB_HEAD);
    s.i32_const(0).global_set(G_JOB_TAIL);
    s.return_();
    s.end();
    s.global_get(G_JOBS).local_set(Q);
    s.global_get(G_JOB_HEAD).local_set(H);
    for (k, local) in [(1, A), (2, B), (3, C)] {
        s.local_get(Q).ref_as_non_null();
        s.local_get(H).i32_const(k).i32_add();
        s.array_get(T_VALS).local_set(local);
    }
    s.local_get(Q)
        .ref_as_non_null()
        .local_get(H)
        .array_get(T_VALS);
    s.ref_cast_non_null(HeapType::I31).i31_get_s().local_set(K);
    // Forget the slots (they may hold garbage for long).
    s.local_get(Q).ref_as_non_null().local_get(H);
    s.ref_null(wasm_encoder::RefType::EQREF.heap_type)
        .i32_const(4);
    s.array_fill(T_VALS);
    s.local_get(H).i32_const(4).i32_add().global_set(G_JOB_HEAD);
    s.local_get(K).i32_const(JOB_THENABLE).i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(A).local_get(K).local_get(B).call(reaction_job);
    s.else_();
    s.local_get(K).i32_const(JOB_THENABLE).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(A).local_get(B).local_get(C).call(thenable_job);
    s.else_();
    s.local_get(A).global_get(G_UNDEFINED);
    s.i32_const(0).array_new_default(T_VALS);
    s.global_get(empty).call(call_value).drop();
    s.end();
    s.end();
    s.br(0);
    s.end();
    s.end();
    f
}

fn reaction_job(cx: &mut dyn Cx) -> Function {
    let resolve = ph(cx, Helper::Resolve);
    let reject = ph(cx, Helper::Settle { rejected: true });
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let empty = cx.str("");
    const RX: u32 = 0;
    const K: u32 = 1;
    const ARG: u32 = 2;
    const H: u32 = 3;
    const D: u32 = 4;
    const V: u32 = 5;
    let mut f = Function::new([(1, jsval()), (1, rt(true, T_PROMISE)), (1, jsval())]);
    let mut s = f.instructions();
    s.local_get(RX)
        .ref_cast_non_null(HeapType::Concrete(T_REACTION));
    s.struct_get(T_REACTION, R_DERIVED).local_set(D);
    s.local_get(RX)
        .ref_cast_non_null(HeapType::Concrete(T_REACTION));
    s.struct_get(T_REACTION, R_FULFILLED);
    s.local_get(RX)
        .ref_cast_non_null(HeapType::Concrete(T_REACTION));
    s.struct_get(T_REACTION, R_REJECTED);
    s.local_get(K).i32_eqz();
    s.typed_select(jsval()).local_tee(H);
    s.global_get(G_UNDEFINED).ref_eq();
    s.if_(BlockType::Empty);
    {
        // No handler: pass the value / reason on.
        s.local_get(D).ref_is_null();
        s.if_(BlockType::Empty);
        s.return_();
        s.end();
        s.local_get(K).i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(D)
            .ref_as_non_null()
            .local_get(ARG)
            .call(resolve);
        s.else_();
        s.local_get(D).ref_as_non_null().local_get(ARG).call(reject);
        s.end();
        s.return_();
    }
    s.end();
    try_catch(
        &mut s,
        |s| {
            s.local_get(H).global_get(G_UNDEFINED);
            s.local_get(ARG).array_new_fixed(T_VALS, 1);
            s.global_get(empty).call(call_value).local_set(V);
        },
        |s| {
            s.local_set(V);
            s.local_get(D).ref_is_null();
            s.if_(BlockType::Empty);
            s.local_get(V).throw(TAG);
            s.end();
            s.local_get(D).ref_as_non_null().local_get(V).call(reject);
            s.return_();
        },
    );
    s.local_get(D).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(D).ref_as_non_null().local_get(V).call(resolve);
    s.end();
    s.end();
    f
}

fn queue_microtask(cx: &mut dyn Cx) -> Function {
    let enqueue = ph(cx, Helper::Enqueue);
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let to_string = cx.helper(runtime::Helper::ToString);
    let typeof_string = cx.helper(runtime::Helper::TypeofString);
    let inspect = cx.helper(runtime::Helper::Inspect);
    let ctor_name = cx.helper(runtime::Helper::Prop(props::Helper::CtorName));
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let (msg, instance, type_, open, close, quote) = (
        cx.str("The \"callback\" argument must be of type function. Received "),
        cx.str("an instance of "),
        cx.str("type "),
        cx.str(" ("),
        cx.str(")"),
        cx.str("'"),
    );
    let (array_name, object_name) = (cx.str("Array"), cx.str("Object"));
    const CB: u32 = 3;
    const N: u32 = 4;
    let mut f = Function::new([(1, jsval()), (1, rt(true, runtime::T_STRING))]);
    let mut s = f.instructions();
    arg(cx, &mut s, 0);
    s.local_tee(CB).ref_test_non_null(function()).i32_eqz();
    s.if_(BlockType::Empty);
    {
        // node's ERR_INVALID_ARG_TYPE text.
        s.global_get(msg);
        s.local_get(CB).call(is_nullish);
        s.if_(BlockType::Result(string_ref()));
        s.local_get(CB).call(to_string);
        s.else_();
        s.local_get(CB).ref_test_non_null(runtime::object());
        s.if_(BlockType::Result(string_ref()));
        s.local_get(CB)
            .ref_cast_non_null(runtime::object())
            .call(ctor_name)
            .local_tee(N)
            .ref_is_null();
        s.if_(BlockType::Result(string_ref()));
        // `Object` and `Array` aren't function objects yet (F47).
        s.local_get(CB)
            .ref_cast_non_null(runtime::object())
            .struct_get(T_OBJECT, object::F_PROTO)
            .ref_is_null();
        s.if_(BlockType::Result(string_ref()));
        s.local_get(CB).call(inspect);
        s.else_();
        s.global_get(instance);
        s.global_get(array_name).global_get(object_name);
        s.local_get(CB).ref_test_non_null(array::array());
        s.typed_select(string_ref());
        s.call(concat);
        s.end();
        s.else_();
        s.global_get(instance)
            .local_get(N)
            .ref_as_non_null()
            .call(concat);
        s.end();
        s.else_();
        s.global_get(type_);
        s.local_get(CB).call(typeof_string).call(concat);
        s.global_get(open).call(concat);
        runtime::is_string(&mut s, CB);
        s.if_(BlockType::Result(string_ref()));
        s.global_get(quote);
        s.local_get(CB).call(to_string).call(concat);
        s.global_get(quote).call(concat);
        s.else_();
        s.local_get(CB).call(inspect);
        s.end();
        s.call(concat);
        s.global_get(close).call(concat);
        s.end();
        s.end();
        s.call(concat).call(throw);
    }
    s.end();
    s.i32_const(JOB_CALLBACK).local_get(CB);
    s.global_get(G_UNDEFINED).global_get(G_UNDEFINED);
    s.call(enqueue);
    s.global_get(G_UNDEFINED);
    s.end();
    f
}

fn unhandled() -> Function {
    const I: u32 = 0;
    const P: u32 = 1;
    let mut f = Function::new([(1, ValType::I32), (1, rt(true, T_PROMISE))]);
    let mut s = f.instructions();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).global_get(G_REJECTED_N).i32_ge_u().br_if(1);
    s.global_get(G_REJECTED)
        .ref_as_non_null()
        .local_get(I)
        .array_get(T_VALS);
    s.ref_cast_non_null(promise()).local_tee(P);
    s.struct_get(T_PROMISE, P_HANDLED).i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(0).global_set(G_REJECTED_N);
    s.local_get(P)
        .ref_as_non_null()
        .struct_get(T_PROMISE, P_RESULT);
    s.return_();
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.i32_const(0).global_set(G_REJECTED_N);
    s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
    s.end();
    f
}

fn unhandled_text(cx: &mut dyn Cx) -> Function {
    let describe_error = cx.helper(runtime::Helper::Describe);
    let describe = ph(cx, Helper::Describe);
    let concat = cx.helper(runtime::Helper::Concat);
    let (head, tail) = (
        cx.str(
            "UnhandledPromiseRejection: This error originated either by throwing inside of an async \
             function without a catch block, or by rejecting a promise which was not handled with \
             .catch(). The promise rejected with the reason \"",
        ),
        cx.str("\"."),
    );
    const V: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    object::is_error(&mut s, V);
    s.if_(BlockType::Result(string_ref()));
    s.local_get(V).call(describe_error);
    s.else_();
    s.global_get(head);
    s.local_get(V).call(describe).call(concat);
    s.global_get(tail).call(concat);
    s.end();
    s.end();
    f
}
