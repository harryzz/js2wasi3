//! Lowering: checked JS AST ([`js2wasi3_frontend::Program`]) → [`js2wasi3_ir::Module`].
//!
//! # Subset (T05 numbers, T08 primitive values, T09 strings)
//! Values are numbers, booleans, strings, `undefined` and `null`. Expressions have one of three
//! IR types: `f64`, `bool` (comparisons, `!`, boolean literals) or `jsval` (anything else), and a
//! [`Kind`] saying what they are known to hold (`Num`, `Bool`, `Str` or `Any`); see [`Val`].
//!
//! **Type inference** is optimistic and driven by the lowering itself: every variable, parameter
//! and function result starts as `f64`. Each store records the kind of the stored value for its
//! slot; when a slot's kind changes (`Num` → `Bool`/`Str` → `Any`), [`lower`] runs again with that
//! knowledge until nothing changes. A slot is `f64` exactly when every value stored into it is a
//! number, so the inference can't disagree with the code it produces.
//!
//! **Boundary types** (the WIT signature of an export, see `docs/wit-names.md` and T09): a
//! parameter is `string` if its JSDoc says `@param {string}`, or, without JSDoc, if it's used as
//! a string (a `+`, `==`, `===` operand next to a known string, a template-literal interpolation,
//! a `.length` read); otherwise `f64`. The result is `string` if `@returns {string}` or every
//! returned value is a string; otherwise `f64`.
//!
//! Supported: `function` declarations (exported or not), simple parameters (missing
//! arguments are `undefined`), `var`/`let`/`const`, top-level variables with a literal
//! initializer (they become globals), number/boolean/string/`null` literals, template literals,
//! `undefined`, `NaN`, `Infinity`, `+` (addition or concatenation), `- * / %` (ToNumber),
//! unary `- + ! ~ void typeof`, `| & ^ << >> >>>`, comparisons, `=== !== == !=`, `.length`,
//! `&& || ??`, `?:`, comma, assignment and compound assignment, `++`/`--`, direct calls, `if`,
//! `while`, `do/while`, `for`, labels, `break`, `continue`, `return`.
//!
//! **Exceptions** (T12): `throw`, `try`/`catch` (with or without a binding)/`finally`. A `finally`
//! is lowered with a completion local ([`Lowerer::try_stmt`]); `break`/`continue`/`return` that
//! leave it go through it ([`Lowerer::jump`]). `new E(m, options)` / `E(m)` for the native error
//! constructors.
//!
//! **Objects** (T13): literals (identifier/string/number/computed keys, shorthand, spread,
//! `__proto__: v`), `o.k`, `o[k]`, optional member chains `o?.k`, assignment, compound assignment
//! and `++`/`--` on properties, `delete`, `in`, `for-in`, `instanceof` with `Object` or a native
//! error constructor, `Object.create`/`getPrototypeOf`/`hasOwn`, `B.prototype`. The runtime does
//! the rest (`crates/backend/src/object.rs`); property values are `jsval` of kind `Any`.
//! Converting a string to a number (`"5" * 2`) goes through the runtime's ToNumber, which calls the
//! prelude's StringToNumber (T21); `<`/`>`/`<=`/`>=` on two JS values is `Intrinsic::JsLess`
//! (strings compare by code units).
//!
//! **Functions** (T14): nested function declarations (hoisted to the start of their block), function
//! expressions, arrow functions and object-literal methods are values; so are top-level functions
//! (one object each). The analysis in [`closures`] finds what each function captures. Captured
//! variables live in cells (`let`/`const`: a new cell each time the declaration runs, so each loop
//! iteration has its own; `for (let …)` copies it per iteration; `var` and parameters: one cell per
//! call); `this` and a function expression's own name are captured by value. A function's IR
//! signature is `(self?, this?, params…)` (see [`Hidden`]): calls of a known function are direct
//! and typed; a function used as a value also gets a `$code` adapter `(self, this, args) -> jsval`
//! and its parameters become `Any`. Other calls (`o.m()` with `this = o`, `f()()`, `o?.m()`) are
//! `CallRef`s. Extra arguments are evaluated and dropped. A `let`/`const` a closure may read before
//! its declaration gets an uninitialized cell and TDZ checks. Not yet: `arguments`, `apply`/`bind`
//! (T15/T21), assigning to a function's binding.
//!
//! **Arrays and iteration** (T15): array literals (holes, spread), `for-of`, spread calls, and
//! destructuring everywhere (declarations, assignments, parameters with defaults/patterns/rest,
//! `catch`, `for-in`/`for-of` heads): see [`patterns`]. `Symbol()`, `Symbol.iterator`,
//! `Array.isArray`, `Object.keys/values/entries`.
//!
//! **Classes** (T16): see [`classes`]. Also `new` and `instanceof` with any value, getters and setters
//! in literals (with `super.x` in literal methods), `Object.defineProperty` / `getOwnPropertyDescriptor`
//! / `freeze` / `seal` / `preventExtensions` / `is*`, and the native error constructors as values.
//!
//! **Generators** (T18): `function*` (declarations, expressions, methods), `yield`, `yield*`: see
//! [`generators`]. [`lower`] finishes with `ir::suspend::lower_generators`, so its IR has no
//! `Suspend` left. Exported generator functions are rejected (T20).
//!
//! **Built-ins** (T21): the prelude (JS in `builtins/`, see [`builtins`]) is parsed with the user's
//! code; `Math.x(…)` is a direct call of its `$$Math_x`, `Math.<constant>` a number, `parseInt(…)` a
//! direct call of `$$parseInt`. Prototype methods (`$$String_prototype_slice`) go into
//! `ir::Module::builtin_methods`; the StringToNumber hook is kept when the IR can convert a JS value
//! to a number (another round with it as a root, [`Facts::string_to_number`]).
//!
//! An export whose IR signature differs from its boundary types gets a wrapper `<name>$export`
//! that converts (box an `f64`, ToNumber / ToString of the result).
//!
//! **Commands** (T10): a file without exports is a `wasi:cli/run` command. Its top-level
//! statements (all but function declarations) become a synthetic `main` ([`ir::Module::main`]).
//! Top-level variables stay globals: `main` assigns them where they're declared, and a literal
//! initializer is the global's initial value when no code can run first (a `var` that could be
//! read before its declaration starts as `undefined`). `main` using a `let`/`const` before its
//! declaration throws a ReferenceError (TDZ). A function using one checks an init flag if it could
//! run before the declaration ([`Lowerer::tdz_flags`], F22).
//! `console.log`/`info`/`debug` (stdout) and `console.error`/`warn` (stderr) take primitives:
//! each is shown like node (`-0` as `-0`), joined by spaces, plus a newline. A library (a file
//! with exports) can't use `console` yet (T20); its
//! top-level code (beyond literal globals) is lowered the same way into `ir::Module::init`, which the
//! first export call runs (T20, F11).
//!
//! Anything else produces `unsupported: <construct> (planned: <task>)` at its source
//! position. The lowering never guesses: a construct it can't lower exactly is rejected.
//! All diagnostics of a file are collected, not just the first.

mod builtins;
mod classes;
mod closures;
mod generators;
mod patterns;
mod unicode_props;

use std::collections::{HashMap, HashSet};

use closures::Cap;
use js2wasi3_frontend::{Diagnostic, Diagnostics, Program, line_col, statement_kind};
use js2wasi3_ir::{
    self as ir, BinOp, Builtin, Const, ErrorKind, Expr, FuncId, GlobalId, Intrinsic, LabelId,
    LocalId, Pos, Stmt, Type, TypeofTag, UnOp,
};
use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_semantic::Scoping;
use oxc_span::{GetSpan, Span};
use oxc_syntax::{
    operator::{
        AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator,
    },
    scope::ScopeId,
    symbol::SymbolId,
};
use patterns::{Mode, Pat, Src};

pub use builtins::{PRELUDE, parse, parse_entry};

/// An export's signature in a user world (T20): WIT types, whether it returns a `result`
/// (then a throw is its `err`), and whether it's an `async func`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldSig {
    pub params: Vec<ir::AbiType>,
    pub result: Option<ir::AbiType>,
    pub throws: bool,
    pub is_async: bool,
    /// With `throws`: the `result`'s `err`.
    pub err: ir::ErrType,
    /// The compound types (`AbiType::Compound`).
    pub types: Vec<ir::WitType>,
}

/// A function a user world imports (T20 slice 5), as `import { jsName } from "<specifier>"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldImport {
    /// Its JS name (camelCase) and WIT name (a string import name: `import { "wit-name" as f }`).
    pub js_name: String,
    pub wit_name: String,
    /// Its WIT result is a `result`: `ok` is returned, `err` thrown (jco's `ComponentError`: an
    /// `Error` whose `payload` is the value).
    pub throws: bool,
    pub is_async: bool,
    /// Its core import and types.
    pub import: ir::Import,
    /// Its WIT signature, for messages.
    pub signature: String,
    /// Why it can't be imported (a type that isn't supported), reported if the JS imports it.
    pub unsupported: Option<String>,
}

/// A user world (T20): its exports by JS export name, and its imported functions by module
/// specifier (an interface's name, with or without its version).
#[derive(Debug, Clone, Default)]
pub struct Boundary {
    pub exports: HashMap<String, WorldSig>,
    pub imports: HashMap<String, Vec<WorldImport>>,
}

impl Boundary {
    /// Does a signature take or return a `stream` (T20 slice 6b: the prelude's reader and pump
    /// are needed)?
    fn has_streams(&self) -> bool {
        let mut found = Vec::new();
        self.exports
            .values()
            .map(|w| &w.types)
            .chain(self.imports.values().flatten().map(|i| &i.import.sig.types))
            .flatten()
            .for_each(|t| t.payloads(&mut found));
        found.iter().any(|p| p.stream)
    }
}

/// Lower a whole module. Returns every unsupported construct as a diagnostic.
pub fn lower(program: &Program) -> Result<ir::Module, Diagnostics> {
    lower_with(program, None)
}

/// [`lower`], with the export signatures of a user world (T20).
pub fn lower_with(
    program: &Program,
    boundary: Option<&Boundary>,
) -> Result<ir::Module, Diagnostics> {
    let mut facts = Facts::default();
    let an = closures::analyze(&program.semantic);
    loop {
        let mut extra = Vec::new();
        if facts.string_to_number {
            extra.push(builtins::STRING_TO_NUMBER);
        }
        if boundary.is_some_and(Boundary::has_streams) {
            extra.extend(builtins::STREAMS);
        }
        let prelude = builtins::Prelude::new(program, &extra);
        let mut l = Lowerer {
            p: program,
            boundary,
            prelude: &prelude,
            scoping: program.semantic.scoping(),
            module: ir::Module {
                bind: prelude.bind,
                to_string: prelude.to_string,
                ctor_stubs: prelude.constructor,
                bigint: prelude.bigint,
                suppressed: prelude.suppressed,
                stacks: prelude.stacks,
                names: builtins::GATED
                    .iter()
                    .filter(|n| prelude.user_props.contains(**n))
                    .map(|n| n.to_string())
                    .collect(),
                ..ir::Module::default()
            },
            funcs: HashMap::new(),
            func_params: HashMap::new(),
            globals: HashMap::new(),
            names: HashMap::new(),
            diags: Vec::new(),
            facts: &facts,
            seen: HashMap::new(),
            new_string_params: HashSet::new(),
            exported: HashSet::new(),
            jsdoc: HashMap::new(),
            command: program.exports.is_empty(),
            main: None,
            module_bodies: Vec::new(),
            lexical_end: HashMap::new(),
            tdz_flags: HashMap::new(),
            an: &an,
            nested: HashMap::new(),
            nested_decl: HashMap::new(),
            adapters: HashMap::new(),
            ctor_adapters: HashMap::new(),
            lowered: HashSet::new(),
            hidden: HashMap::new(),
            fn_scope: HashMap::new(),
            fn_source: HashMap::new(),
            rest_param: HashSet::new(),
            fn_length: HashMap::new(),
            classes: HashMap::new(),
            class_adapters: HashMap::new(),
            iface_exports: Vec::new(),
            boundary_fns: Vec::new(),
        };
        l.run();
        if let Some(next) = l.next_facts() {
            // Facts only grow (kinds go up a finite lattice), so this terminates.
            drop(l);
            facts = next;
            continue;
        }
        return if l.diags.is_empty() {
            l.wrap_exports();
            // Generators become resumable frames (T18).
            ir::suspend::lower_generators(&mut l.module);
            Ok(l.module)
        } else {
            l.diags.sort_by_key(|d| (d.line, d.col));
            Err(Diagnostics(l.diags))
        };
    }
}

/// Something whose type is inferred: a variable or parameter (by symbol) or a function result.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Slot {
    Var(SymbolId),
    Result(FuncId),
    /// What an async function's promise settles to: the values its `return`s give (T20).
    Settled(FuncId),
}

/// What a value is known to hold: a lattice `Bottom < Num | Bool | Str < Any`.
/// `Bottom` ("nothing known yet") comes from reading a slot no store has reached so far; in
/// the final round it's left only in dead code. `Num` and `Bottom` values have IR type `f64`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Kind {
    Bottom,
    Num,
    Bool,
    Str,
    Any,
}

impl Kind {
    fn join(self, o: Kind) -> Kind {
        match (self, o) {
            (Kind::Bottom, k) | (k, Kind::Bottom) => k,
            (a, b) if a == b => a,
            _ => Kind::Any,
        }
    }

    fn of_const(c: &Const) -> Kind {
        match c {
            Const::F64(_) | Const::I32(_) => Kind::Num,
            Const::Bool(_) => Kind::Bool,
            Const::Undefined | Const::Null => Kind::Any,
        }
    }
}

/// What earlier rounds learned: the kind stored into each slot (absent: `Bottom`), and the
/// exported parameters used as strings. Every kind rule is monotone and facts only grow, so the
/// rounds reach the least fixpoint.
#[derive(Default)]
struct Facts {
    kinds: HashMap<Slot, Kind>,
    string_params: HashSet<SymbolId>,
    /// The IR may convert a string to a number: keep the prelude's StringToNumber (T21).
    string_to_number: bool,
}

/// Boundary types from an exported function's JSDoc.
#[derive(Default)]
struct JsDoc {
    params: HashMap<String, ir::AbiType>,
    result: Option<ir::AbiType>,
    /// `@returns {Promise<T>}` (T20): `result` is `T` (`None` for `void`/`undefined`).
    promise: bool,
    /// Unsupported types, reported if the function is exported.
    errors: Vec<String>,
}

/// A function of an interface export: a method (declared as a function), or the name of a
/// top-level function (resolved after pass 1).
#[derive(Clone, Copy)]
enum IfaceFn<'a> {
    Func(FuncId),
    Name(&'a str),
}

/// Marker: a diagnostic has been recorded, abandon the current construct.
struct Unsupported;
type R<T> = Result<T, Unsupported>;

#[derive(Clone, Copy)]
enum Binding {
    Func(FuncId),
    Global,
}

struct Lowerer<'p, 'a> {
    p: &'p Program<'a>,
    /// A user world's export signatures (T20).
    boundary: Option<&'p Boundary>,
    /// The built-ins prelude (T21).
    prelude: &'p builtins::Prelude,
    scoping: &'p Scoping,
    module: ir::Module,
    funcs: HashMap<SymbolId, FuncId>,
    /// Parameter symbols of each function, for typing arguments.
    func_params: HashMap<FuncId, Vec<Option<SymbolId>>>,
    globals: HashMap<SymbolId, GlobalId>,
    /// Top-level names, for resolving the export list.
    names: HashMap<String, Binding>,
    diags: Vec<Diagnostic>,
    /// What earlier rounds learned: this round's code assumes it.
    facts: &'p Facts,
    /// The kinds this round stores into each slot (absent: nothing stored).
    seen: HashMap<Slot, Kind>,
    /// Exported parameters this round found used as strings.
    new_string_params: HashSet<SymbolId>,
    /// Functions with an export.
    exported: HashSet<FuncId>,
    jsdoc: HashMap<FuncId, JsDoc>,
    /// No ES exports: a `wasi:cli/run` command whose top-level code runs in `main`.
    command: bool,
    main: Option<FuncId>,
    /// T41 slice 3: each module's top-level code, when a dependency has top-level `await`
    /// (`SourceMap::modules`); `main` runs them through the prelude's `$$EvaluateModules`.
    module_bodies: Vec<FuncId>,
    /// Top-level `let`/`const` of a command: where the declaration ends. `main` reading one
    /// before that is a TDZ error.
    lexical_end: HashMap<SymbolId, u32>,
    /// Top-level `let`/`const` that a function may access before `main` initializes it: a `bool`
    /// global, set once it is (F22). Functions check it on every access.
    tdz_flags: HashMap<SymbolId, GlobalId>,
    /// Closure analysis (T14), the same in every round.
    an: &'p closures::Analysis,
    /// Functions other than top-level declarations, by scope.
    nested: HashMap<ScopeId, Nested<'a>>,
    /// Nested function declarations: their binding's function.
    nested_decl: HashMap<SymbolId, ScopeId>,
    /// The `$code` adapter of each function used as a value.
    adapters: HashMap<FuncId, FuncId>,
    /// Construct entries of functions that use `new.target` (T38), by function.
    ctor_adapters: HashMap<FuncId, FuncId>,
    /// Functions whose body is lowered (nested ones are lowered where they're created).
    lowered: HashSet<FuncId>,
    /// Hidden leading params of each JS function.
    hidden: HashMap<FuncId, Hidden>,
    fn_scope: HashMap<FuncId, ScopeId>,
    /// The source span of each function (T25: `Function.prototype.toString`).
    fn_source: HashMap<FuncId, Span>,
    /// Functions with a rest parameter (their last IR parameter, an array).
    rest_param: HashSet<FuncId>,
    /// Top-level functions' `length`.
    fn_length: HashMap<FuncId, u32>,
    /// Classes (T16), by their scope.
    classes: HashMap<ScopeId, ClassFn<'a>>,
    /// Each class's `$code` functions: (call trap, construct adapter).
    class_adapters: HashMap<ScopeId, (FuncId, FuncId)>,
    /// The functions of interface exports (T20 slice 5): (`iface.fn` key, function, where).
    iface_exports: Vec<(String, IfaceFn<'a>, Span)>,
    /// Every exported function by its boundary name: `export`s and interface functions.
    boundary_fns: Vec<(String, FuncId, Span)>,
}

/// A class (T16): its constructor as an IR function `(self, this?, new_target?, params…) -> jsval`
/// (see [`Hidden`]); a derived class's `this` is a local.
struct ClassFn<'a> {
    id: FuncId,
    class: &'a Class<'a>,
    /// The explicit constructor, if any.
    ctor: Option<&'a Function<'a>>,
}

/// A function other than a top-level declaration (T14).
struct Nested<'a> {
    id: FuncId,
    f: FnAst<'a>,
    /// Its own name (declarations and named function expressions), else empty.
    name: String,
    /// A `function` (not an arrow function or a method): has a `prototype`.
    constructor: bool,
    /// Can be called through a value (an expression, or a declaration used other than as a callee).
    value: bool,
}

#[derive(Clone, Copy)]
enum FnAst<'a> {
    Func(&'a Function<'a>),
    Arrow(&'a ArrowFunctionExpression<'a>),
}

impl<'a> FnAst<'a> {
    fn params(self) -> &'a FormalParameters<'a> {
        match self {
            FnAst::Func(f) => &f.params,
            FnAst::Arrow(a) => &a.params,
        }
    }

    fn statements(self) -> &'a [Statement<'a>] {
        match self {
            FnAst::Func(f) => f.body.as_ref().map_or(&[], |b| &b.statements),
            FnAst::Arrow(a) => match &a.body {
                ArrowFunctionBody::FunctionBody(b) => &b.statements,
                _ => &[],
            },
        }
    }

    /// An arrow function whose body is an expression: that expression.
    fn expression(self) -> Option<&'a Expression<'a>> {
        match self {
            FnAst::Arrow(a) => a.get_expression(),
            _ => None,
        }
    }

    fn span(self) -> Span {
        match self {
            FnAst::Func(f) => f.span,
            FnAst::Arrow(a) => a.span,
        }
    }

    fn is_async(self) -> bool {
        match self {
            FnAst::Func(f) => f.r#async,
            FnAst::Arrow(a) => a.r#async,
        }
    }
    fn is_generator(self) -> bool {
        matches!(self, FnAst::Func(f) if f.generator)
    }
}

/// The parameters a JS function's IR function takes before the JS ones (T14): the function
/// object (`self`: its environment and own name), `this` and (class constructors, T16)
/// `new.target`, each only if used.
#[derive(Clone, Copy, Default)]
struct Hidden {
    self_: bool,
    this: bool,
    new_target: bool,
}

impl Hidden {
    fn count(self) -> u32 {
        self.self_ as u32 + self.this as u32 + self.new_target as u32
    }
}

/// A value being built: an IR expression, its type (`F64`, `Bool` or `JsVal`) and its kind.
struct Val {
    e: Expr,
    ty: Type,
    kind: Kind,
}

impl Val {
    /// A value whose kind is all its type says.
    fn new(e: Expr, ty: Type) -> Val {
        let kind = match ty {
            Type::F64 => Kind::Num,
            Type::Bool => Kind::Bool,
            _ => Kind::Any,
        };
        Val { e, ty, kind }
    }

    fn num(e: Expr) -> Val {
        Val::new(e, Type::F64)
    }

    /// A JS value of a known kind (never `Num`/`Bottom`: those are `f64`).
    fn js(e: Expr, kind: Kind) -> Val {
        debug_assert!(!matches!(kind, Kind::Num | Kind::Bottom));
        Val {
            e,
            ty: Type::JsVal,
            kind,
        }
    }
}

/// What a lowered call is.
#[derive(Clone, Copy)]
enum Call {
    /// A direct call of this function (its result type and kind are known).
    Direct(FuncId),
    /// A call through a value (a `jsval` of any kind).
    Value,
    /// `console.log(…)` and co. (no result).
    Console,
    /// A prelude intrinsic (T21) with a result of this kind (`None`: no result).
    Intrinsic(Option<Kind>),
}

/// A `break`/`continue` target.
struct Target {
    js_label: Option<String>,
    brk: LabelId,
    kind: TargetKind,
}

enum TargetKind {
    /// Labelled non-loop statement: only `break label` may target it.
    Block,
    /// `switch` (T24): `break` may target it, `continue` goes past it.
    Switch,
    /// `while`: `continue` jumps straight to the loop head.
    While,
    /// `for` / `do-while`: `continue` must still run the update / test, so it breaks out of
    /// a block around the body. The block label is created on first use.
    Wrapped(Option<LabelId>),
}

struct FnCx {
    id: FuncId,
    func: ir::Function,
    locals: HashMap<SymbolId, LocalId>,
    next_label: u32,
    targets: Vec<Target>,
    /// `var` declarations without an initializer: set to `undefined` on entry.
    var_undefined: Vec<LocalId>,
    /// Enclosing `finally` blocks, innermost last.
    finally: Vec<Finally>,
    /// Holds the value of a `return` that runs a `finally` first (created on first use).
    ret: Option<LocalId>,
    /// The JS function's scope (`None`: `main`).
    scope: Option<ScopeId>,
    /// Captured variables' cells: the local holding the cell, and whether reads check the TDZ.
    cells: HashMap<SymbolId, (LocalId, bool)>,
    /// Cells created before their declaration (TDZ): not readable here until it's lowered.
    pending: HashSet<SymbolId>,
    /// `this` (a param, or captured by an arrow function); `None`: `undefined`.
    this: Option<LocalId>,
    /// The function object (`self` param).
    self_local: Option<LocalId>,
    /// Captured values (`Cap::Value`) and the function's own name: the local holding them.
    values: HashMap<SymbolId, LocalId>,
    /// The home object for `super.x` (T16).
    home: Option<LocalId>,
    /// The `arguments` object (T25).
    arguments: Option<LocalId>,
    /// `new.target` (a class constructor's param).
    new_target: Option<LocalId>,
    /// A class constructor: `Some(derived)`.
    ctor: Option<bool>,
    /// `this` may be uninitialized (a derived constructor before `super()`): reads check it.
    this_checked: bool,
    /// A class constructor's instance fields, initialized after each `super()` (derived).
    class: Option<ScopeId>,
    /// Private names' symbols (by [`closures::Analysis::privates`] index): their locals.
    privates: HashMap<u32, LocalId>,
    /// Instance private methods' cells: their locals.
    priv_cells: HashMap<u32, LocalId>,
    /// The cells of computed class field keys (by the field's start), filled when the class is
    /// evaluated.
    field_keys: HashMap<u32, LocalId>,
}

impl FnCx {
    fn new(id: FuncId, func: ir::Function) -> FnCx {
        FnCx {
            id,
            func,
            locals: HashMap::new(),
            next_label: 0,
            targets: Vec::new(),
            var_undefined: Vec::new(),
            finally: Vec::new(),
            ret: None,
            scope: None,
            cells: HashMap::new(),
            pending: HashSet::new(),
            this: None,
            self_local: None,
            values: HashMap::new(),
            home: None,
            arguments: None,
            new_target: None,
            ctor: None,
            this_checked: false,
            class: None,
            privates: HashMap::new(),
            priv_cells: HashMap::new(),
            field_keys: HashMap::new(),
        }
    }

    fn label(&mut self) -> LabelId {
        self.next_label += 1;
        LabelId(self.next_label - 1)
    }
}

/// A jump out of statements: `break`/`continue` (to `targets[i]`) or `return`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Jump {
    Break(usize),
    Continue(usize),
    Return,
}

/// A `try … finally` being lowered. Jumps crossing it set `kind` and break to `label`, where the
/// finally body runs and then re-issues them (see [`Lowerer::try_stmt`]).
struct Finally {
    /// `targets.len()` at the `try`: targets below this index are outside it.
    depth: usize,
    label: LabelId,
    /// The completion: 0 normal, 1 throw, `2 + i` for `jumps[i]`.
    kind: LocalId,
    jumps: Vec<Jump>,
}

/// A TDZ check on a variable access (top-level `let`/`const` of a command).
#[derive(Clone, Copy)]
enum Guard {
    /// Always in the TDZ: `main` uses it before its declaration.
    Always,
    /// In the TDZ until `main` sets this `bool` global (F22).
    Flag(GlobalId),
    /// In the TDZ while this (`jsval`) cell is uninitialized (T14).
    Cell(LocalId),
}

#[derive(Clone, Copy)]
enum Place {
    Local(LocalId),
    Global(GlobalId),
    /// A captured variable: the local holding its cell (T14).
    Cell(LocalId),
}

/// A resolved variable: where it lives, its slot, its type and kind.
#[derive(Clone, Copy)]
struct Var {
    place: Place,
    sym: SymbolId,
    ty: Type,
    kind: Kind,
    /// Accesses throw a ReferenceError while the variable is uninitialized.
    guard: Option<(Guard, SymbolId, u32)>,
    /// Writes throw (`const`, an import).
    constant: bool,
}

impl<'p, 'a> Lowerer<'p, 'a> {
    // ---- diagnostics -------------------------------------------------------------------

    fn unsupported(&mut self, span: Span, what: &str, planned: &str) -> Unsupported {
        // In the prelude: a built-in the compiler can't lower yet, a bug in the prelude.
        let (path, line, col) = self.p.locate(span.start);
        self.diags.push(Diagnostic {
            path,
            line,
            col,
            message: format!("unsupported: {what} (planned: {planned})"),
            help: None,
        });
        Unsupported
    }

    fn pos(&self, span: Span) -> Pos {
        let (line, col) = match &self.p.map {
            Some(_) if !self.p.in_prelude(span.start) => {
                let (_, line, col) = self.p.locate(span.start);
                (line, col)
            }
            _ => line_col(self.p.source, span.start),
        };
        Pos { line, col }
    }

    // ---- inferred types ----------------------------------------------------------------

    /// The kind of the values in `s`, as assumed by this round.
    fn slot_kind(&self, s: Slot) -> Kind {
        self.facts.kinds.get(&s).copied().unwrap_or(Kind::Bottom)
    }

    fn slot_ty(&self, s: Slot) -> Type {
        match self.slot_kind(s) {
            Kind::Num | Kind::Bottom => Type::F64,
            _ => Type::JsVal,
        }
    }

    /// Record that a value of kind `k` is stored into `s`.
    fn observe(&mut self, s: Slot, k: Kind) {
        let joined = self.seen.get(&s).map_or(k, |cur| cur.join(k));
        self.seen.insert(s, joined);
    }

    /// The facts for another round, if this round saw anything its assumptions didn't cover.
    /// Then this round's module is thrown away and lowering runs again.
    fn next_facts(&self) -> Option<Facts> {
        let mut string_params = self.facts.string_params.clone();
        let before = string_params.len();
        string_params.extend(self.new_string_params.iter().copied());
        if string_params.len() > before {
            // A parameter the boundary passed as a number is a string now: start the kinds over,
            // or the earlier guess (`Num`) would join with `Str` into `Any`. String parameters
            // only grow, so this happens a bounded number of times.
            return Some(Facts {
                kinds: HashMap::new(),
                string_params,
                string_to_number: self.facts.string_to_number,
            });
        }
        if !self.facts.string_to_number && self.may_convert_string() {
            // More prelude functions: function ids (in result slots) change, so start the kinds
            // over. This happens once.
            return Some(Facts {
                kinds: HashMap::new(),
                string_params,
                string_to_number: true,
            });
        }
        let mut next = Facts {
            kinds: self.facts.kinds.clone(),
            string_params,
            string_to_number: self.facts.string_to_number,
        };
        let mut changed = false;
        for (s, k) in &self.seen {
            let joined = next.kinds.get(s).map_or(*k, |f| f.join(*k));
            if joined != Kind::Bottom && next.kinds.get(s) != Some(&joined) {
                next.kinds.insert(*s, joined);
                changed = true;
            }
        }
        changed.then_some(next)
    }

    /// Can the IR so far call ToNumber on a string (a `JsVal` → `f64` conversion, `==` on JS
    /// values, `JsLess`)? Then the runtime needs the prelude's StringToNumber.
    fn may_convert_string(&self) -> bool {
        let mut found = false;
        for f in &self.module.functions {
            ir::visit_exprs(&f.body, &mut |e| {
                found |= match e {
                    Expr::Convert { from, to, .. } => *from == Type::JsVal && *to == Type::F64,
                    Expr::Binary { op, ty, .. } => {
                        *ty == Type::JsVal && matches!(op, BinOp::LooseEq | BinOp::LooseNe)
                    }
                    Expr::Intrinsic { op, .. } => matches!(op, Intrinsic::JsLess { .. }),
                    _ => false,
                };
            });
        }
        found
    }

    /// `v` converted for storing into `slot` of type `ty`.
    fn store(&mut self, slot: Slot, ty: Type, v: Val) -> Expr {
        self.observe(slot, v.kind);
        match ty {
            Type::JsVal => jsval_of(v),
            _ if v.ty == Type::F64 => v.e,
            _ => Expr::f64(0.0), // placeholder: `observe` forced a new round
        }
    }

    /// Is `test` the prelude's `__intrinsic_bigint_mode()` (T40)?
    fn bigint_mode_test(&self, test: &Expression<'a>) -> bool {
        matches!(test, Expression::CallExpression(c)
            if matches!(&c.callee, Expression::Identifier(id) if id.name == "__intrinsic_bigint_mode")
                && self.p.in_prelude(c.span.start))
    }

    /// Can `v` be a BigInt (T40)? Only in a module that can make one, and only a JS value of
    /// unknown kind.
    fn may_be_bigint(&self, v: &Val) -> bool {
        self.module.bigint && v.kind == Kind::Any
    }

    /// JS ToNumber of `v` (a string goes through the runtime's ToNumber: the prelude's
    /// StringToNumber, T21).
    fn num(&mut self, v: Val, span: Span) -> R<Expr> {
        let _ = span;
        Ok(number_of(v))
    }

    /// `e` is an exported function's parameter used as a string: make it a WIT `string`
    /// (unless its JSDoc says otherwise).
    fn hint_string(&mut self, cx: &FnCx, e: &Expression<'a>) {
        let Expression::Identifier(id) = e.without_parentheses() else {
            return;
        };
        let Some(sym) = self.scoping.get_reference(id.reference_id()).symbol_id() else {
            return;
        };
        let is_param = self.func_params[&cx.id].contains(&Some(sym));
        let documented = self
            .jsdoc
            .get(&cx.id)
            .is_some_and(|d| d.params.contains_key(id.name.as_str()));
        if is_param
            && self.exported.contains(&cx.id)
            && !documented
            && !self.facts.string_params.contains(&sym)
        {
            self.new_string_params.insert(sym);
        }
    }

    // ---- module ------------------------------------------------------------------------

    fn run(&mut self) {
        for (span, what) in self.an.unsupported.clone() {
            if !self.prelude.is_dead(span.start) {
                self.unsupported(span, what, "later");
            }
        }
        for (span, name) in self.prelude.user_refs.clone() {
            self.unsupported(span, &format!("global `{name}`"), "T21");
        }
        for (span, name) in self.prelude.replaced.clone() {
            // Reads of the name stay direct references to the built-in (T42, docs/design.md D6).
            self.unsupported(span, &format!("replacing or deleting the built-in global `{name}`"), "later");
        }
        let body = &self.p.ast.body;
        // Pass 1: declare every top-level function and global so bodies can refer to them in any order.
        let mut fns: Vec<(&'a Function<'a>, FuncId)> = Vec::new();
        // A command's top-level code, in order (everything but function declarations).
        let mut top: Vec<&'a Statement<'a>> = Vec::new();
        // A library needs an `init` only for code beyond literal globals (T20).
        let init = !self.command
            && body.iter().any(|stmt| {
                !self.p.in_prelude(stmt.span().start)
                    && !matches!(
                        stmt,
                        Statement::FunctionDeclaration(_) | Statement::EmptyStatement(_)
                    )
                    && stmt.as_module_declaration().is_none()
                    && !self.static_globals(stmt)
            });
        for stmt in body.iter() {
            if self.p.in_prelude(stmt.span().start) {
                // Only the needed built-ins (functions, and state variables as globals).
                if !self.prelude.is_dead(stmt.span().start) {
                    let _ = self.declare_top(stmt, &mut fns);
                }
                continue;
            }
            if (self.command || init)
                && !matches!(stmt, Statement::FunctionDeclaration(_))
                && stmt.as_module_declaration().is_none()
            {
                if let Statement::VariableDeclaration(d) = stmt {
                    let _ = self.declare_command_globals(d, &top);
                }
                if let Statement::ClassDeclaration(c) = stmt {
                    self.declare_class_global(c);
                }
                top.push(stmt);
                continue;
            }
            let _ = self.declare_top(stmt, &mut fns);
        }
        // A library's top-level code is its `init` (T20, F11), lowered like a command's `main`.
        if self.command || init {
            self.nested_vars();
            self.tdz_flags(&top);
            let name = if self.command { "main" } else { "init" };
            let mut f = ir::Function::new(name, &[], None);
            f.pos = Some(Pos { line: 1, col: 1 });
            self.main = Some(self.module.add_function(f));
            if self.command {
                self.module.main = self.main;
            } else {
                self.module.init = self.main;
            }
            self.func_params.insert(self.main.unwrap(), Vec::new());
            let ranges = self.p.map.as_ref().map(|m| m.modules.clone()).unwrap_or_default();
            for k in 0..ranges.len() {
                let mut f = ir::Function::new(format!("module${k}"), &[], None);
                f.pos = Some(Pos { line: 1, col: 1 });
                let id = self.module.add_function(f);
                self.func_params.insert(id, Vec::new());
                self.hidden.insert(id, Hidden::default());
                self.module_bodies.push(id);
            }
        }
        self.declare_nested();
        self.declare_builtin_methods();
        for e in &self.p.exports {
            if e.from.is_none()
                && let Some(Binding::Func(f)) = e.local.as_deref().and_then(|l| self.names.get(l))
            {
                self.exported.insert(*f);
            }
        }
        // Every exported function by its boundary name (T20 slice 5: interface functions too).
        let mut boundary_fns: Vec<(String, FuncId, Span)> = Vec::new();
        for e in &self.p.exports {
            if e.from.is_none()
                && let Some(Binding::Func(f)) = e.local.as_deref().and_then(|l| self.names.get(l))
            {
                boundary_fns.push((e.name.clone(), *f, e.span));
            }
        }
        for (key, f, span) in std::mem::take(&mut self.iface_exports) {
            let func = match f {
                IfaceFn::Func(id) => id,
                IfaceFn::Name(n) => match self.names.get(n) {
                    Some(Binding::Func(id)) => *id,
                    _ => {
                        let what = format!("`{n}` in an interface export (not a top-level function)");
                        self.unsupported(span, &what, "later");
                        continue;
                    }
                },
            };
            self.exported.insert(func);
            boundary_fns.push((key, func, span));
        }
        self.boundary_fns = boundary_fns.clone();
        for func in self.exported.clone() {
            let f = self.module.function(func);
            let pos = f.pos.expect("declared functions have a position");
            let what = if f.flags.is_generator {
                "exported generator function (the boundary needs an iterator type)"
            } else if self
                .fn_scope
                .get(&func)
                .is_some_and(|s| self.an.uses_arguments.contains(s))
            {
                "`arguments` in an exported function (the WIT signature needs a `list`)"
            } else if self.rest_param.contains(&func) {
                "rest parameter in an exported function"
            } else {
                continue;
            };
            self.diags.push(Diagnostic {
                path: self.p.path.clone(),
                line: pos.line,
                col: pos.col,
                message: format!("unsupported: {what} (planned: T20)"),
                help: None,
            });
        }
        // A user world's signatures (T20): its parameter types, checked against the functions.
        if let Some(boundary) = self.boundary {
            if boundary
                .exports
                .values()
                .map(|w| &w.types)
                .chain(boundary.imports.values().flatten().map(|i| &i.import.sig.types))
                .any(|ts| ts.iter().any(ir::WitType::has_typed))
            {
                // Numeric lists are typed arrays (T20 slice 4b-2).
                self.module.names.insert("$typed".to_string());
            }
            for (name, func, span) in &boundary_fns {
                let (func, span) = (*func, *span);
                let Some(w) = boundary.exports.get(name) else {
                    continue;
                };
                let f = self.module.function(func);
                let lead = self.hidden.get(&func).map_or(0, |h| h.count()) as usize;
                let n = f.params as usize - lead;
                let mut errs = Vec::new();
                if w.is_async != f.flags.is_async {
                    errs.push(if w.is_async {
                        format!("the world's `{}` is an `async func`: export an `async function`", name)
                    } else {
                        format!("the world's `{}` isn't an `async func`: export a function that isn't `async`", name)
                    });
                }
                if n != w.params.len() && !self.rest_param.contains(&func) {
                    errs.push(format!(
                        "`{}` takes {n} parameter{}, the world's function {}",
                        name,
                        if n == 1 { "" } else { "s" },
                        w.params.len()
                    ));
                }
                let Pos { line, col } = self.pos(span);
                for message in errs {
                    self.diags.push(Diagnostic {
                        path: self.p.path.clone(),
                        line,
                        col,
                        message,
                        help: None,
                    });
                }
                for (sym, a) in self.func_params[&func].clone().into_iter().zip(&w.params) {
                    let Some(sym) = sym else { continue };
                    let kind = match a {
                        ir::AbiType::String | ir::AbiType::Char => Kind::Str,
                        ir::AbiType::Bool => Kind::Bool,
                        ir::AbiType::Compound(_) => Kind::Any,
                        _ => Kind::Num,
                    };
                    self.observe(Slot::Var(sym), kind);
                }
            }
        }
        // Boundary facts: string parameters (JSDoc or earlier usage) hold strings.
        for func in self.exported.clone() {
            if self.boundary.is_some() {
                break;
            }
            let params = self.func_params[&func].clone();
            let f = self.module.function(func);
            let pos = f.pos.expect("declared functions have a position");
            let names: Vec<String> = f.locals[..f.params as usize]
                .iter()
                .map(|l| l.name.clone())
                .collect();
            let doc = self.jsdoc.remove(&func).unwrap_or_default();
            for (name, sym) in names.iter().zip(params) {
                let Some(sym) = sym else { continue };
                let string = match doc.params.get(name) {
                    Some(t) => *t == ir::AbiType::String,
                    None => self.facts.string_params.contains(&sym),
                };
                // What the boundary passes in.
                let kind = if string { Kind::Str } else { Kind::Num };
                self.observe(Slot::Var(sym), kind);
            }
            let mut errors = doc.errors.clone();
            if doc.promise && !self.module.function(func).flags.is_async {
                errors
                    .push("JSDoc return type `Promise<…>` on a function that isn't `async`".into());
            }
            for e in &errors {
                self.diags.push(Diagnostic {
                    path: self.p.path.clone(),
                    line: pos.line,
                    col: pos.col,
                    message: format!("unsupported: {e} (planned: T20)"),
                    help: None,
                });
            }
            self.jsdoc.insert(func, doc);
        }
        // Pass 2: lower bodies.
        for (f, id) in fns {
            let _ = self.lower_function(f, id);
        }
        if let Some(main) = self.main {
            self.lower_main(main, &top);
            let async_top = std::iter::once(main)
                .chain(self.module_bodies.iter().copied())
                .any(|f| self.module.function(f).flags.is_async);
            if !self.command && async_top {
                self.library_await(&top);
            }
        }
        // Pass 3: exports.
        for e in &self.p.exports {
            let _ = self.export(e);
        }
        // Interface functions (T20 slice 5): `iface.fn`, renamed by the CLI.
        for (name, func, span) in self.boundary_fns.clone() {
            if name.contains('.') {
                let pos = Some(self.pos(span));
                self.module.exports.push(ir::Export {
                    name,
                    func,
                    string_name: false,
                    pos,
                    sig: ir::AbiSig::default(),
                });
            }
        }
    }

    /// The prelude's prototype methods in use (`$$String_prototype_slice`) and StringToNumber (T21).
    fn declare_builtin_methods(&mut self) {
        let mut names: Vec<(&String, &SymbolId)> = self.prelude.names.iter().collect();
        names.sort();
        for (name, sym) in names {
            if !self.prelude.is_live(name) {
                continue;
            }
            let Some(&func) = self.funcs.get(sym) else {
                continue;
            };
            if builtins::STREAMS.contains(&name.as_str()) {
                // Called by the backend's stream lifting / lowering (T20 slice 6b): JS values.
                if name == builtins::STREAMS[0] {
                    self.module.stream_reader = Some(func);
                } else {
                    self.module.stream_pump = Some(func);
                }
                for p in self.func_params[&func].clone().into_iter().flatten() {
                    self.observe(Slot::Var(p), Kind::Any);
                }
                continue;
            }
            if name == builtins::DATE_CALL {
                // `Date` as a value: its call entry (T26).
                for p in self.func_params[&func].clone().into_iter().flatten() {
                    self.observe(Slot::Var(p), Kind::Any);
                }
                self.module.date_call = Some(self.adapter(func));
                continue;
            }
            if name == builtins::DATE_INSPECT {
                // Called by `console.log` with a Date (T26).
                for p in self.func_params[&func].clone().into_iter().flatten() {
                    self.observe(Slot::Var(p), Kind::Any);
                }
                self.module.date_inspect = Some(func);
                continue;
            }
            if name == builtins::STRING_TO_BIGINT {
                // Called by the runtime with a string (T40).
                self.module.string_to_bigint = Some(func);
                if let Some(Some(p)) = self.func_params[&func].first() {
                    self.observe(Slot::Var(*p), Kind::Str);
                }
                continue;
            }
            if name == builtins::STRING_TO_NUMBER {
                // Called by the runtime with a string.
                self.module.string_to_number = Some(func);
                if let Some(Some(p)) = self.func_params[&func].first() {
                    self.observe(Slot::Var(*p), Kind::Str);
                }
                continue;
            }
            let Some((b, key)) = name[2..].split_once("_prototype_") else {
                // A constructor used as a value (`$$Array`) and its statics (`$$Array_from`, T25).
                let (b, key) = name[2..].split_once('_').unwrap_or((&name[2..], ""));
                // `$$Array_isArray$value` is the static `isArray` (T28).
                let key = key.strip_suffix(builtins::VALUE).unwrap_or(key);
                if !self.prelude.ctor_values.contains(b) {
                    continue;
                }
                let length = builtins::length(name).unwrap_or(self.fn_length[&func]);
                for p in self.func_params[&func].clone().into_iter().flatten() {
                    self.observe(Slot::Var(p), Kind::Any);
                }
                let code = self.adapter(func);
                if let Some(ns) = ir::Namespace::parse(b) {
                    let m = ir::BuiltinMethod {
                        proto: Builtin::Object,
                        key: key.to_string(),
                        code,
                        length,
                    };
                    self.module.namespace_methods.push((ns, m));
                    continue;
                }
                let owner = Builtin::parse(b).expect("a built-in constructor");
                let m = ir::BuiltinMethod {
                    proto: owner,
                    key: key.to_string(),
                    code,
                    length,
                };
                if key.is_empty() {
                    self.module.builtin_ctors.push(m);
                } else {
                    self.module.builtin_statics.push(m);
                }
                continue;
            };
            let proto = Builtin::parse_prelude(b).expect("a built-in prototype");
            let length = builtins::length(name).unwrap_or(self.fn_length[&func]);
            // Called through the prototype: any arguments.
            for p in self.func_params[&func].clone().into_iter().flatten() {
                self.observe(Slot::Var(p), Kind::Any);
            }
            let code = self.adapter(func);
            if proto == Builtin::Array && builtins::ARRAY_NATIVE.contains(&key) {
                // The fallback of a native method for a `this` that isn't an array (T35).
                self.module.array_generic.push((key.to_string(), code));
                continue;
            }
            self.module.builtin_methods.push(ir::BuiltinMethod {
                proto,
                key: key.to_string(),
                code,
                length,
            });
        }
    }

    fn declare_top(
        &mut self,
        stmt: &'a Statement<'a>,
        fns: &mut Vec<(&'a Function<'a>, FuncId)>,
    ) -> R<()> {
        match stmt {
            Statement::FunctionDeclaration(f) => self.declare_function(f, f.span.start, fns),
            Statement::VariableDeclaration(d) => self.declare_globals(d),
            Statement::ExportDeclaration(e) => match &e.declaration {
                Declaration::FunctionDeclaration(f) => self.declare_function(f, e.span.start, fns),
                Declaration::VariableDeclaration(d)
                    if let Some((name, obj)) = self.interface_export(d) =>
                {
                    self.declare_interface(name, obj, fns)
                }
                Declaration::VariableDeclaration(d) => {
                    self.declare_globals(d)?;
                    Err(self.unsupported(
                        e.span,
                        "exporting a variable (only functions can be exported)",
                        "T20",
                    ))
                }
                _ => Err(self.unsupported(
                    e.span,
                    "exported declaration other than a function",
                    "T20",
                )),
            },
            Statement::ExportDefaultDeclaration(e) => match &e.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(f) if f.id.is_some() => {
                    self.declare_function(f, e.span.start, fns)
                }
                _ => Err(self.unsupported(
                    e.span,
                    "`export default` of anything but a named function",
                    "T14",
                )),
            },
            // `export { a, b as c }`: resolved from the export list in pass 3.
            Statement::ExportNamedDeclaration(_) => Ok(()),
            Statement::ImportDeclaration(i) => self.declare_import(i),
            Statement::ExportFromDeclaration(e) => Err(self.unsupported(
                e.span,
                "re-export from another module (multi-file modules, F7)",
                "not scheduled",
            )),
            Statement::ExportAllDeclaration(e) => Err(self.unsupported(
                e.span,
                "`export *` from another module (multi-file modules, F7)",
                "not scheduled",
            )),
            Statement::EmptyStatement(_) => Ok(()),
            other => Err(self.unsupported(
                other.span(),
                &format!(
                    "top-level {} in a module with exports (library init code, F11; a file without exports is a command and may have top-level code)",
                    statement_kind(other)
                ),
                "T20",
            )),
        }
    }

    /// `doc_at`: where a JSDoc comment for the function would be attached (the start of the
    /// `export` statement or of the declaration).
    /// `(result) -> jsval` for an import whose WIT result is a `result` (T20 slice 5): the `ok`
    /// value, or throw the `err` value like jco's `ComponentError` (an Error whose `payload` is
    /// the value, its message the value if a string, else `String(value) + " (see error.payload)"`).
    fn unwrap_function(&mut self, wit_name: &str) -> FuncId {
        let mut func = ir::Function::new(
            format!("{wit_name}$unwrap"),
            &[("result", Type::JsVal)],
            Some(Type::JsVal),
        );
        let r = LocalId(0);
        let v = func.add_local("val", Type::JsVal);
        let e = func.add_local("error", Type::JsVal);
        let get = |k: &str| Expr::intrinsic(Intrinsic::GetProp, vec![Expr::local(r), Expr::str(k)]);
        let message = Expr::Cond {
            cond: Box::new(Expr::TypeofIs {
                arg: Box::new(Expr::local(v)),
                tag: TypeofTag::String,
            }),
            then: Box::new(Expr::local(v)),
            els: Box::new(Expr::intrinsic(
                Intrinsic::Concat,
                vec![
                    Expr::intrinsic(Intrinsic::ToString, vec![Expr::local(v)]),
                    Expr::str(" (see error.payload)"),
                ],
            )),
            ty: Type::JsVal,
        };
        func.body = vec![
            Stmt::Set(v, get("val")),
            Stmt::If {
                cond: Expr::binary(BinOp::Eq, Type::JsVal, get("tag"), Expr::str("err")),
                then: vec![
                    Stmt::Set(
                        e,
                        Expr::intrinsic(
                            Intrinsic::NewError(ErrorKind::Error),
                            vec![message, Expr::undefined()],
                        ),
                    ),
                    Stmt::Expr(Expr::intrinsic(
                        Intrinsic::SetProp,
                        vec![Expr::local(e), Expr::str("payload"), Expr::local(v)],
                    )),
                    Stmt::Throw(Expr::local(e)),
                ],
                els: vec![],
            },
            Stmt::Return(Some(Expr::local(v))),
        ];
        self.module.add_function(func)
    }

    /// `export const host = { … }` where the user world exports an interface whose JS name is
    /// `host` (T20 slice 5): its name and object literal.
    fn interface_export(
        &self,
        d: &'a VariableDeclaration<'a>,
    ) -> Option<(&'a str, &'a ObjectExpression<'a>)> {
        let [decl] = d.declarations.as_slice() else {
            return None;
        };
        let (BindingPattern::BindingIdentifier(b), Some(Expression::ObjectExpression(o))) =
            (&decl.id, &decl.init)
        else {
            return None;
        };
        let prefix = format!("{}.", b.name);
        let boundary = self.boundary?;
        boundary
            .exports
            .keys()
            .any(|k| k.starts_with(&prefix))
            .then_some((b.name.as_str(), &**o))
    }

    /// An interface export's functions (jco's shape): `{ f(x) {…}, g: function (x) {…}, h,
    /// i: topLevelFunction }`, keys the camelCase function names. The object isn't a value: its
    /// functions are exported as `iface.fn`.
    fn declare_interface(
        &mut self,
        name: &'a str,
        obj: &'a ObjectExpression<'a>,
        fns: &mut Vec<(&'a Function<'a>, FuncId)>,
    ) -> R<()> {
        let prefix = format!("{name}.");
        let wanted: Vec<String> = self
            .boundary
            .map(|b| {
                b.exports
                    .keys()
                    .filter_map(|k| k.strip_prefix(&prefix).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let mut result = Ok(());
        let mut seen = Vec::new();
        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                result = Err(self.unsupported(prop.span(), "spread in an interface export", "later"));
                continue;
            };
            let key = match (&p.key, p.computed, p.kind) {
                (PropertyKey::StaticIdentifier(k), false, PropertyKind::Init) => k.name.as_str(),
                (PropertyKey::StringLiteral(k), false, PropertyKind::Init) => k.value.as_str(),
                _ => {
                    let what = "a computed key or accessor in an interface export";
                    result = Err(self.unsupported(p.span, what, "later"));
                    continue;
                }
            };
            if !wanted.iter().any(|w| w == key) {
                self.diags.push(Diagnostic {
                    path: self.p.path.clone(),
                    line: self.pos(p.span).line,
                    col: self.pos(p.span).col,
                    message: format!("the world's interface `{name}` has no function `{key}`"),
                    help: Some(format!("its functions: {}", wanted.join(", "))),
                });
                result = Err(Unsupported);
                continue;
            }
            seen.push(key.to_string());
            let f = match &p.value {
                Expression::Identifier(r) => IfaceFn::Name(r.name.as_str()),
                Expression::FunctionExpression(f) => {
                    let scope = f.scope_id.get().expect("functions have scopes");
                    let what = if self.an.uses_this.contains(&scope) {
                        Some("`this` in an interface export's function")
                    } else if self.an.uses_arguments.contains(&scope) {
                        Some("`arguments` in an interface export's function")
                    } else {
                        None
                    };
                    if let Some(what) = what {
                        result = Err(self.unsupported(p.span, what, "later"));
                        continue;
                    }
                    let full = format!("{name}.{key}");
                    match self.declare_function_as(f, &full, None, p.span.start, fns) {
                        Ok(id) => IfaceFn::Func(id),
                        Err(e) => {
                            result = Err(e);
                            continue;
                        }
                    }
                }
                _ => {
                    let what = "an interface export's function that isn't a method, a function \
                                expression or the name of a top-level function";
                    result = Err(self.unsupported(p.span, what, "later"));
                    continue;
                }
            };
            self.iface_exports.push((format!("{name}.{key}"), f, p.span));
        }
        for w in &wanted {
            if !seen.contains(w) {
                self.diags.push(Diagnostic {
                    path: self.p.path.clone(),
                    line: self.pos(obj.span).line,
                    col: self.pos(obj.span).col,
                    message: format!("interface export `{name}` has no function `{w}`"),
                    help: None,
                });
                result = Err(Unsupported);
            }
        }
        result
    }

    /// `import { f, g as h } from "ns:pkg/iface"` (T20 slice 5): each imported function of a user
    /// world is a function `(jsval…) -> jsval` calling it ([`Intrinsic::CallImport`]).
    fn declare_import(&mut self, i: &'a ImportDeclaration<'a>) -> R<()> {
        let from = i.source.value.as_str();
        let Some(funcs) = self.boundary.and_then(|b| b.imports.get(from)) else {
            let what = if self.boundary.is_some() {
                format!("import from `{from}`: the world doesn't import an interface of that name")
            } else if from.contains(':') {
                format!("import from `{from}` without a world (`--wit`) that imports it")
            } else {
                "import of another module (multi-file modules, F7)".into()
            };
            return Err(self.unsupported(i.span, &what, "not scheduled"));
        };
        if i.with_clause.is_some() || i.phase.is_some() {
            return Err(self.unsupported(i.span, "import attributes or phases", "not scheduled"));
        }
        let mut result = Ok(());
        for s in i.specifiers.iter().flatten() {
            let ImportDeclarationSpecifier::ImportSpecifier(s) = s else {
                let what = format!("default or namespace import from `{from}` (import each function by name)");
                result = Err(self.unsupported(s.span(), &what, "later"));
                continue;
            };
            let name = s.imported.name();
            let found = funcs.iter().find(|w| match &s.imported {
                ModuleExportName::StringLiteral(_) => w.wit_name == name.as_str(),
                _ => w.js_name == name.as_str(),
            });
            let Some(w) = found else {
                let known: Vec<&str> = funcs.iter().map(|w| w.js_name.as_str()).collect();
                self.diags.push(Diagnostic {
                    path: self.p.locate(s.span.start).0,
                    line: self.pos(s.span).line,
                    col: self.pos(s.span).col,
                    message: format!("`{from}` has no function `{name}`"),
                    help: Some(format!("its functions: {}", known.join(", "))),
                });
                result = Err(Unsupported);
                continue;
            };
            if let Some(why) = &w.unsupported {
                self.diags.push(Diagnostic {
                    path: self.p.locate(s.span.start).0,
                    line: self.pos(s.span).line,
                    col: self.pos(s.span).col,
                    message: format!("import of `{name}` from `{from}`: {why}"),
                    help: None,
                });
                result = Err(Unsupported);
                continue;
            }
            if w.is_async && !self.boundary.is_some_and(|b| b.exports.values().any(|e| e.is_async)) {
                let what = format!(
                    "importing the `async func` `{}` in a world without an `async func` export (nothing would wait for it)",
                    w.wit_name
                );
                result = Err(self.unsupported(s.span, &what, "later"));
                continue;
            }
            let index = match self
                .module
                .imports
                .iter()
                .position(|x| x.module == w.import.module && x.name == w.import.name)
            {
                Some(k) => k,
                None => {
                    let unwrap = w.throws.then(|| self.unwrap_function(&w.wit_name));
                    self.module.imports.push(ir::Import {
                        unwrap,
                        ..w.import.clone()
                    });
                    self.module.imports.len() - 1
                }
            };
            let unwrap = self.module.imports[index].unwrap;
            let n = w.import.sig.params.len();
            let names: Vec<String> = (0..n).map(|k| format!("p{k}")).collect();
            let sig: Vec<(&str, Type)> = names.iter().map(|n| (n.as_str(), Type::JsVal)).collect();
            let local = s.local.name.as_str();
            let mut func = ir::Function::new(local, &sig, Some(Type::JsVal));
            func.pos = Some(self.pos(s.span));
            let args = Expr::Array((0..n as u32).map(|k| Some(Expr::local(LocalId(k)))).collect());
            let call = Expr::intrinsic(Intrinsic::CallImport(index as u32), vec![args]);
            // An `async func` gives a promise, settled through `unwrap` (the backend calls it).
            func.body = vec![Stmt::Return(Some(match unwrap {
                Some(u) if !w.is_async => Expr::Call {
                    func: u,
                    args: vec![call],
                },
                _ => call,
            }))];
            let id = self.module.add_function(func);
            // Its body isn't lowered from JS: what it returns is any JS value.
            self.observe(Slot::Result(id), Kind::Any);
            self.hidden.insert(id, Hidden::default());
            self.fn_length.insert(id, n as u32);
            self.func_params.insert(id, vec![None; n]);
            if let Some(sym) = s.local.symbol_id.get() {
                self.funcs.insert(sym, id);
            }
            self.names.insert(local.to_string(), Binding::Func(id));
        }
        result
    }

    fn declare_function(
        &mut self,
        f: &'a Function<'a>,
        doc_at: u32,
        fns: &mut Vec<(&'a Function<'a>, FuncId)>,
    ) -> R<()> {
        let id_node = f.id.as_ref().expect("function declarations have a name");
        self.declare_function_as(f, id_node.name.as_str(), id_node.symbol_id.get(), doc_at, fns)
            .map(|_| ())
    }

    /// Declare `f` as a top-level function named `name`, bound to `sym` if it has one (T20
    /// slice 5: an interface export's methods have none).
    fn declare_function_as(
        &mut self,
        f: &'a Function<'a>,
        name: &str,
        sym: Option<SymbolId>,
        doc_at: u32,
        fns: &mut Vec<(&'a Function<'a>, FuncId)>,
    ) -> R<FuncId> {
        // Simple parameters are typed; the others (defaults, patterns: `None`) are `jsval`s bound
        // in the body's prologue, and a rest parameter is one more `jsval` (an array).
        let mut params = Vec::new();
        let bad = false;
        for p in &f.params.items {
            match (&p.pattern, &p.initializer) {
                (BindingPattern::BindingIdentifier(b), None) => {
                    params.push((b.name.as_str(), b.symbol_id.get()))
                }
                _ => params.push(("param", None)),
            }
        }
        let body = f.body.as_ref().expect("function declarations have a body");
        let id = FuncId(self.module.functions.len() as u32);
        let result = Slot::Result(id);
        let uses_arguments = f
            .scope_id
            .get()
            .is_some_and(|s| self.an.uses_arguments.contains(&s));
        if uses_arguments {
            // `function f(...arguments)`: the parameters are bound in the prologue (T25).
            if f.params.rest.is_some() {
                return Err(self.unsupported(f.span, "a rest parameter with `arguments`", "later"));
            }
            params.clear();
        }
        // A function has a result if it returns a value, or if its (undefined) result is used.
        let returns_value = f.generator
            || f.r#async
            || body.statements.iter().any(has_value_return)
            || self.slot_ty(result) == Type::JsVal;
        let scope = f.scope_id.get().expect("functions have scopes");
        let hidden = Hidden {
            self_: false,
            this: self.an.uses_this.contains(&scope),
            // T38: a constructor's `new.target` (`undefined` when called).
            new_target: self.an.uses_new_target.contains(&scope) && !f.generator && !f.r#async,
        };
        let mut sig: Vec<(&str, Type)> = Vec::new();
        if hidden.this {
            sig.push(("this", Type::JsVal));
        }
        if hidden.new_target {
            sig.push(("new.target", Type::JsVal));
        }
        for (n, sym) in &params {
            let ty = sym.map_or(Type::JsVal, |s| self.slot_ty(Slot::Var(s)));
            sig.push((n, ty));
        }
        if f.params.rest.is_some() || uses_arguments {
            sig.push(("rest", Type::JsVal));
            self.rest_param.insert(id);
        }
        self.fn_length.insert(id, js_length(&f.params));
        let mut func = ir::Function::new(
            name,
            &sig,
            returns_value.then(|| self.slot_ty(result)),
        );
        if f.generator {
            self.generator_sig(&mut func, result);
        }
        if f.r#async {
            self.async_sig(&mut func, result);
        }
        func.pos = Some(self.pos(f.span));
        self.module.add_function(func);
        self.hidden.insert(id, hidden);
        self.fn_scope.insert(id, scope);
        self.fn_source.insert(id, f.span);
        if let Some(text) = self.p.jsdoc(doc_at) {
            self.jsdoc.insert(id, parse_jsdoc(text));
        }
        self.func_params
            .insert(id, params.iter().map(|(_, s)| *s).collect());
        if let Some(sym) = sym {
            self.funcs.insert(sym, id);
            self.names.insert(name.to_string(), Binding::Func(id));
        }
        if bad {
            return Err(Unsupported);
        }
        fns.push((f, id));
        Ok(id)
    }

    /// A command's top-level variables are globals, assigned by `main` where they're declared.
    /// A literal initializer becomes the global's initial value when no code can run before the
    /// declaration. A `var` read before its declaration is `undefined`, so it starts as
    /// `undefined` when any code comes before it or its initializer isn't a literal.
    fn declare_command_globals(
        &mut self,
        d: &'a VariableDeclaration<'a>,
        before: &[&'a Statement<'a>],
    ) -> R<()> {
        if matches!(
            d.kind,
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing
        ) {
            return Err(self.unsupported(d.span, "`using` declaration", "not scheduled"));
        }
        let is_var = d.kind == VariableDeclarationKind::Var;
        let result = Ok(());
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(b) = &decl.id else {
                // A pattern (T15): each name is a global, set by `main`.
                for b in decl.id.get_binding_identifiers() {
                    let sym = b
                        .symbol_id
                        .get()
                        .expect("semantic analysis assigns symbols");
                    let slot = Slot::Var(sym);
                    if is_var {
                        self.observe(slot, Kind::Any);
                    }
                    let ty = self.slot_ty(slot);
                    self.module.globals.push(ir::Global {
                        name: b.name.to_string(),
                        ty,
                        mutable: true,
                        init: if ty == Type::F64 {
                            Const::F64(0.0)
                        } else {
                            Const::Undefined
                        },
                    });
                    let id = GlobalId(self.module.globals.len() as u32 - 1);
                    self.globals.insert(sym, id);
                    self.names.insert(b.name.to_string(), Binding::Global);
                    if !is_var {
                        self.lexical_end.insert(sym, decl.span.end);
                    }
                }
                continue;
            };
            let sym = b
                .symbol_id
                .get()
                .expect("semantic analysis assigns symbols");
            let slot = Slot::Var(sym);
            let literal = decl.init.as_ref().and_then(|e| self.literal(e));
            // (A non-literal initializer may call a function that reads the variable.)
            let undefined_first = is_var && (!before.is_empty() || literal.is_none());
            let init = match literal {
                _ if decl.init.is_none() || undefined_first => Some(Const::Undefined),
                Some(c) => Some(c),
                // Non-literal: `main` sets it before any (non-TDZ) read.
                None => None,
            };
            if let Some(c) = &init {
                self.observe(slot, Kind::of_const(c));
            }
            let ty = self.slot_ty(slot);
            // A placeholder of the slot's type when there's no initial value.
            let init = init.unwrap_or(if ty == Type::F64 {
                Const::F64(0.0)
            } else {
                Const::Undefined
            });
            self.module.globals.push(ir::Global {
                name: b.name.to_string(),
                ty,
                mutable: true,
                init,
            });
            let id = GlobalId(self.module.globals.len() as u32 - 1);
            self.globals.insert(sym, id);
            self.names.insert(b.name.to_string(), Binding::Global);
            if !is_var {
                self.lexical_end.insert(sym, decl.span.end);
            }
        }
        result
    }

    /// A command's top-level class declaration (T16): a global like a `let`, set where the class is
    /// defined (before its static fields run). Reading it earlier, including in the heritage, is a
    /// TDZ error.
    fn declare_class_global(&mut self, c: &'a Class<'a>) {
        let Some(b) = &c.id else { return };
        let sym = b
            .symbol_id
            .get()
            .expect("semantic analysis assigns symbols");
        let slot = Slot::Var(sym);
        self.observe(slot, Kind::Any);
        self.module.globals.push(ir::Global {
            name: b.name.to_string(),
            ty: self.slot_ty(slot),
            mutable: true,
            init: Const::Undefined,
        });
        let id = GlobalId(self.module.globals.len() as u32 - 1);
        self.globals.insert(sym, id);
        self.names.insert(b.name.to_string(), Binding::Global);
        self.lexical_end.insert(sym, c.body.span.start);
    }

    /// F28: a `var` in a nested block of a command's top-level code (`{ var z }`, `for (var i …)`)
    /// belongs to the module scope too: a global, `undefined` until assigned.
    fn nested_vars(&mut self) {
        let root = self.scoping.root_scope_id();
        let mut syms: Vec<SymbolId> = self.scoping.get_bindings(root).values().copied().collect();
        syms.sort();
        for sym in syms {
            if self.globals.contains_key(&sym)
                || self.funcs.contains_key(&sym)
                || !self
                    .scoping
                    .symbol_flags(sym)
                    .is_function_scoped_declaration()
            {
                continue;
            }
            let slot = Slot::Var(sym);
            self.observe(slot, Kind::Any);
            let name = self.scoping.symbol_name(sym).to_string();
            self.module.globals.push(ir::Global {
                name: name.clone(),
                ty: self.slot_ty(slot),
                mutable: true,
                init: Const::Undefined,
            });
            let id = GlobalId(self.module.globals.len() as u32 - 1);
            self.globals.insert(sym, id);
            self.names.insert(name, Binding::Global);
        }
    }

    /// F22: a function can run before a top-level `let`/`const` is initialized only if `main`
    /// refers to a top-level function or makes a closure before the declaration ends. Those
    /// variables get an init flag that functions check.
    fn tdz_flags(&mut self, top: &[&'a Statement<'a>]) {
        let nodes = self.p.semantic.nodes();
        let in_top = |at: &u32| {
            top.iter()
                .any(|s| s.span().start <= *at && *at < s.span().end)
        };
        let referenced = self
            .funcs
            .keys()
            .flat_map(|f| self.scoping.get_resolved_references(*f))
            .map(|r| nodes.get_node(r.node_id()).span().start)
            .filter(in_top);
        // (T14) A closure made in top-level code can run too: from where it's made (a nested
        // declaration: from the start of its top-level statement, hoisting).
        let made = nodes
            .iter()
            .filter_map(|n| match n.kind() {
                AstKind::Function(f) if f.is_declaration() => top
                    .iter()
                    .find(|s| s.span().start <= f.span.start && f.span.start < s.span().end)
                    .map(|s| s.span().start),
                AstKind::Function(f) => Some(f.span.start),
                AstKind::ArrowFunctionExpression(a) => Some(a.span.start),
                AstKind::Class(c) => Some(c.span.start),
                _ => None,
            })
            .filter(in_top);
        let first_call = referenced.chain(made).min();
        let Some(first_call) = first_call else {
            return;
        };
        let mut lexical: Vec<(SymbolId, u32)> =
            self.lexical_end.iter().map(|(s, e)| (*s, *e)).collect();
        lexical.sort_by_key(|(sym, end)| (*end, *sym)); // ties: one pattern (deterministic, T42)
        for (sym, end) in lexical {
            if first_call < end {
                self.module.globals.push(ir::Global {
                    name: format!("{}$init", self.scoping.symbol_name(sym)),
                    ty: Type::Bool,
                    mutable: true,
                    init: Const::Bool(false),
                });
                let g = GlobalId(self.module.globals.len() as u32 - 1);
                self.tdz_flags.insert(sym, g);
            }
        }
    }

    /// Reject a library's top-level `await` (its `init` runs synchronously before the first export
    /// call; async module evaluation needs async exports, T20).
    fn library_await(&mut self, top: &[&'a Statement<'a>]) {
        let nodes = self.p.semantic.nodes();
        let mut spans = Vec::new();
        for node in nodes.iter() {
            let span = match node.kind() {
                AstKind::AwaitExpression(a) => a.span,
                AstKind::ForOfStatement(f) if f.r#await => f.span,
                _ => continue,
            };
            let in_top = top
                .iter()
                .any(|s| s.span().start <= span.start && span.start < s.span().end);
            let in_function = nodes.ancestors(node.id()).any(|a| {
                matches!(
                    a.kind(),
                    AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
                )
            });
            if in_top && !in_function {
                spans.push(span);
            }
        }
        for span in spans {
            self.unsupported(span, "top-level `await` in a module with exports", "T20");
        }
    }

    /// Is `id` top-level code (`main`, or a module's body): it runs top to bottom.
    pub(crate) fn is_top_code(&self, id: FuncId) -> bool {
        Some(id) == self.main || self.module_bodies.contains(&id)
    }

    /// `main`: a command's top-level code. Top-level variables are globals set in place.
    /// With [`Self::module_bodies`], each module's statements go to its own function (lowered
    /// first: `main` makes their function objects).
    fn lower_main(&mut self, id: FuncId, top: &[&'a Statement<'a>]) {
        if !self.module_bodies.is_empty() {
            let ranges = self.p.map.as_ref().expect("joined program").modules.clone();
            for (k, (start, end)) in ranges.into_iter().enumerate() {
                let stmts: Vec<_> = top
                    .iter()
                    .copied()
                    .filter(|s| start <= s.span().start && s.span().start < end)
                    .collect();
                self.lower_top(self.module_bodies[k], &stmts);
            }
            let rest: Vec<_> = top
                .iter()
                .copied()
                .filter(|s| !self.p.map.as_ref().unwrap().modules.iter().any(|(a, b)| *a <= s.span().start && s.span().start < *b))
                .collect();
            return self.lower_top(id, &rest);
        }
        self.lower_top(id, top);
    }

    /// Top-level statements into function `id` (`main`, or a module's body).
    fn lower_top(&mut self, id: FuncId, top: &[&'a Statement<'a>]) {
        let func = self.module.functions[id.0 as usize].clone();
        let mut cx = FnCx::new(id, func);
        let mut stmts = Vec::new();
        for s in top {
            let _ = match s {
                Statement::VariableDeclaration(d) => self.global_inits(&mut cx, d, &mut stmts),
                _ => self.stmt(&mut cx, s, &mut stmts),
            };
        }
        let mut body: Vec<Stmt> = cx
            .var_undefined
            .iter()
            .map(|l| Stmt::Set(*l, Expr::undefined()))
            .collect();
        body.extend(stmts);
        cx.func.body = body;
        self.module.functions[id.0 as usize] = cx.func;
    }

    /// The assignments of a top-level declaration in `main` (see [`Self::declare_command_globals`]).
    fn global_inits(
        &mut self,
        cx: &mut FnCx,
        d: &'a VariableDeclaration<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        if matches!(
            d.kind,
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing
        ) {
            // Reported by `declare_command_globals`; it declared no globals (F140).
            return Err(Unsupported);
        }
        let mut result = Ok(());
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(b) = &decl.id else {
                let Some(init) = &decl.init else { continue };
                let src = Src::Text(self.callee_text(init));
                let fresh = d.kind != VariableDeclarationKind::Var;
                let r = self.expr(cx, init).and_then(|v| {
                    self.pattern(
                        cx,
                        Pat::Bind(&decl.id),
                        jsval_of(v),
                        Mode::Decl { fresh },
                        Some(src),
                        out,
                    )
                });
                if let Err(e) = r {
                    result = Err(e);
                }
                for b in decl.id.get_binding_identifiers() {
                    if let Some(sym) = b.symbol_id.get()
                        && let Some(flag) = self.tdz_flags.get(&sym)
                    {
                        out.push(Stmt::SetGlobal(*flag, Expr::bool(true)));
                    }
                }
                continue;
            };
            let sym = b
                .symbol_id
                .get()
                .expect("semantic analysis assigns symbols");
            let g = self.globals[&sym];
            let global = &self.module.globals[g.0 as usize];
            let (ty, init) = (global.ty, global.init);
            match &decl.init {
                // `undefined` from the start.
                None => {}
                // The literal is the initial value.
                Some(e) if init != Const::Undefined && self.literal(e).is_some() => {}
                Some(e) => match self.named(cx, e, Some(b.name.as_str())) {
                    Ok(v) => {
                        let e = self.store(Slot::Var(sym), ty, v);
                        out.push(Stmt::SetGlobal(g, e));
                    }
                    Err(e) => result = Err(e),
                },
            }
            if let Some(flag) = self.tdz_flags.get(&sym) {
                out.push(Stmt::SetGlobal(*flag, Expr::bool(true)));
            }
        }
        result
    }

    /// A top-level declaration of plain names with literal (or no) initializers: globals with an
    /// initial value, no code.
    fn static_globals(&self, stmt: &'a Statement<'a>) -> bool {
        let Statement::VariableDeclaration(d) = stmt else {
            return false;
        };
        !matches!(
            d.kind,
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing
        ) && d.declarations.iter().all(|decl| {
            matches!(decl.id, BindingPattern::BindingIdentifier(_))
                && decl.init.as_ref().is_none_or(|e| self.literal(e).is_some())
        })
    }

    fn declare_globals(&mut self, d: &'a VariableDeclaration<'a>) -> R<()> {
        let mut result = Ok(());
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(b) = &decl.id else {
                result = Err(self.unsupported(decl.span, "destructuring declaration", "T15"));
                continue;
            };
            let init = match &decl.init {
                None => Some(Const::Undefined),
                Some(e) => self.literal(e),
            };
            let Some(init) = init else {
                result = Err(self.unsupported(
                    decl.span,
                    "top-level variable whose initializer isn't a literal in a module with exports (library init code, F11)",
                    "T20",
                ));
                continue;
            };
            let sym = b
                .symbol_id
                .get()
                .expect("semantic analysis assigns symbols");
            let slot = Slot::Var(sym);
            self.observe(slot, Kind::of_const(&init));
            self.module.globals.push(ir::Global {
                name: b.name.to_string(),
                ty: self.slot_ty(slot),
                mutable: d.kind != VariableDeclarationKind::Const,
                init,
            });
            let id = GlobalId(self.module.globals.len() as u32 - 1);
            self.globals.insert(sym, id);
            self.names.insert(b.name.to_string(), Binding::Global);
        }
        result
    }

    fn export(&mut self, e: &js2wasi3_frontend::Export) -> R<()> {
        if e.from.is_some() {
            return Ok(()); // already reported as a re-export in pass 1
        }
        match e.local.as_deref().and_then(|l| self.names.get(l)).copied() {
            Some(Binding::Func(func)) => {
                let pos = Some(self.pos(e.span));
                self.module.exports.push(ir::Export {
                    name: e.name.clone(),
                    func,
                    string_name: e.string_name,
                    pos,
                    sig: ir::AbiSig::default(), // set by `wrap_exports`
                });
                Ok(())
            }
            // Exported globals and non-function defaults are reported in pass 1.
            Some(Binding::Global) => Ok(()),
            None => Ok(()),
        }
    }

    /// Set each export's boundary signature: a user world's (T20), else from JSDoc and usage.
    /// Where the function's IR signature differs, the export goes through a wrapper
    /// `<name>$export` that converts.
    fn wrap_exports(&mut self) {
        use ir::AbiType;
        let mut done: Vec<(FuncId, ir::AbiSig, FuncId)> = Vec::new();
        let throws = ir::may_throw(&self.module);
        // Jobs run after each call; an unhandled rejection, or an `init` that threw, fails it (T20).
        let jobs = ir::can_queue(&self.module);
        let init_throws = self.module.init.is_some_and(|f| throws[f.0 as usize]);
        for i in 0..self.module.exports.len() {
            let func = self.module.exports[i].func;
            let f = self.module.function(func).clone();
            // A `this` param (T14) is `undefined` for calls from the boundary.
            let lead = self.hidden.get(&func).map_or(0, |h| h.count()) as usize;
            let params = &f.locals[lead..f.params as usize];
            let syms = &self.func_params[&func];
            let doc = self.jsdoc.get(&func);
            let is_async = f.flags.is_async;
            let result_kind = self.slot_kind(if is_async {
                Slot::Settled(func)
            } else {
                Slot::Result(func)
            });
            let world = self
                .boundary
                .and_then(|b| b.exports.get(&self.module.exports[i].name));
            let sig = if let Some(w) = world {
                // The world decides; without a `result` a throw traps (T20).
                ir::AbiSig {
                    params: w.params.clone(),
                    result: w.result,
                    throws: w.throws,
                    jobs,
                    is_async,
                    err: w.err,
                    types: w.types.clone(),
                }
            } else {
                let result = if !is_async {
                    f.result.map(|_| match doc.and_then(|d| d.result) {
                        Some(t) => t,
                        None if result_kind == Kind::Str => AbiType::String,
                        None => AbiType::F64,
                    })
                } else {
                    // What the promise settles to: `@returns {Promise<T>}`, else the `return`s'.
                    match doc {
                        Some(d) if d.promise || d.result.is_some() => d.result,
                        _ => match result_kind {
                            Kind::Bottom => None,
                            Kind::Str => Some(AbiType::String),
                            _ => Some(AbiType::F64),
                        },
                    }
                };
                ir::AbiSig {
                    params: params
                        .iter()
                        .zip(syms)
                        .map(|(l, sym)| match doc.and_then(|d| d.params.get(&l.name)) {
                            Some(t) => *t,
                            None if sym.is_some_and(|s| self.facts.string_params.contains(&s)) => {
                                AbiType::String
                            }
                            None => AbiType::F64,
                        })
                        .collect(),
                    result,
                    // An async function's promise can reject.
                    throws: throws[func.0 as usize] || jobs || init_throws || is_async,
                    jobs,
                    is_async,
                    err: ir::ErrType::JsError,
                    types: Vec::new(),
                }
            };
            if let Some((_, _, w)) = done.iter().find(|(g, s, _)| *g == func && *s == sig) {
                self.module.exports[i].func = *w;
                self.module.exports[i].sig = sig;
                continue;
            }
            let direct = lead == 0
                && params.len() == sig.params.len()
                && params
                    .iter()
                    .zip(&sig.params)
                    .all(|(l, a)| l.ty == a.param_type())
                && (is_async
                    || f.result == sig.result.map(|a| a.result_type())
                        && (sig.result != Some(AbiType::String) || result_kind == Kind::Str));
            let target = if direct {
                func
            } else {
                // Parameters the world has and the function doesn't are dropped (T20: `check_world`
                // reports a count that differs).
                let wsig: Vec<(String, Type)> = sig
                    .params
                    .iter()
                    .enumerate()
                    .map(|(j, a)| {
                        let name = params.get(j).map_or(format!("p{j}"), |l| l.name.clone());
                        (name, a.param_type())
                    })
                    .collect();
                let wsig: Vec<(&str, Type)> = wsig.iter().map(|(n, t)| (n.as_str(), *t)).collect();
                // An async function's wrapper returns its promise (the backend converts the value).
                let wresult = if is_async {
                    f.result
                } else {
                    sig.result.map(|a| a.result_type())
                };
                let mut w = ir::Function::new(format!("{}$export", f.name), &wsig, wresult);
                w.pos = f.pos;
                let mut args = vec![Expr::undefined(); lead];
                args.extend(
                    params
                        .iter()
                        .enumerate()
                        .map(|(j, l)| match sig.params.get(j) {
                            Some(a) => {
                                convert_any(a.param_type(), l.ty, Expr::local(LocalId(j as u32)))
                            }
                            None => convert_any(Type::JsVal, l.ty, Expr::undefined()),
                        }),
                );
                let call = Expr::Call { func, args };
                w.body = match (sig.result, f.result) {
                    _ if is_async => vec![Stmt::Return(Some(call))],
                    (None, _) => vec![Stmt::Expr(call)],
                    (Some(a), None) => vec![
                        Stmt::Expr(call),
                        Stmt::Return(Some(to_boundary(
                            Expr::undefined(),
                            Type::JsVal,
                            Kind::Any,
                            a,
                        ))),
                    ],
                    (Some(a), Some(t)) => {
                        vec![Stmt::Return(Some(to_boundary(call, t, result_kind, a)))]
                    }
                };
                self.module.add_function(w)
            };
            self.module.exports[i].func = target;
            self.module.exports[i].sig = sig.clone();
            done.push((func, sig, target));
        }
    }

    // ---- functions ---------------------------------------------------------------------

    fn lower_function(&mut self, f: &'a Function<'a>, id: FuncId) -> R<()> {
        let scope = f.scope_id.get().expect("functions have scopes");
        self.lower_body(id, FnAst::Func(f), scope)
    }

    /// Declare every function that isn't a top-level declaration (T14), in source order so that
    /// ids are the same in every round. Bodies are lowered where the function objects are made.
    fn declare_nested(&mut self) {
        let nodes = self.p.semantic.nodes();
        let root = self.scoping.root_scope_id();
        for node in nodes.iter() {
            if self.prelude.is_dead(node.span().start) {
                continue;
            }
            let f = match node.kind() {
                AstKind::Function(f) => FnAst::Func(f),
                AstKind::ArrowFunctionExpression(a) => FnAst::Arrow(a),
                AstKind::Class(c) => {
                    self.declare_class(c);
                    continue;
                }
                _ => continue,
            };
            let scope = match f {
                FnAst::Func(f) => f.scope_id.get(),
                FnAst::Arrow(a) => a.scope_id.get(),
            }
            .expect("functions have scopes");
            if let FnAst::Func(func) = f
                && func.is_declaration()
                && self.scoping.scope_parent_id(scope) == Some(root)
            {
                continue; // top-level: `declare_function`
            }
            if self.an.ctor_class.contains_key(&scope) {
                continue; // a class constructor: `declare_class`
            }
            // Methods and accessors aren't constructors.
            let method = match nodes.parent_kind(node.id()) {
                AstKind::MethodDefinition(_) => true,
                AstKind::ObjectProperty(p) => p.method || p.kind != PropertyKind::Init,
                _ => false,
            };
            let arrow = matches!(f, FnAst::Arrow(_));
            let hidden = Hidden {
                self_: self.an.captures.get(&scope).is_some_and(|c| !c.is_empty())
                    || self.an.self_ref.contains(&scope),
                this: !arrow && self.an.uses_this.contains(&scope),
                // T38: only constructors take it; elsewhere `new.target` is `undefined`.
                new_target: !arrow
                    && !method
                    && !f.is_generator()
                    && !f.is_async()
                    && self.an.uses_new_target.contains(&scope),
            };
            let id = FuncId(self.module.functions.len() as u32);
            let mut sig: Vec<(String, Type)> = Vec::new();
            if hidden.self_ {
                sig.push(("self".into(), Type::JsVal));
            }
            if hidden.this {
                sig.push(("this".into(), Type::JsVal));
            }
            if hidden.new_target {
                sig.push(("new.target".into(), Type::JsVal));
            }
            let mut params = Vec::new();
            let uses_arguments = self.an.uses_arguments.contains(&scope);
            if uses_arguments && f.params().rest.is_some() {
                let _ = self.unsupported(f.span(), "a rest parameter with `arguments`", "later");
                continue;
            }
            for p in &f.params().items {
                if uses_arguments {
                    break; // bound in the prologue from the rest parameter (T25)
                }
                let (name, sym) = match (&p.pattern, &p.initializer) {
                    (BindingPattern::BindingIdentifier(b), None) => {
                        (b.name.to_string(), b.symbol_id.get())
                    }
                    _ => ("param".to_string(), None),
                };
                let ty = sym.map_or(Type::JsVal, |s| self.slot_ty(Slot::Var(s)));
                sig.push((name, ty));
                params.push(sym);
            }
            if f.params().rest.is_some() || uses_arguments {
                sig.push(("rest".to_string(), Type::JsVal));
                self.rest_param.insert(id);
            }
            let result = Slot::Result(id);
            let generator = matches!(f, FnAst::Func(func) if func.generator);
            let is_async = f.is_async();
            let returns_value = generator
                || is_async
                || f.expression().is_some()
                || f.statements().iter().any(has_value_return)
                || self.slot_ty(result) == Type::JsVal;
            let name = match f {
                FnAst::Func(func) => func.id.as_ref().map(|i| i.name.to_string()),
                FnAst::Arrow(_) => None,
            };
            let sig_refs: Vec<(&str, Type)> = sig.iter().map(|(n, t)| (n.as_str(), *t)).collect();
            let mut func = ir::Function::new(
                name.clone().unwrap_or_else(|| format!("anonymous{}", id.0)),
                &sig_refs,
                returns_value.then(|| self.slot_ty(result)),
            );
            if generator {
                self.generator_sig(&mut func, result);
            }
            if is_async {
                self.async_sig(&mut func, result);
            }
            func.pos = Some(self.pos(f.span()));
            self.module.add_function(func);
            self.hidden.insert(id, hidden);
            self.fn_scope.insert(id, scope);
            // `toString` text: a method's from its key (without `static`), an accessor's from
            // `get`/`set`.
            let source = match nodes.parent_kind(node.id()) {
                AstKind::MethodDefinition(m) => {
                    let mut s = m.span;
                    if m.r#static {
                        let text = &self.p.source[s.start as usize..s.end as usize];
                        let rest = text["static".len()..].trim_start();
                        s.start += (text.len() - rest.len()) as u32;
                    }
                    s
                }
                AstKind::ObjectProperty(p) if method => p.span,
                _ => f.span(),
            };
            self.fn_source.insert(id, source);
            self.func_params.insert(id, params);
            if let FnAst::Func(func) = f
                && func.is_declaration()
                && let Some(sym) = func.id.as_ref().and_then(|i| i.symbol_id.get())
            {
                self.nested_decl.insert(sym, scope);
            }
            self.nested.insert(
                scope,
                Nested {
                    id,
                    f,
                    name: name.unwrap_or_default(),
                    constructor: !arrow && !method && !generator,
                    value: match f {
                        FnAst::Func(func) if func.is_declaration() => func
                            .id
                            .as_ref()
                            .and_then(|i| i.symbol_id.get())
                            .is_some_and(|s| self.an.fn_values.contains(&s)),
                        _ => true,
                    },
                },
            );
        }
    }

    /// Declare a class's constructor function (T16): `(self, this, new_target?, params…) -> jsval`
    /// for a base class, `(self, new_target, params…) -> jsval` for a derived one. An implicit
    /// derived constructor takes a rest parameter (the arguments it passes to `super`).
    fn declare_class(&mut self, c: &'a Class<'a>) {
        let scope = c.scope_id.get().expect("classes have scopes");
        let ctor = c.body.body.iter().find_map(|e| match e {
            ClassElement::MethodDefinition(m) if m.kind == MethodDefinitionKind::Constructor => {
                Some(&*m.value)
            }
            _ => None,
        });
        let derived = c.heritage.is_some();
        let hidden = Hidden {
            self_: true,
            this: !derived,
            new_target: derived || self.an.uses_new_target.contains(&scope),
        };
        let id = FuncId(self.module.functions.len() as u32);
        let mut sig: Vec<(String, Type)> = vec![("self".into(), Type::JsVal)];
        if hidden.this {
            sig.push(("this".into(), Type::JsVal));
        }
        if hidden.new_target {
            sig.push(("new.target".into(), Type::JsVal));
        }
        let mut params = Vec::new();
        match ctor {
            Some(f) => {
                for p in &f.params.items {
                    let (name, sym) = match (&p.pattern, &p.initializer) {
                        (BindingPattern::BindingIdentifier(b), None) => {
                            (b.name.to_string(), b.symbol_id.get())
                        }
                        _ => ("param".to_string(), None),
                    };
                    let ty = sym.map_or(Type::JsVal, |s| self.slot_ty(Slot::Var(s)));
                    sig.push((name, ty));
                    params.push(sym);
                }
                if f.params.rest.is_some() {
                    sig.push(("rest".to_string(), Type::JsVal));
                    self.rest_param.insert(id);
                }
            }
            None if derived => {
                sig.push(("args".to_string(), Type::JsVal));
                self.rest_param.insert(id);
            }
            None => {}
        }
        let name = c.id.as_ref().map(|i| i.name.to_string());
        let sig_refs: Vec<(&str, Type)> = sig.iter().map(|(n, t)| (n.as_str(), *t)).collect();
        let mut func = ir::Function::new(
            name.unwrap_or_else(|| format!("anonymous{}", id.0)),
            &sig_refs,
            Some(Type::JsVal),
        );
        func.pos = Some(self.pos(c.span));
        self.module.add_function(func);
        self.hidden.insert(id, hidden);
        self.fn_scope.insert(id, scope);
        self.func_params.insert(id, params);
        self.classes.insert(scope, ClassFn { id, class: c, ctor });
    }

    /// Lower a nested function's body (once per round).
    fn lower_nested(&mut self, scope: ScopeId) -> R<()> {
        let n = &self.nested[&scope];
        let (id, f) = (n.id, n.f);
        if self.lowered.contains(&id) {
            return Ok(());
        }
        self.lowered.insert(id);
        self.lower_body(id, f, scope)
    }

    /// Lower a function body: first the environment (captured cells and values), cells for captured
    /// parameters and `var`s, then the statements.
    fn lower_body(&mut self, id: FuncId, f: FnAst<'a>, scope: ScopeId) -> R<()> {
        self.lowered.insert(id);
        let func = self.module.functions[id.0 as usize].clone();
        let mut cx = FnCx::new(id, func);
        cx.scope = Some(scope);
        let mut prologue = Vec::new();
        let before = self.diags.len();
        self.prologue(
            &mut cx,
            scope,
            scope,
            Some(f.params()),
            f.span(),
            &mut prologue,
        )?;
        if matches!(f, FnAst::Arrow(_)) {
            // `this` captured from a derived constructor may be uninitialized.
            let mut owner = self.an.parent.get(&scope).copied().flatten();
            while let Some(o) = owner
                && self.an.is_arrow(o)
            {
                owner = self.an.parent.get(&o).copied().flatten();
            }
            cx.this_checked = owner
                .and_then(|o| self.an.classes.get(&o))
                .is_some_and(|c| c.derived);
        }
        let mut stmts = Vec::new();
        if let Some(e) = f.expression() {
            // `(…) => e`
            if let Ok(v) = self.expr(&mut cx, e) {
                let ty = cx.func.result.expect("expression arrows have a result");
                let e = self.store(Slot::Result(id), ty, v);
                stmts.push(Stmt::Return(Some(e)));
            }
        } else {
            let body = f.statements();
            if cx.func.flags.is_generator {
                // The prologue runs when the generator is made, the body on the first `next()`.
                let y = self.suspend(&mut cx, ir::SuspendKind::Yield, Expr::undefined());
                stmts.push(Stmt::Expr(y));
            }
            stmts.extend(self.stmts(&mut cx, body));
            if let Some(ty) = cx.func.result
                && !always_returns(body)
            {
                // Falling off the end returns `undefined`.
                let v = self.undefined_val();
                let e = self.store(Slot::Result(id), ty, v);
                stmts.push(Stmt::Return(Some(e)));
            }
        }
        if self.diags.len() > before {
            return Err(Unsupported);
        }
        self.finish(cx, prologue, stmts);
        Ok(())
    }

    /// Store a lowered function: prologue, `var`s set to `undefined`, statements.
    fn finish(&mut self, mut cx: FnCx, prologue: Vec<Stmt>, stmts: Vec<Stmt>) {
        let mut body = prologue;
        body.extend(
            cx.var_undefined
                .iter()
                .map(|l| Stmt::Set(*l, Expr::undefined())),
        );
        body.extend(stmts);
        cx.func.body = body;
        let id = cx.id;
        self.module.functions[id.0 as usize] = cx.func;
    }

    /// Cells for the captured `var`s of scope `vars` (not those `skip` says): one for the whole
    /// call (or `static {}` block, T31), `undefined` until assigned.
    pub(crate) fn var_cells(
        &mut self,
        cx: &mut FnCx,
        vars: ScopeId,
        skip: impl Fn(SymbolId) -> bool,
        prologue: &mut Vec<Stmt>,
    ) {
        let mut var_syms: Vec<SymbolId> =
            self.scoping.get_bindings(vars).values().copied().collect();
        var_syms.sort();
        for sym in var_syms {
            if self
                .scoping
                .symbol_flags(sym)
                .is_function_scoped_declaration()
                && self.an.captured.contains(&sym)
                && !skip(sym)
                && !self.nested_decl.contains_key(&sym)
            {
                let slot = Slot::Var(sym);
                let ty = self.slot_ty(slot);
                let v = self.undefined_val();
                let v = self.store(slot, ty, v);
                let cell = cx.func.add_local(
                    format!("{}.cell", self.scoping.symbol_name(sym)),
                    Type::JsVal,
                );
                prologue.push(Stmt::Set(
                    cell,
                    Expr::intrinsic(Intrinsic::NewCell(ty), vec![v]),
                ));
                cx.cells.insert(sym, (cell, false));
            }
        }
    }

    /// The start of a function (`scope`: the function, or a class): the hidden params, the
    /// environment (captured cells and values), the parameters (cells for captured ones, then
    /// defaults and patterns, then the rest parameter), cells for the captured `var`s of `vars`
    /// (the scope declaring them).
    fn prologue(
        &mut self,
        cx: &mut FnCx,
        scope: ScopeId,
        vars: ScopeId,
        params: Option<&'a FormalParameters<'a>>,
        span: Span,
        prologue: &mut Vec<Stmt>,
    ) -> R<()> {
        let hidden = self.hidden[&cx.id];
        let mut k = 0;
        if hidden.self_ {
            cx.self_local = Some(LocalId(0));
            k += 1;
        }
        if hidden.this {
            cx.this = Some(LocalId(k));
            k += 1;
        }
        if hidden.new_target {
            cx.new_target = Some(LocalId(k));
            k += 1;
        }
        if let Some(self_local) = cx.self_local {
            for (sym, s) in &self.an.self_names {
                if *s == scope {
                    cx.values.insert(*sym, self_local);
                }
            }
        }
        let captures = self.an.captures.get(&scope).cloned().unwrap_or_default();
        for (i, cap) in captures.iter().enumerate() {
            let name = match cap {
                Cap::This => "this".to_string(),
                Cap::Home => "home".to_string(),
                Cap::NewTarget => "new.target".to_string(),
                Cap::Arguments => "arguments".to_string(),
                Cap::Private(i) => self.an.privates[*i as usize].name.clone(),
                Cap::PrivMethod(i) => format!("{}.cell", self.an.privates[*i as usize].name),
                Cap::FieldKey(_) => "key.cell".to_string(),
                Cap::Cell(sym) | Cap::Value(sym) => self.scoping.symbol_name(*sym).to_string(),
            };
            let Some(self_local) = cx.self_local else {
                return Err(self.unsupported(
                    span,
                    "internal: a closure without its function object",
                    "T14",
                ));
            };
            let l = cx.func.add_local(name, Type::JsVal);
            prologue.push(Stmt::Set(
                l,
                Expr::intrinsic(Intrinsic::EnvGet(i as u32), vec![Expr::Local(self_local)]),
            ));
            match cap {
                Cap::This => cx.this = Some(l),
                Cap::Home => cx.home = Some(l),
                Cap::NewTarget => cx.new_target = Some(l),
                Cap::Arguments => cx.arguments = Some(l),
                Cap::Private(i) => {
                    cx.privates.insert(*i, l);
                }
                Cap::PrivMethod(i) => {
                    cx.priv_cells.insert(*i, l);
                }
                Cap::FieldKey(at) => {
                    cx.field_keys.insert(*at, l);
                }
                Cap::Cell(sym) => {
                    cx.cells.insert(*sym, (l, self.an.tdz.contains(sym)));
                }
                Cap::Value(sym) => {
                    cx.values.insert(*sym, l);
                }
            }
        }
        let mut param_syms = Vec::new();
        let mut complex = Vec::new();
        if self.an.uses_arguments.contains(&scope) {
            // `function f(...args)` (T25): each parameter from `args[i]`, then `arguments`.
            let all = cx.func.param(k);
            for (i, p) in params.map_or(&[][..], |ps| &ps.items).iter().enumerate() {
                // Bound here, before `var_cells`: it must not give them fresh cells (T43).
                param_syms.extend(p.pattern.get_binding_identifiers().iter().filter_map(|b| b.symbol_id.get()));
                let v = Expr::intrinsic(
                    Intrinsic::GetProp,
                    vec![Expr::Local(all), jsval_of(Val::num(Expr::f64(i as f64)))],
                );
                let _ = match &p.initializer {
                    Some(init) => self.param_default(cx, &p.pattern, init, v, prologue),
                    None => self.pattern(
                        cx,
                        Pat::Bind(&p.pattern),
                        v,
                        Mode::Decl { fresh: false },
                        Some(Src::Value),
                        prologue,
                    ),
                };
            }
            let l = cx.func.add_local("arguments", Type::JsVal);
            prologue.push(Stmt::Set(
                l,
                Expr::intrinsic(Intrinsic::MakeArguments, vec![Expr::Local(all)]),
            ));
            cx.arguments = Some(l);
        }
        let items = if self.an.uses_arguments.contains(&scope) {
            &[][..]
        } else {
            params.map_or(&[][..], |ps| &ps.items)
        };
        for (i, p) in items.iter().enumerate() {
            if p.initializer.is_some() || !matches!(p.pattern, BindingPattern::BindingIdentifier(_))
            {
                complex.push((cx.func.param(k + i as u32), p));
                continue;
            }
            if let BindingPattern::BindingIdentifier(b) = &p.pattern
                && let Some(sym) = b.symbol_id.get()
            {
                let param = cx.func.param(k + i as u32);
                param_syms.push(sym);
                if self.an.captured.contains(&sym) {
                    let ty = cx.func.local(param).ty;
                    let cell = cx.func.add_local(format!("{}.cell", b.name), Type::JsVal);
                    prologue.push(Stmt::Set(
                        cell,
                        Expr::intrinsic(Intrinsic::NewCell(ty), vec![Expr::Local(param)]),
                    ));
                    cx.cells.insert(sym, (cell, false));
                } else {
                    cx.locals.insert(sym, param);
                }
            }
        }
        self.var_cells(cx, vars, |sym| param_syms.contains(&sym), prologue);
        // Parameters with defaults or patterns, then the rest parameter.
        for (param, p) in complex {
            let mode = Mode::Decl { fresh: false };
            let v = Expr::Local(param);
            let _ = match &p.initializer {
                Some(init) => self.param_default(cx, &p.pattern, init, v, prologue),
                None => self.pattern(
                    cx,
                    Pat::Bind(&p.pattern),
                    v,
                    mode,
                    Some(Src::Value),
                    prologue,
                ),
            };
        }
        if let Some(rest) = params.and_then(|ps| ps.rest.as_ref())
            && cx.arguments.is_none()
        {
            let param = cx.func.param(cx.func.params - 1);
            let _ = self.pattern(
                cx,
                Pat::Bind(&rest.rest.argument),
                Expr::Local(param),
                Mode::Decl { fresh: false },
                None,
                prologue,
            );
        }
        Ok(())
    }

    /// A new function object for nested function `scope` (its body is lowered first).
    /// `name`: the name it gets from where it's defined (`const f = () => …`).
    fn closure(
        &mut self,
        cx: &mut FnCx,
        scope: ScopeId,
        name: Option<&str>,
        span: Span,
    ) -> R<Expr> {
        let n = &self.nested[&scope];
        let (id, f, own, constructor, value) = (n.id, n.f, n.name.clone(), n.constructor, n.value);
        let body = self.lower_nested(scope);
        // Called through a value: its parameters can be anything. (A declaration only called
        // directly needs its object just for its environment.)
        if value {
            for p in self.func_params[&id].clone().into_iter().flatten() {
                self.observe(Slot::Var(p), Kind::Any);
            }
        }
        body?;
        let code = self.adapter(id);
        let env = self.env_for(cx, scope, span)?;
        let name = if own.is_empty() {
            name.unwrap_or_default().to_string()
        } else {
            own
        };
        let name = self.p.original_name(&name).to_string();
        Ok(Expr::Closure(Box::new(ir::Closure {
            code,
            name,
            length: js_length(f.params()),
            kind: if self.module.function(id).flags.is_generator
                && self.module.function(id).flags.is_async
            {
                ir::FnKind::AsyncGenerator
            } else if self.module.function(id).flags.is_generator {
                ir::FnKind::Generator
            } else if self.module.function(id).flags.is_async {
                ir::FnKind::Async
            } else if constructor && self.hidden[&id].new_target {
                ir::FnKind::OrdinaryWith {
                    construct: self.ctor_adapter(id),
                }
            } else if constructor {
                ir::FnKind::Ordinary
            } else {
                ir::FnKind::Plain
            },
            env,
            shared: false,
            source: self.source_text(self.fn_source[&id]),
        })))
    }

    /// The source text of `span` for `Function.prototype.toString` (T25): only when the code names
    /// `toString`, and not for the prelude's functions.
    pub(crate) fn source_text(&self, span: Span) -> Option<String> {
        (self.module.to_string && !self.p.in_prelude(span.start))
            .then(|| self.p.source[span.start as usize..span.end as usize].to_string())
    }

    /// The environment of a new closure of function (or class) `scope`: what it captures, from `cx`.
    fn env_for(&mut self, cx: &FnCx, scope: ScopeId, span: Span) -> R<Vec<Expr>> {
        let mut env = Vec::new();
        for cap in self.an.captures.get(&scope).cloned().unwrap_or_default() {
            env.push(match cap {
                Cap::This => cx.this.map_or_else(Expr::undefined, Expr::Local),
                Cap::Home => cx.home.map_or_else(Expr::undefined, Expr::Local),
                Cap::NewTarget => cx.new_target.map_or_else(Expr::undefined, Expr::Local),
                Cap::Arguments => cx.arguments.map_or_else(Expr::undefined, Expr::Local),
                Cap::FieldKey(at) => match cx.field_keys.get(&at) {
                    Some(l) => Expr::Local(*l),
                    None => {
                        return Err(self.unsupported(
                            span,
                            "internal: computed field key not evaluated",
                            "T31",
                        ));
                    }
                },
                Cap::Private(i) | Cap::PrivMethod(i) => {
                    let found = match cap {
                        Cap::Private(_) => cx.privates.get(&i),
                        _ => cx.priv_cells.get(&i),
                    };
                    match found {
                        Some(l) => Expr::Local(*l),
                        None => {
                            return Err(self.unsupported(
                                span,
                                &format!(
                                    "internal: private name `{}` not in scope",
                                    self.an.privates[i as usize].name
                                ),
                                "T16",
                            ));
                        }
                    }
                }
                Cap::Cell(sym) | Cap::Value(sym) => {
                    let found = match cap {
                        Cap::Cell(_) => cx.cells.get(&sym).map(|(l, _)| *l),
                        _ => cx.values.get(&sym).copied(),
                    };
                    match found {
                        Some(l) => Expr::Local(l),
                        None => {
                            return Err(self.unsupported(
                                span,
                                &format!(
                                    "closure capturing `{}` before its declaration",
                                    self.scoping.symbol_name(sym)
                                ),
                                "later",
                            ));
                        }
                    }
                }
            });
        }
        Ok(env)
    }

    /// The object of a top-level function (one per module).
    fn func_value(&mut self, func: FuncId) -> Expr {
        for p in self.func_params[&func].clone().into_iter().flatten() {
            self.observe(Slot::Var(p), Kind::Any);
        }
        let code = self.adapter(func);
        let f = self.module.function(func);
        // A built-in (`$$Math_floor`) is named after its property (`floor`).
        let name = match f.name.strip_prefix("$$") {
            Some(n) => {
                let n = n.strip_suffix(builtins::VALUE).unwrap_or(n);
                n.rsplit('_').next().unwrap_or(n).to_string()
            }
            None => self.p.original_name(&f.name).to_string(),
        };
        let length = builtins::length(&f.name).unwrap_or(self.fn_length[&func]);
        Expr::Closure(Box::new(ir::Closure {
            code,
            name,
            length,
            // Built-in functions other than constructors (made by `BuiltinCtor`) aren't
            // constructors (T36: `Reflect.construct`).
            kind: if f.name.starts_with("$$") {
                ir::FnKind::Plain
            } else if f.flags.is_generator && f.flags.is_async {
                ir::FnKind::AsyncGenerator
            } else if f.flags.is_generator {
                ir::FnKind::Generator
            } else if f.flags.is_async {
                ir::FnKind::Async
            } else if self.hidden.get(&func).is_some_and(|h| h.new_target) {
                ir::FnKind::OrdinaryWith {
                    construct: self.ctor_adapter(func),
                }
            } else {
                ir::FnKind::Ordinary
            },
            env: Vec::new(),
            shared: true,
            source: self.fn_source.get(&func).and_then(|s| self.source_text(*s)),
        }))
    }

    /// What `new` runs for a function that uses `new.target` (T38): `f$construct(self, new_target,
    /// args) -> jsval` makes `this` from `new_target.prototype`, calls the typed function `func` with
    /// both, and gives its result if that is an object, else `this`.
    fn ctor_adapter(&mut self, func: FuncId) -> FuncId {
        if let Some(a) = self.ctor_adapters.get(&func) {
            return *a;
        }
        let f = self.module.function(func).clone();
        let hidden = self.hidden[&func];
        let mut a = ir::Function::new(
            format!("{}$construct", f.name),
            &[
                ("self", Type::JsVal),
                ("new.target", Type::JsVal),
                ("args", Type::JsVal),
            ],
            Some(Type::JsVal),
        );
        a.pos = f.pos;
        let this = a.add_local("this", Type::JsVal);
        let mut args = Vec::new();
        if hidden.self_ {
            args.push(Expr::Local(LocalId(0)));
        }
        if hidden.this {
            args.push(Expr::Local(this));
        }
        args.push(Expr::Local(LocalId(1)));
        let rest = self.rest_param.contains(&func);
        for i in hidden.count()..f.params {
            let op = if rest && i == f.params - 1 {
                Intrinsic::ArgsRest(i - hidden.count())
            } else {
                Intrinsic::ArgGet(i - hidden.count())
            };
            let arg = Expr::intrinsic(op, vec![Expr::Local(LocalId(2))]);
            args.push(match f.locals[i as usize].ty {
                Type::JsVal => arg,
                ty => Expr::convert(Type::JsVal, ty, arg),
            });
        }
        let call = Expr::Call { func, args };
        let result = match f.result {
            None => Expr::Seq {
                body: vec![Stmt::Expr(call)],
                value: Box::new(Expr::undefined()),
            },
            Some(Type::JsVal) => call,
            Some(ty) => Expr::convert(ty, Type::JsVal, call),
        };
        a.body = vec![
            Stmt::Set(
                this,
                Expr::intrinsic(Intrinsic::CreateThis, vec![Expr::Local(LocalId(1))]),
            ),
            Stmt::Return(Some(Expr::intrinsic(
                Intrinsic::CtorReturn { derived: false },
                vec![result, Expr::Local(this)],
            ))),
        ];
        let id = self.module.add_function(a);
        self.ctor_adapters.insert(func, id);
        id
    }

    /// What calls through a function value run: `f$code(self, this, args) -> jsval` calls the typed
    /// function `func` with the arguments it takes (missing ones `undefined`, extra ones dropped).
    fn adapter(&mut self, func: FuncId) -> FuncId {
        if let Some(a) = self.adapters.get(&func) {
            return *a;
        }
        let f = self.module.function(func).clone();
        let hidden = self.hidden[&func];
        let mut a = ir::Function::new(
            format!("{}$code", f.name),
            &[
                ("self", Type::JsVal),
                ("this", Type::JsVal),
                ("args", Type::JsVal),
            ],
            Some(Type::JsVal),
        );
        a.pos = f.pos;
        let mut args = Vec::new();
        if hidden.self_ {
            args.push(Expr::Local(LocalId(0)));
        }
        if hidden.this {
            args.push(Expr::Local(LocalId(1)));
        }
        if hidden.new_target {
            args.push(Expr::undefined());
        }
        let rest = self.rest_param.contains(&func);
        for i in hidden.count()..f.params {
            let op = if rest && i == f.params - 1 {
                Intrinsic::ArgsRest(i - hidden.count())
            } else {
                Intrinsic::ArgGet(i - hidden.count())
            };
            let arg = Expr::intrinsic(op, vec![Expr::Local(LocalId(2))]);
            args.push(match f.locals[i as usize].ty {
                Type::JsVal => arg,
                // Not known to be `Any` yet (this round is thrown away).
                ty => Expr::convert(Type::JsVal, ty, arg),
            });
        }
        let call = Expr::Call { func, args };
        a.body = vec![Stmt::Return(Some(match f.result {
            None => Expr::Seq {
                body: vec![Stmt::Expr(call)],
                value: Box::new(Expr::undefined()),
            },
            Some(Type::JsVal) => call,
            Some(ty) => Expr::convert(ty, Type::JsVal, call),
        }))];
        let id = self.module.add_function(a);
        self.adapters.insert(func, id);
        id
    }

    /// Hoisting at the start of a statement list: uninitialized cells for captured `let`/`const` a
    /// closure may read before their declaration (TDZ), then the function declarations' objects.
    fn hoist<I>(&mut self, cx: &mut FnCx, list: I, out: &mut Vec<Stmt>)
    where
        I: IntoIterator<Item = &'a Statement<'a>> + Clone,
    {
        for s in list.clone() {
            // A class declaration is like a `let` (T16).
            if let Statement::ClassDeclaration(c) = s
                && let Some(b) = &c.id
                && let Some(sym) = b.symbol_id.get()
                && self.an.tdz.contains(&sym)
                && self.an.captured.contains(&sym)
                && !self.globals.contains_key(&sym)
            {
                self.observe(Slot::Var(sym), Kind::Any);
                let l = cx.func.add_local(format!("{}.cell", b.name), Type::JsVal);
                out.push(Stmt::Set(
                    l,
                    Expr::intrinsic(Intrinsic::NewCellUninit, vec![]),
                ));
                cx.cells.insert(sym, (l, self.an.switch_tdz.contains(&sym)));
                cx.pending.insert(sym);
            }
            let Statement::VariableDeclaration(d) = s else {
                continue;
            };
            if d.kind == VariableDeclarationKind::Var {
                continue;
            }
            for decl in &d.declarations {
                if let BindingPattern::BindingIdentifier(b) = &decl.id
                    && let Some(sym) = b.symbol_id.get()
                    && self.an.tdz.contains(&sym)
                    && self.an.captured.contains(&sym)
                    && !self.globals.contains_key(&sym)
                {
                    // Uninitialized means a Wasm null in a `jsval` cell. (T24: read from a later
                    // `switch` case, which a jump can reach without the declaration: checked.)
                    self.observe(Slot::Var(sym), Kind::Any);
                    let l = cx.func.add_local(format!("{}.cell", b.name), Type::JsVal);
                    out.push(Stmt::Set(
                        l,
                        Expr::intrinsic(Intrinsic::NewCellUninit, vec![]),
                    ));
                    cx.cells.insert(sym, (l, self.an.switch_tdz.contains(&sym)));
                    cx.pending.insert(sym);
                }
            }
        }
        let mut decls = Vec::new();
        for s in list {
            let Statement::FunctionDeclaration(f) = s else {
                continue;
            };
            let (Some(scope), Some(sym)) = (
                f.scope_id.get(),
                f.id.as_ref().and_then(|i| i.symbol_id.get()),
            ) else {
                continue;
            };
            if !self.nested.contains_key(&scope) {
                continue; // top-level
            }
            // Bindings first, so functions can capture each other.
            self.observe(Slot::Var(sym), Kind::Any);
            let name = self.scoping.symbol_name(sym).to_string();
            if self.an.captured.contains(&sym) {
                let l = cx.func.add_local(format!("{name}.cell"), Type::JsVal);
                out.push(Stmt::Set(
                    l,
                    Expr::intrinsic(Intrinsic::NewCell(Type::JsVal), vec![Expr::undefined()]),
                ));
                cx.cells.insert(sym, (l, false));
            } else {
                let l = cx.func.add_local(name, Type::JsVal);
                cx.locals.insert(sym, l);
            }
            decls.push((f, scope, sym));
        }
        for (f, scope, sym) in decls {
            let id = self.nested[&scope].id;
            // Only called directly and captures nothing: no object needed.
            if !self.an.fn_values.contains(&sym) && !self.hidden[&id].self_ {
                let _ = self.lower_nested(scope);
                continue;
            }
            let Ok(obj) = self.closure(cx, scope, None, f.span) else {
                continue;
            };
            out.push(match (cx.cells.get(&sym), cx.locals.get(&sym)) {
                (Some((l, _)), _) => Stmt::Expr(Expr::intrinsic(
                    Intrinsic::CellSet(Type::JsVal),
                    vec![Expr::Local(*l), obj],
                )),
                (_, Some(l)) => Stmt::Set(*l, obj),
                _ => unreachable!("bound above"),
            });
        }
    }

    // ---- statements --------------------------------------------------------------------

    /// Lower a statement list, continuing after errors so every problem is reported.
    fn stmts(&mut self, cx: &mut FnCx, list: &'a [Statement<'a>]) -> Vec<Stmt> {
        let mut out = Vec::new();
        self.hoist(cx, list, &mut out);
        for s in list {
            let _ = self.stmt(cx, s, &mut out);
        }
        out
    }

    /// `switch` (T24): the discriminant in a local, the case bodies' declarations hoisted
    /// together (one scope), then nested blocks, one ending before each case body:
    ///
    /// ```text
    /// block $brk { block $c2 { block $c1 { block $c0 {
    ///   if d === t0 { br $c0 }  if d === t2 { br $c2 } …  br $default (or $brk)
    /// } body0 } body1 } body2 }
    /// ```
    ///
    /// The tests run lazily in source order (skipping `default`); a body falls through into the
    /// next. `break` targets `$brk`.
    fn switch_stmt(
        &mut self,
        cx: &mut FnCx,
        sw: &'a SwitchStatement<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let d = self.expr(cx, &sw.discriminant)?;
        let (ty, kind) = (d.ty, d.kind);
        let tmp = cx.func.add_local("switch", ty);
        out.push(Stmt::Set(tmp, d.e));
        if sw.cases.is_empty() {
            return Ok(());
        }
        self.hoist(cx, sw.cases.iter().flat_map(|c| &c.consequent), out);
        let brk = cx.label();
        let labels: Vec<LabelId> = sw.cases.iter().map(|_| cx.label()).collect();
        let mut result = Ok(());
        let mut dispatch = Vec::new();
        for (i, c) in sw.cases.iter().enumerate() {
            let Some(test) = &c.test else { continue };
            match self.expr(cx, test) {
                Ok(t) => {
                    let d = Val {
                        e: Expr::Local(tmp),
                        ty,
                        kind,
                    };
                    dispatch.push(Stmt::If {
                        cond: equality(d, t, BinOp::Eq, true),
                        then: vec![Stmt::Break(labels[i])],
                        els: vec![],
                    });
                }
                Err(e) => result = Err(e),
            }
        }
        let default = sw.cases.iter().position(|c| c.test.is_none());
        dispatch.push(Stmt::Break(default.map_or(brk, |i| labels[i])));
        cx.targets.push(Target {
            js_label: None,
            brk,
            kind: TargetKind::Switch,
        });
        let mut cur = Stmt::Block {
            label: labels[0],
            body: dispatch,
        };
        let mut last = Vec::new();
        for (i, c) in sw.cases.iter().enumerate() {
            let mut body = vec![cur];
            for s in &c.consequent {
                let _ = self.stmt(cx, s, &mut body);
            }
            match labels.get(i + 1) {
                Some(&label) => cur = Stmt::Block { label, body },
                None => {
                    last = body;
                    break;
                }
            }
        }
        cx.targets.pop();
        out.push(Stmt::Block {
            label: brk,
            body: last,
        });
        result
    }

    fn stmt_body(&mut self, cx: &mut FnCx, s: &'a Statement<'a>) -> Vec<Stmt> {
        let mut out = Vec::new();
        let _ = self.stmt(cx, s, &mut out);
        out
    }

    fn stmt(&mut self, cx: &mut FnCx, s: &'a Statement<'a>, out: &mut Vec<Stmt>) -> R<()> {
        match s {
            Statement::EmptyStatement(_) => Ok(()),
            Statement::ExpressionStatement(e) => self.expr_stmt(cx, &e.expression, out),
            Statement::BlockStatement(b) => {
                out.extend(self.stmts(cx, &b.body));
                Ok(())
            }
            Statement::VariableDeclaration(d) => self.var_decl(cx, d, out),
            Statement::ReturnStatement(r) => {
                let v = match (&r.argument, cx.func.result) {
                    (None, None) => None,
                    (Some(a), None) => {
                        // `has_value_return` gives such functions a result.
                        unreachable!(
                            "return value at {:?} in a function without a result",
                            a.span()
                        )
                    }
                    (a, Some(ty)) => {
                        // `return;` next to value returns returns `undefined`.
                        let v = match a {
                            // In an async generator `return v` awaits `v` (T20).
                            Some(a) if cx.func.flags.is_async && cx.func.flags.is_generator => {
                                let v = jsval_of(self.expr(cx, a)?);
                                Val::js(self.await_value(cx, v), Kind::Any)
                            }
                            Some(a) => {
                                let v = self.expr(cx, a)?;
                                if cx.func.flags.is_async {
                                    self.observe(Slot::Settled(cx.id), v.kind);
                                }
                                v
                            }
                            None => self.undefined_val(),
                        };
                        if cx.ctor.is_some() {
                            Some(self.ctor_result(cx, jsval_of(v)))
                        } else {
                            Some(self.store(Slot::Result(cx.id), ty, v))
                        }
                    }
                };
                out.extend(self.jump(cx, Jump::Return, v));
                Ok(())
            }
            Statement::IfStatement(i) if self.bigint_mode_test(&i.test) => {
                // `if (__intrinsic_bigint_mode()) { … }` in the prelude (T40): the block only in a
                // module that can make a BigInt, else nothing (the IR stays as without it).
                if self.module.bigint {
                    out.extend(self.stmt_body(cx, &i.consequent));
                }
                Ok(())
            }
            Statement::IfStatement(i) => {
                let c = self.cond_expr(cx, &i.test);
                let then = self.stmt_body(cx, &i.consequent);
                let els = match &i.alternate {
                    Some(a) => self.stmt_body(cx, a),
                    None => Vec::new(),
                };
                let cond = c?;
                out.push(Stmt::If { cond, then, els });
                Ok(())
            }
            Statement::SwitchStatement(sw) => self.switch_stmt(cx, sw, out),
            // No debugger to stop in.
            Statement::DebuggerStatement(_) => Ok(()),
            Statement::WhileStatement(w) => self.while_loop(cx, None, w, out),
            Statement::ForInStatement(f) => self.for_in(cx, None, f, out),
            Statement::ForOfStatement(f) => self.for_of(cx, None, f, out),
            Statement::DoWhileStatement(w) => self.do_while_loop(cx, None, w, out),
            Statement::ForStatement(f) => self.for_loop(cx, None, f, out),
            Statement::LabeledStatement(l) => {
                let name = Some(l.label.name.to_string());
                match &l.body {
                    Statement::WhileStatement(w) => self.while_loop(cx, name, w, out),
                    Statement::DoWhileStatement(w) => self.do_while_loop(cx, name, w, out),
                    Statement::ForStatement(f) => self.for_loop(cx, name, f, out),
                    Statement::ForInStatement(f) => self.for_in(cx, name, f, out),
                    Statement::ForOfStatement(f) => self.for_of(cx, name, f, out),
                    body => {
                        let brk = cx.label();
                        cx.targets.push(Target {
                            js_label: name,
                            brk,
                            kind: TargetKind::Block,
                        });
                        let inner = self.stmt_body(cx, body);
                        cx.targets.pop();
                        out.push(Stmt::Block {
                            label: brk,
                            body: inner,
                        });
                        Ok(())
                    }
                }
            }
            Statement::BreakStatement(b) => {
                let name = b.label.as_ref().map(|l| l.name.as_str());
                let Some(t) = find_target(&cx.targets, name, false) else {
                    return Err(self.unsupported(
                        b.span,
                        "`break` outside a loop or `switch`",
                        "later",
                    ));
                };
                out.extend(self.jump(cx, Jump::Break(t), None));
                Ok(())
            }
            Statement::ContinueStatement(c) => {
                let name = c.label.as_ref().map(|l| l.name.as_str());
                let t = find_target(&cx.targets, name, true)
                    .expect("semantic analysis checks continue targets");
                out.extend(self.jump(cx, Jump::Continue(t), None));
                Ok(())
            }
            Statement::ThrowStatement(t) => {
                let v = self.expr(cx, &t.argument)?;
                out.push(Stmt::Throw(jsval_of(v)));
                Ok(())
            }
            Statement::TryStatement(t) => self.try_stmt(cx, t, out),
            // Hoisted to the start of its statement list (`hoist`).
            Statement::FunctionDeclaration(_) => Ok(()),
            Statement::ClassDeclaration(c) => {
                let sym = c.id.as_ref().and_then(|i| i.symbol_id.get());
                let e = self.class_value(cx, c, None, sym)?;
                out.push(Stmt::Expr(e));
                Ok(())
            }
            other => Err(self.unsupported(
                other.span(),
                &format!("statement `{}`", statement_kind(other)),
                "later",
            )),
        }
    }

    /// A `break`, `continue` or `return` (with its converted value). If it leaves a `finally`,
    /// it goes to the innermost such `finally` first, which re-issues it after its body.
    fn jump(&mut self, cx: &mut FnCx, j: Jump, value: Option<Expr>) -> Vec<Stmt> {
        let crossed = match j {
            Jump::Break(t) | Jump::Continue(t) => cx.finally.iter().rposition(|f| t < f.depth),
            Jump::Return => cx.finally.len().checked_sub(1),
        };
        if let Some(i) = crossed {
            let mut out = Vec::new();
            if let Some(v) = value {
                let ty = cx
                    .func
                    .result
                    .expect("a return value in a function with a result");
                let ret = *cx.ret.get_or_insert_with(|| cx.func.add_local("ret", ty));
                if v != Expr::Local(ret) {
                    out.push(Stmt::Set(ret, v));
                }
            }
            let f = &mut cx.finally[i];
            let code = match f.jumps.iter().position(|x| *x == j) {
                Some(k) => k,
                None => {
                    f.jumps.push(j);
                    f.jumps.len() - 1
                }
            };
            out.push(Stmt::Set(f.kind, Expr::i32(code as i32 + 2)));
            out.push(Stmt::Break(f.label));
            return out;
        }
        vec![match j {
            Jump::Break(t) => Stmt::Break(cx.targets[t].brk),
            Jump::Continue(t) => match cx.targets[t].kind {
                TargetKind::While => Stmt::Continue(cx.targets[t].brk),
                TargetKind::Wrapped(Some(l)) => Stmt::Break(l),
                TargetKind::Wrapped(None) => {
                    let l = cx.label();
                    cx.targets[t].kind = TargetKind::Wrapped(Some(l));
                    Stmt::Break(l)
                }
                TargetKind::Block | TargetKind::Switch => {
                    unreachable!("find_target skips blocks and switches for continue")
                }
            },
            Jump::Return => Stmt::Return(value),
        }]
    }

    /// `try`/`catch`/`finally` (design D2). A `finally` is lowered with one copy of its body:
    ///
    /// ```text
    /// kind = 0
    /// block $f { try { <try + catch> } catch (val) { kind = 1 } }   // a jump crossing it: kind = 2+i; br $f
    /// <finally body>
    /// if kind == 2+i { <jump i again> } …   if kind == 1 { throw val }
    /// ```
    fn try_stmt(&mut self, cx: &mut FnCx, t: &'a TryStatement<'a>, out: &mut Vec<Stmt>) -> R<()> {
        let Some(fin) = &t.finalizer else {
            return self.try_catch(cx, t, out);
        };
        let label = cx.label();
        let kind = cx.func.add_local("completion", Type::I32);
        let val = cx.func.add_local("exception", Type::JsVal);
        cx.finally.push(Finally {
            depth: cx.targets.len(),
            label,
            kind,
            jumps: Vec::new(),
        });
        let mut protected = Vec::new();
        let result = self.try_catch(cx, t, &mut protected);
        let f = cx.finally.pop().expect("pushed above");
        out.push(Stmt::Set(kind, Expr::i32(0)));
        out.push(Stmt::Block {
            label,
            body: vec![Stmt::Try {
                body: protected,
                catch_local: Some(val),
                catch_body: vec![Stmt::Set(kind, Expr::i32(1))],
            }],
        });
        out.extend(self.stmts(cx, &fin.body));
        let is = |code: i32| Expr::binary(BinOp::Eq, Type::I32, Expr::Local(kind), Expr::i32(code));
        for (i, j) in f.jumps.into_iter().enumerate() {
            let value = match (j, cx.func.result) {
                (Jump::Return, Some(_)) => cx.ret.map(Expr::Local),
                _ => None,
            };
            let then = self.jump(cx, j, value);
            out.push(Stmt::If {
                cond: is(i as i32 + 2),
                then,
                els: vec![],
            });
        }
        out.push(Stmt::If {
            cond: is(1),
            then: vec![Stmt::Throw(Expr::Local(val))],
            els: vec![],
        });
        result
    }

    /// The `try` block and the `catch` clause, if any.
    fn try_catch(&mut self, cx: &mut FnCx, t: &'a TryStatement<'a>, out: &mut Vec<Stmt>) -> R<()> {
        let body = self.stmts(cx, &t.block.body);
        let Some(h) = &t.handler else {
            out.extend(body);
            return Ok(());
        };
        let mut catch_body = Vec::new();
        let mut catch_local = None;
        let mut result = Ok(());
        match h.param.as_ref().map(|p| &p.pattern) {
            None => {}
            Some(BindingPattern::BindingIdentifier(b)) => {
                let sym = b
                    .symbol_id
                    .get()
                    .expect("semantic analysis assigns symbols");
                let slot = Slot::Var(sym);
                // The thrown value can be anything.
                let tmp = cx.func.add_local("thrown", Type::JsVal);
                let v = Val::js(Expr::Local(tmp), Kind::Any);
                let ty = self.slot_ty(slot);
                let v = self.store(slot, ty, v);
                catch_body.extend(self.declare_store(cx, sym, ty, v, true));
                catch_local = Some(tmp);
            }
            Some(p) => {
                let tmp = cx.func.add_local("thrown", Type::JsVal);
                catch_local = Some(tmp);
                let src = Src::Text("undefined".to_string());
                if let Err(e) = self.pattern(
                    cx,
                    Pat::Bind(p),
                    Expr::Local(tmp),
                    Mode::Decl { fresh: true },
                    Some(src),
                    &mut catch_body,
                ) {
                    result = Err(e);
                }
            }
        }
        catch_body.extend(self.stmts(cx, &h.body.body));
        out.push(Stmt::Try {
            body,
            catch_local,
            catch_body,
        });
        result
    }

    fn var_decl(
        &mut self,
        cx: &mut FnCx,
        d: &'a VariableDeclaration<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        if matches!(
            d.kind,
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing
        ) {
            return Err(self.unsupported(d.span, "`using` declaration", "not scheduled"));
        }
        let mut result = Ok(());
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(b) = &decl.id else {
                // A pattern (its initializer is required by the parser).
                let Some(init) = &decl.init else { continue };
                let r = self.expr(cx, init).and_then(|v| {
                    let src = Src::Text(self.callee_text(init));
                    let fresh = d.kind != VariableDeclarationKind::Var;
                    self.pattern(
                        cx,
                        Pat::Bind(&decl.id),
                        jsval_of(v),
                        Mode::Decl { fresh },
                        Some(src),
                        out,
                    )
                });
                if let Err(e) = r {
                    result = Err(e);
                }
                continue;
            };
            let sym = b
                .symbol_id
                .get()
                .expect("semantic analysis assigns symbols");
            let slot = Slot::Var(sym);
            let v = match &decl.init {
                Some(init) => self.named(cx, init, Some(b.name.as_str())),
                None => Ok(self.undefined_val()),
            };
            let ty = self.slot_ty(slot);
            let is_var = d.kind == VariableDeclarationKind::Var;
            let plain = !self.an.captured.contains(&sym) && !self.globals.contains_key(&sym);
            // Declared even if the initializer failed, so later uses don't report more errors.
            if plain {
                self.local_for(cx, sym, ty);
            }
            let v = match v {
                Ok(v) => v,
                Err(e) => {
                    cx.pending.remove(&sym);
                    result = Err(e);
                    continue;
                }
            };
            let v = self.store(slot, ty, v);
            if decl.init.is_none() && is_var {
                // `var x;` doesn't reset `x`: it's `undefined` from function entry on (a captured
                // one's cell starts as `undefined`, a global too).
                if plain {
                    let local = cx.locals[&sym];
                    if !cx.var_undefined.contains(&local) {
                        cx.var_undefined.push(local);
                    }
                }
                continue;
            }
            out.extend(self.declare_store(cx, sym, ty, v, !is_var));
        }
        result
    }

    fn while_loop(
        &mut self,
        cx: &mut FnCx,
        name: Option<String>,
        w: &'a WhileStatement<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let brk = cx.label();
        let test = self.cond_expr(cx, &w.test);
        cx.targets.push(Target {
            js_label: name,
            brk,
            kind: TargetKind::While,
        });
        let body = self.stmt_body(cx, &w.body);
        cx.targets.pop();
        let test = test?;
        let mut inner = exit_unless(test, brk);
        inner.extend(body);
        inner.push(Stmt::Continue(brk));
        out.push(Stmt::Loop {
            label: brk,
            body: inner,
        });
        Ok(())
    }

    fn do_while_loop(
        &mut self,
        cx: &mut FnCx,
        name: Option<String>,
        w: &'a DoWhileStatement<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let brk = cx.label();
        cx.targets.push(Target {
            js_label: name,
            brk,
            kind: TargetKind::Wrapped(None),
        });
        let body = self.stmt_body(cx, &w.body);
        let t = cx.targets.pop().expect("pushed above");
        let test = self.cond_expr(cx, &w.test);
        let mut inner = wrap_continue(t.kind, body);
        let test = test?;
        inner.push(Stmt::If {
            cond: test,
            then: vec![Stmt::Continue(brk)],
            els: vec![],
        });
        out.push(Stmt::Loop {
            label: brk,
            body: inner,
        });
        Ok(())
    }

    fn for_loop(
        &mut self,
        cx: &mut FnCx,
        name: Option<String>,
        f: &'a ForStatement<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let mut result = Ok(());
        // Captured `let`s of the head: each iteration gets a copy (a new cell), made after the
        // initialization and before each update.
        let mut copies = Vec::new();
        match &f.init {
            None => {}
            Some(ForStatementInit::VariableDeclaration(d)) => {
                result = self.var_decl(cx, d, out);
                if d.kind != VariableDeclarationKind::Var {
                    for decl in &d.declarations {
                        if let BindingPattern::BindingIdentifier(b) = &decl.id
                            && let Some(sym) = b.symbol_id.get()
                            && let Some(&(l, _)) = cx.cells.get(&sym)
                        {
                            let ty = self.slot_ty(Slot::Var(sym));
                            copies.push(Stmt::Set(
                                l,
                                Expr::intrinsic(
                                    Intrinsic::NewCell(ty),
                                    vec![Expr::intrinsic(
                                        Intrinsic::CellGet(ty),
                                        vec![Expr::Local(l)],
                                    )],
                                ),
                            ));
                        }
                    }
                    out.extend(copies.clone());
                }
            }
            Some(init) => {
                let e = init
                    .as_expression()
                    .expect("non-declaration for-init is an expression");
                result = self.expr_stmt(cx, e, out);
            }
        }
        let brk = cx.label();
        let test = f.test.as_ref().map(|t| self.cond_expr(cx, t));
        cx.targets.push(Target {
            js_label: name,
            brk,
            kind: TargetKind::Wrapped(None),
        });
        let body = self.stmt_body(cx, &f.body);
        let t = cx.targets.pop().expect("pushed above");
        let mut update = Vec::new();
        if let Some(u) = &f.update {
            result = result.and(self.expr_stmt(cx, u, &mut update));
        }
        let mut inner = match test {
            Some(t) => exit_unless(t?, brk),
            None => Vec::new(),
        };
        inner.extend(wrap_continue(t.kind, body));
        inner.extend(copies);
        inner.extend(update);
        inner.push(Stmt::Continue(brk));
        out.push(Stmt::Loop {
            label: brk,
            body: inner,
        });
        result
    }

    /// An expression whose value is discarded.
    fn expr_stmt(&mut self, cx: &mut FnCx, e: &'a Expression<'a>, out: &mut Vec<Stmt>) -> R<()> {
        match e {
            Expression::ParenthesizedExpression(p) => self.expr_stmt(cx, &p.expression, out),
            Expression::SequenceExpression(s) => {
                let mut result = Ok(());
                for x in &s.expressions {
                    result = result.and(self.expr_stmt(cx, x, out));
                }
                result
            }
            Expression::AssignmentExpression(a)
                if a.left.as_assignment_target_pattern().is_some() =>
            {
                let v = self.destructuring_assign(cx, a)?;
                out.push(Stmt::Expr(v.e));
                Ok(())
            }
            Expression::AssignmentExpression(a)
                if a.left.as_simple_assignment_target().is_some_and(|t| self.global_target(t).is_some()) =>
            {
                let v = self.global_assign(cx, a)?;
                out.push(Stmt::Expr(v.e));
                Ok(())
            }
            Expression::UpdateExpression(u) if self.global_target(&u.argument).is_some() => {
                let v = self.global_update(cx, u)?;
                out.push(Stmt::Expr(v.e));
                Ok(())
            }
            Expression::AssignmentExpression(a) if a.operator.is_logical() => {
                let v = self.logical_assign(cx, a)?;
                out.push(Stmt::Expr(v.e));
                Ok(())
            }
            Expression::AssignmentExpression(a) if a.left.as_member_expression().is_some() => {
                let v = self.member_assign(cx, a)?;
                out.push(Stmt::Expr(v.e));
                Ok(())
            }
            Expression::UpdateExpression(u) if u.argument.as_member_expression().is_some() => {
                let v = self.member_update(cx, u)?;
                out.push(Stmt::Expr(v.e));
                Ok(())
            }
            Expression::AssignmentExpression(a) => {
                let (var, value) = self.assignment(cx, a)?;
                out.extend(self.write_var(cx, &var, value));
                Ok(())
            }
            Expression::UpdateExpression(u) => {
                let (var, value) = self.update(cx, u)?;
                out.extend(self.write_var(cx, &var, value));
                Ok(())
            }
            Expression::CallExpression(c)
                if self.error_ctor(&c.callee).is_none()
                    && self.object_static(&c.callee).is_none() =>
            {
                let (call, _) = self.call(cx, c)?;
                out.push(Stmt::Expr(call));
                Ok(())
            }
            Expression::UnaryExpression(u) if u.operator == UnaryOperator::Void => {
                self.expr_stmt(cx, &u.argument, out)
            }
            _ => {
                let v = self.expr(cx, e)?;
                out.push(Stmt::Expr(v.e));
                Ok(())
            }
        }
    }

    /// An expression used only for its truthiness (`if`, loop tests, `!`, `?:` tests).
    /// `&&`, `||`, `!` and `?:` combine truthiness directly, so their operands are never boxed.
    fn cond_expr(&mut self, cx: &mut FnCx, e: &'a Expression<'a>) -> R<Expr> {
        match e {
            Expression::ParenthesizedExpression(p) => self.cond_expr(cx, &p.expression),
            Expression::LogicalExpression(l) if l.operator != LogicalOperator::Coalesce => {
                let a = self.cond_expr(cx, &l.left);
                let b = self.cond_expr(cx, &l.right);
                let (a, b) = (a?, b?);
                Ok(if l.operator == LogicalOperator::And {
                    bool_cond(a, b, Expr::bool(false))
                } else {
                    bool_cond(a, Expr::bool(true), b)
                })
            }
            Expression::UnaryExpression(u) if u.operator == UnaryOperator::LogicalNot => {
                let c = self.cond_expr(cx, &u.argument)?;
                Ok(Expr::unary(UnOp::Not, Type::Bool, c))
            }
            Expression::ConditionalExpression(c) => {
                let t = self.cond_expr(cx, &c.test);
                let a = self.cond_expr(cx, &c.consequent);
                let b = self.cond_expr(cx, &c.alternate);
                Ok(bool_cond(t?, a?, b?))
            }
            _ => Ok(cond_of(self.expr(cx, e)?)),
        }
    }

    // ---- expressions -------------------------------------------------------------------

    fn expr(&mut self, cx: &mut FnCx, e: &'a Expression<'a>) -> R<Val> {
        let span = e.span();
        let val = Val::new;
        match e {
            Expression::NumericLiteral(n) => Ok(val(Expr::f64(n.value), Type::F64)),
            Expression::BooleanLiteral(b) => Ok(val(Expr::bool(b.value), Type::Bool)),
            Expression::NullLiteral(_) => Ok(val(Expr::Const(Const::Null), Type::JsVal)),
            Expression::ParenthesizedExpression(p) => {
                let v = self.expr(cx, &p.expression)?;
                Ok(v)
            }
            Expression::Identifier(id) => {
                if self.prelude_leak(id) {
                    return Err(Unsupported);
                }
                if id.name == "arguments"
                    && let Some(l) = cx.arguments
                    && self
                        .scoping
                        .get_reference(id.reference_id())
                        .symbol_id()
                        .is_none()
                {
                    return Ok(Val::js(Expr::Local(l), Kind::Any));
                }
                if matches!(id.name.as_str(), "globalThis" | "global")
                    && self.prelude.global_object
                    && self.scoping.get_reference(id.reference_id()).symbol_id().is_none()
                {
                    // The global object (T42).
                    return Ok(Val::js(self.prelude_call("$$GlobalObject", vec![]), Kind::Any));
                }
                if Builtin::parse(id.name.as_str()).is_none()
                    && let Some(func) = self.builtin_global(e)
                {
                    // `parseInt` as a value: a function object (T21).
                    return Ok(Val::js(self.func_value(func), Kind::Any));
                }
                if let Some(c) = self.global_constant(id) {
                    return Ok(val(Expr::Const(c), c.ty()));
                }
                if let Some(sym) = self.scoping.get_reference(id.reference_id()).symbol_id()
                    && let Some(&func) = self.funcs.get(&sym)
                {
                    return Ok(Val::js(self.func_value(func), Kind::Any));
                }
                if let Some(ns) = ir::Namespace::parse(&id.name)
                    && self.prelude.ctor_values.contains(id.name.as_str())
                    && self
                        .scoping
                        .get_reference(id.reference_id())
                        .symbol_id()
                        .is_none()
                {
                    // `Math`/`JSON` as a value (T25).
                    self.module.namespaces = true;
                    return Ok(Val::js(
                        Expr::intrinsic(Intrinsic::Namespace(ns), vec![]),
                        Kind::Any,
                    ));
                }
                if let Some(b) = self.builtin_ident(e)
                    && self.prelude.ctor_values.contains(id.name.as_str())
                {
                    // A built-in constructor as a value (T25).
                    return Ok(Val::js(
                        Expr::intrinsic(Intrinsic::BuiltinCtor(b), vec![]),
                        Kind::Any,
                    ));
                }
                if let Some(k) = self.error_ctor(e) {
                    // A native error constructor as a value (T16).
                    self.note_error_kind(k);
                    return Ok(Val::js(
                        Expr::intrinsic(Intrinsic::ErrorCtor(k), vec![]),
                        Kind::Any,
                    ));
                }
                if let Some(op) = self.global_function(id) {
                    self.module.promises |= op == Intrinsic::PromiseCtor;
                    if let Intrinsic::Timer(k) = op {
                        return self.timer_value(id.span, k);
                    }
                    return Ok(Val::js(Expr::intrinsic(op, vec![]), Kind::Any));
                }
                if self.undeclared(id) {
                    return Ok(self.global_read(id));
                }
                let var = self.resolve(cx, id)?;
                Ok(self.read_var(&var))
            }
            Expression::BinaryExpression(b) => self.binary(cx, b),
            Expression::LogicalExpression(l) => self.logical(cx, l),
            Expression::UnaryExpression(u) => match u.operator {
                UnaryOperator::LogicalNot => {
                    let c = self.cond_expr(cx, &u.argument)?;
                    Ok(val(Expr::unary(UnOp::Not, Type::Bool, c), Type::Bool))
                }
                UnaryOperator::Void => {
                    let mut body = Vec::new();
                    self.expr_stmt(cx, &u.argument, &mut body)?;
                    let value = Box::new(Expr::undefined());
                    Ok(val(Expr::Seq { body, value }, Type::JsVal))
                }
                UnaryOperator::Typeof => {
                    if let Expression::Identifier(id) = u.argument.without_parentheses()
                        && self.undeclared(id)
                    {
                        // `typeof x` of an unresolvable name doesn't throw (F139): the global
                        // object's property (T42), else `"undefined"`.
                        if !self.prelude.global_object {
                            return Ok(Val::js(Expr::str("undefined"), Kind::Str));
                        }
                        let v = self.prelude_call("$$GlobalRead", vec![Expr::str(&id.name)]);
                        let e = Expr::intrinsic(Intrinsic::TypeofString, vec![v]);
                        return Ok(Val::js(e, Kind::Str));
                    }
                    let v = self.expr(cx, &u.argument)?;
                    let known = match v.kind {
                        Kind::Num | Kind::Bottom => Some("number"),
                        Kind::Bool => Some("boolean"),
                        Kind::Str => Some("string"),
                        Kind::Any => None,
                    };
                    let e = match known {
                        Some(name) => Expr::Seq {
                            body: vec![Stmt::Expr(v.e)],
                            value: Box::new(Expr::str(name)),
                        },
                        None => Expr::intrinsic(Intrinsic::TypeofString, vec![jsval_of(v)]),
                    };
                    Ok(Val::js(e, Kind::Str))
                }
                UnaryOperator::Delete => match u.argument.without_parentheses() {
                    arg @ (Expression::StaticMemberExpression(_)
                    | Expression::ComputedMemberExpression(_)) => {
                        let m = arg.as_member_expression().expect("a member expression");
                        if m.optional() {
                            return Err(self.unsupported(
                                span,
                                "`delete` of an optional chain",
                                "later",
                            ));
                        }
                        let (obj, key) = self.member(cx, m)?;
                        Ok(val(
                            Expr::intrinsic(Intrinsic::DeleteProp, vec![obj, key]),
                            Type::Bool,
                        ))
                    }
                    Expression::ChainExpression(_) | Expression::PrivateFieldExpression(_) => {
                        Err(self.unsupported(
                            span,
                            "`delete` of an optional chain or private field",
                            "later",
                        ))
                    }
                    // `delete x` is a syntax error in strict code; anything else is evaluated, then true.
                    arg => {
                        let mut body = Vec::new();
                        self.expr_stmt(cx, arg, &mut body)?;
                        Ok(val(
                            Expr::Seq {
                                body,
                                value: Box::new(Expr::bool(true)),
                            },
                            Type::Bool,
                        ))
                    }
                },
                UnaryOperator::UnaryNegation
                    if matches!(u.argument.without_parentheses(), Expression::BigIntLiteral(_)) =>
                {
                    // `-5n` (T40): a negative constant.
                    let Expression::BigIntLiteral(b) = u.argument.without_parentheses() else {
                        unreachable!("checked by the guard")
                    };
                    let digits = Expr::Str(format!("-{}", b.value).encode_utf16().collect());
                    Ok(Val::js(
                        Expr::intrinsic(Intrinsic::BigIntConst, vec![digits]),
                        Kind::Any,
                    ))
                }
                op => {
                    let v = self.expr(cx, &u.argument)?;
                    if op != UnaryOperator::UnaryPlus && self.may_be_bigint(&v) {
                        // T40: `-x` / `~x` of a BigInt.
                        let n = if op == UnaryOperator::UnaryNegation {
                            ir::NumericOp::Neg
                        } else {
                            ir::NumericOp::BitNot
                        };
                        return Ok(Val::js(
                            Expr::intrinsic(Intrinsic::Numeric(n), vec![jsval_of(v)]),
                            Kind::Any,
                        ));
                    }
                    let v = self.num(v, span)?;
                    let e = match op {
                        UnaryOperator::UnaryNegation => Expr::unary(UnOp::Neg, Type::F64, v),
                        UnaryOperator::UnaryPlus => v,
                        UnaryOperator::BitwiseNot => {
                            from_i32(Expr::unary(UnOp::BitNot, Type::I32, to_i32(v)))
                        }
                        _ => unreachable!("all unary operators are handled"),
                    };
                    Ok(val(e, Type::F64))
                }
            },
            Expression::ConditionalExpression(c) => {
                let test = self.cond_expr(cx, &c.test);
                let then = self.expr(cx, &c.consequent);
                let els = self.expr(cx, &c.alternate);
                let (cond, then, els) = (test?, then?, els?);
                let (then, els, ty, kind) = unify(then, els);
                let e = Expr::Cond {
                    cond: Box::new(cond),
                    then: Box::new(then),
                    els: Box::new(els),
                    ty,
                };
                Ok(Val { e, ty, kind })
            }
            Expression::AssignmentExpression(a)
                if a.left.as_assignment_target_pattern().is_some() =>
            {
                self.destructuring_assign(cx, a)
            }
            Expression::AssignmentExpression(a)
                if a.left.as_simple_assignment_target().is_some_and(|t| self.global_target(t).is_some()) =>
            {
                self.global_assign(cx, a)
            }
            Expression::UpdateExpression(u) if self.global_target(&u.argument).is_some() => {
                self.global_update(cx, u)
            }
            Expression::AssignmentExpression(a) if a.operator.is_logical() => {
                self.logical_assign(cx, a)
            }
            Expression::AssignmentExpression(a) if a.left.as_member_expression().is_some() => {
                self.member_assign(cx, a)
            }
            Expression::UpdateExpression(u) if u.argument.as_member_expression().is_some() => {
                self.member_update(cx, u)
            }
            Expression::AssignmentExpression(a) => {
                let (var, value) = self.assignment(cx, a)?;
                Ok(Val {
                    e: self.assign_value(cx, &var, value),
                    ty: var.ty,
                    kind: var.kind,
                })
            }
            Expression::UpdateExpression(u) if u.prefix => {
                let (var, value) = self.update(cx, u)?;
                Ok(Val {
                    e: self.assign_value(cx, &var, value),
                    ty: var.ty,
                    kind: var.kind,
                })
            }
            Expression::UpdateExpression(u)
                if self.module.bigint && {
                    let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &u.argument else {
                        unreachable!("members are handled above")
                    };
                    self.resolve(cx, id).is_ok_and(|v| v.kind == Kind::Any)
                } =>
            {
                // Postfix (T40): the old value's ToNumeric, which may be a BigInt.
                let (var, op) = self.update_target(cx, u)?;
                let old = cx.func.add_local("old", Type::JsVal);
                let read = jsval_of(self.read_var(&var));
                let new = Val::js(
                    Expr::intrinsic(Intrinsic::Numeric(inc_dec(op)), vec![Expr::Local(old)]),
                    Kind::Any,
                );
                let new = self.store(Slot::Var(var.sym), var.ty, new);
                let mut body = vec![Stmt::Set(
                    old,
                    Expr::intrinsic(Intrinsic::ToNumeric, vec![read]),
                )];
                body.extend(self.write_var(cx, &var, new));
                let value = Box::new(Expr::Local(old));
                Ok(Val::js(Expr::Seq { body, value }, Kind::Any))
            }
            Expression::UpdateExpression(u) => {
                // Postfix: the value is the old value as a number.
                let (var, op) = self.update_target(cx, u)?;
                let old = cx.func.add_local("old", Type::F64);
                let new = Val::num(Expr::binary(
                    op,
                    Type::F64,
                    Expr::Local(old),
                    Expr::f64(1.0),
                ));
                let new = self.store(Slot::Var(var.sym), var.ty, new);
                let mut body = vec![Stmt::Set(old, self.num(self.read_var(&var), span)?)];
                body.extend(self.write_var(cx, &var, new));
                let value = Box::new(Expr::Local(old));
                Ok(val(Expr::Seq { body, value }, Type::F64))
            }
            Expression::NewExpression(n) => match self.error_ctor(&n.callee) {
                Some(k) => self.new_error(cx, k, &n.arguments, span),
                None => self.new_expr(cx, n),
            },
            Expression::CallExpression(c) if matches!(c.callee, Expression::Super(_)) => {
                self.super_call(cx, c)
            }
            Expression::CallExpression(c)
                if !c.optional && self.error_ctor(&c.callee).is_some() =>
            {
                let k = self.error_ctor(&c.callee).expect("checked by the guard");
                self.new_error(cx, k, &c.arguments, span)
            }
            Expression::CallExpression(c)
                if !c.optional && self.object_static(&c.callee).is_some() =>
            {
                self.object_call(cx, c)
            }
            Expression::CallExpression(c)
                if !c.optional && self.builtin_ident(&c.callee) == Some(Builtin::Symbol) =>
            {
                let args = self.args(cx, &c.arguments, 1)?;
                Ok(Val::js(
                    Expr::intrinsic(Intrinsic::NewSymbol, args),
                    Kind::Any,
                ))
            }
            Expression::CallExpression(c) => {
                let (call, kind) = self.call(cx, c)?;
                self.call_result(call, kind)
            }
            Expression::SequenceExpression(s) => {
                let (last, init) = s.expressions.split_last().expect("non-empty sequence");
                let mut body = Vec::new();
                let mut result = Ok(());
                for x in init {
                    result = result.and(self.expr_stmt(cx, x, &mut body));
                }
                let v = self.expr(cx, last);
                result?;
                let v = v?;
                let value = Box::new(v.e);
                Ok(Val {
                    e: Expr::Seq { body, value },
                    ..v
                })
            }
            Expression::StringLiteral(s) => Ok(Val::js(
                Expr::Str(utf16(s.value.as_str(), s.lone_surrogates)),
                Kind::Str,
            )),
            Expression::TemplateLiteral(t) => self.template(cx, t),
            Expression::RegExpLiteral(r) => {
                // A new RegExp per evaluation (T27); oxc checked the pattern.
                let src = Expr::Str(utf16(r.regex.pattern.text.as_str(), false));
                let flags = Expr::Str(utf16(&r.regex.flags.to_string(), false));
                let e = self.prelude_call(builtins::REGEXP_CREATE, vec![src, flags]);
                Ok(Val::js(e, Kind::Any))
            }
            Expression::StaticMemberExpression(m) if m.property.name == "length" && !m.optional => {
                self.hint_string(cx, &m.object);
                let obj = self.expr(cx, &m.object)?;
                let kind = obj.kind;
                if kind == Kind::Str {
                    return Ok(Val::num(Expr::intrinsic(
                        Intrinsic::StringLength,
                        vec![jsval_of(obj)],
                    )));
                }
                let len =
                    Expr::intrinsic(Intrinsic::GetProp, vec![jsval_of(obj), Expr::str("length")]);
                Ok(match kind {
                    // Not known yet: any f64 will do, this round is thrown away.
                    Kind::Bottom => Val {
                        e: Expr::convert(Type::JsVal, Type::F64, len),
                        ty: Type::F64,
                        kind: Kind::Bottom,
                    },
                    _ => Val::js(len, Kind::Any),
                })
            }
            Expression::BigIntLiteral(b) => {
                // T40: a constant made once (its decimal digits).
                let digits = Expr::Str(b.value.encode_utf16().collect());
                Ok(Val::js(
                    Expr::intrinsic(Intrinsic::BigIntConst, vec![digits]),
                    Kind::Any,
                ))
            }
            Expression::ObjectExpression(o) => self.object_literal(cx, o),
            Expression::ArrayExpression(a) => Ok(Val::js(
                self.array_literal(cx, &a.elements, None, span)?,
                Kind::Any,
            )),
            Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {
                self.named(cx, e, None)
            }
            Expression::ThisExpression(_) => Ok(Val::js(self.this_value(cx), Kind::Any)),
            Expression::NewTarget(nt) => Ok(Val::js(
                cx.new_target
                    .filter(|_| !self.an.static_new_target.contains(&nt.span.start))
                    .map_or_else(Expr::undefined, Expr::Local),
                Kind::Any,
            )),
            Expression::ClassExpression(c) => {
                Ok(Val::js(self.class_value(cx, c, None, None)?, Kind::Any))
            }
            Expression::StaticMemberExpression(_) if self.builtin_static_value(e).is_some() => {
                let func = self.builtin_static_value(e).expect("checked");
                self.check_builtin(span, func)?;
                Ok(Val::js(self.func_value(func), Kind::Any))
            }
            Expression::StaticMemberExpression(sm)
                if !sm.optional
                    && self.is_global(&sm.object, "process")
                    && sm.property.name == "nextTick" =>
            {
                self.timer_value(span, ir::TIMER_NEXT_TICK)
            }
            Expression::StaticMemberExpression(sm)
                if self.is_global(&sm.object, "Math")
                    && !(self.prelude.ctor_values.contains("Math")
                        && builtins::inherited_prop("Math", &sm.property.name)) =>
            {
                if sm.optional {
                    return Err(self.unsupported(span, "`Math?.x`", "later"));
                }
                let p = sm.property.name.as_str();
                let Some(&(_, v)) = ir::MATH_CONSTANTS.iter().find(|(n, _)| *n == p) else {
                    return Err(self.unsupported(span, &format!("`Math.{p}`"), "T21"));
                };
                Ok(Val::num(Expr::f64(v)))
            }
            Expression::StaticMemberExpression(sm)
                if !sm.optional
                    && self.is_global(&sm.object, "Number")
                    && number_constant(&sm.property.name).is_some() =>
            {
                let v = number_constant(&sm.property.name).expect("checked");
                Ok(Val::num(Expr::f64(v)))
            }
            Expression::StaticMemberExpression(sm)
                if !sm.optional && self.builtin_ident(&sm.object) == Some(Builtin::Symbol) =>
            {
                if sm.property.name == "iterator" {
                    return Ok(Val::js(
                        Expr::intrinsic(Intrinsic::SymbolIterator, vec![]),
                        Kind::Any,
                    ));
                }
                if sm.property.name == "prototype" {
                    return Ok(Val::js(
                        Expr::intrinsic(Intrinsic::Prototype(Builtin::Symbol), vec![]),
                        Kind::Any,
                    ));
                }
                if let Some(k) = ir::well_known_symbol(&sm.property.name) {
                    return Ok(Val::js(
                        Expr::intrinsic(Intrinsic::WellKnownSymbol(k), vec![]),
                        Kind::Any,
                    ));
                }
                Err(self.unsupported(span, &format!("`Symbol.{}`", sm.property.name), "T21"))
            }
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_)
                if matches!(
                    e.as_member_expression().map(|m| m.object()),
                    Some(Expression::Super(_))
                ) =>
            {
                let m = e.as_member_expression().expect("a member expression");
                let (get, _) = self.super_member(cx, m)?;
                Ok(Val::js(get, Kind::Any))
            }
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => {
                let m = e.as_member_expression().expect("a member expression");
                if let Some(b) = self.builtin_prototype(m) {
                    return Ok(Val::js(
                        Expr::intrinsic(Intrinsic::Prototype(b), vec![]),
                        Kind::Any,
                    ));
                }
                if m.optional() {
                    // oxc wraps optional links in a `ChainExpression`.
                    return self.chain_member(cx, m);
                }
                let (obj, key) = self.member(cx, m)?;
                Ok(Val::js(
                    Expr::intrinsic(Intrinsic::GetProp, vec![obj, key]),
                    Kind::Any,
                ))
            }
            Expression::ChainExpression(c) => match &c.expression {
                ChainElement::CallExpression(call) => self.chain_call(cx, call),
                ChainElement::TSNonNullExpression(t) => {
                    Err(self.unsupported(t.span, "TypeScript non-null assertion", "not scheduled"))
                }
                el => {
                    let m = el.as_member_expression().expect("the other chain elements");
                    self.chain_member(cx, m)
                }
            },
            Expression::PrivateFieldExpression(pf) => self.private_get(cx, pf),
            Expression::YieldExpression(y) => {
                let arg = match &y.argument {
                    Some(a) => jsval_of(self.expr(cx, a)?),
                    None => Expr::undefined(),
                };
                let async_gen = cx.func.flags.is_async && cx.func.flags.is_generator;
                let e = if y.delegate {
                    if async_gen {
                        self.async_yield_star(cx, arg)
                    } else {
                        self.yield_star(cx, arg)
                    }
                } else if async_gen {
                    // `yield v` awaits `v` first (T20).
                    let arg = self.await_value(cx, arg);
                    self.suspend(cx, ir::SuspendKind::Yield, arg)
                } else {
                    self.suspend(cx, ir::SuspendKind::Yield, arg)
                };
                Ok(Val::js(e, Kind::Any))
            }
            Expression::AwaitExpression(a) => {
                let arg = jsval_of(self.expr(cx, &a.argument)?);
                Ok(Val::js(self.await_value(cx, arg), Kind::Any))
            }
            Expression::PrivateInExpression(p) => {
                let (sym, ..) = self.private_sym(cx, &p.left)?;
                let o = jsval_of(self.expr(cx, &p.right)?);
                Ok(Val::new(
                    Expr::intrinsic(Intrinsic::PrivateHas, vec![sym, o]),
                    Type::Bool,
                ))
            }
            other => Err(self.unsupported(
                span,
                &format!("expression `{}`", expression_kind(other)),
                "later",
            )),
        }
    }

    fn binary(&mut self, cx: &mut FnCx, b: &'a BinaryExpression<'a>) -> R<Val> {
        let span = b.span;
        use BinaryOperator as B;
        if b.operator.is_equality()
            && let Some(v) = self.typeof_compare(cx, b)?
        {
            return Ok(v);
        }
        if b.operator == B::Instanceof {
            let Some(builtin) = self.builtin_ident(&b.right) else {
                let l = self.expr(cx, &b.left);
                let r = self.expr(cx, &b.right);
                let (l, r) = (l?, r?);
                return Ok(Val::new(
                    Expr::intrinsic(Intrinsic::InstanceOfValue, vec![jsval_of(l), jsval_of(r)]),
                    Type::Bool,
                ));
            };
            let l = self.expr(cx, &b.left)?;
            return Ok(Val::new(
                Expr::intrinsic(Intrinsic::InstanceOf(builtin), vec![jsval_of(l)]),
                Type::Bool,
            ));
        }
        let l = self.expr(cx, &b.left);
        let r = self.expr(cx, &b.right);
        let (l, r) = (l?, r?);
        if b.operator == B::In {
            return Ok(Val::new(
                Expr::intrinsic(Intrinsic::HasProp, vec![jsval_of(l), jsval_of(r)]),
                Type::Bool,
            ));
        }
        let num = Val::num;
        let bitwise = match b.operator {
            B::BitwiseOR => Some(BinOp::Or),
            B::BitwiseAnd => Some(BinOp::And),
            B::BitwiseXOR => Some(BinOp::Xor),
            B::ShiftLeft => Some(BinOp::Shl),
            B::ShiftRight => Some(BinOp::Shr),
            _ => None,
        };
        let numeric = |op, l, r| {
            // T40: a BigInt may meet the operator.
            Expr::intrinsic(
                Intrinsic::Numeric(ir::NumericOp::Binary(op)),
                vec![jsval_of(l), jsval_of(r)],
            )
        };
        let may_be_bigint = self.may_be_bigint(&l) || self.may_be_bigint(&r);
        if let Some(op) = bitwise {
            if may_be_bigint {
                return Ok(Val::js(numeric(op, l, r), Kind::Any));
            }
            let (l, r) = (to_i32(self.num(l, span)?), to_i32(self.num(r, span)?));
            return Ok(num(from_i32(Expr::binary(op, Type::I32, l, r))));
        }
        if b.operator == B::ShiftRightZeroFill {
            if may_be_bigint {
                return Ok(Val::js(numeric(BinOp::ShrU, l, r), Kind::Any));
            }
            let (l, r) = (self.num(l, span)?, self.num(r, span)?);
            return Ok(num(ushr(cx, l, r)));
        }
        if b.operator == B::Addition || b.operator.is_equality() {
            // A parameter next to a known string is used as a string.
            if l.kind == Kind::Str {
                self.hint_string(cx, &b.right);
            }
            if r.kind == Kind::Str {
                self.hint_string(cx, &b.left);
            }
        }
        if b.operator == B::Addition {
            return Ok(add(l, r));
        }
        let eq = match b.operator {
            B::StrictEquality => Some((BinOp::Eq, true)),
            B::StrictInequality => Some((BinOp::Ne, true)),
            B::Equality => Some((BinOp::Eq, false)),
            B::Inequality => Some((BinOp::Ne, false)),
            _ => None,
        };
        if let Some((op, strict)) = eq {
            return Ok(Val::new(equality(l, r, op, strict), Type::Bool));
        }
        // Relational operators on two JS values: strings compare as strings (T21). T40: also when
        // one operand may be a BigInt.
        if (l.ty == Type::JsVal && r.ty == Type::JsVal) || may_be_bigint {
            let less = match b.operator {
                B::LessThan => Some((false, false)),
                B::GreaterThan => Some((true, false)),
                B::LessEqualThan => Some((true, true)),
                B::GreaterEqualThan => Some((false, true)),
                _ => None,
            };
            if let Some((swap, or_equal)) = less {
                let e = Expr::intrinsic(
                    Intrinsic::JsLess { swap, or_equal },
                    vec![jsval_of(l), jsval_of(r)],
                );
                return Ok(Val::new(e, Type::Bool));
            }
        }
        // Arithmetic and relational operators on primitives: ToNumber on both operands.
        let op = match b.operator {
            B::Subtraction => BinOp::Sub,
            B::Multiplication => BinOp::Mul,
            B::Division => BinOp::Div,
            B::Remainder => BinOp::Rem,
            B::Exponential => BinOp::Pow,
            B::LessThan => BinOp::Lt,
            B::LessEqualThan => BinOp::Le,
            B::GreaterThan => BinOp::Gt,
            B::GreaterEqualThan => BinOp::Ge,
            _ => {
                return Err(self.unsupported(
                    span,
                    &format!("operator `{}`", b.operator.as_str()),
                    "later",
                ));
            }
        };
        if may_be_bigint {
            return Ok(Val::js(numeric(op, l, r), Kind::Any));
        }
        let e = Expr::binary(op, Type::F64, self.num(l, span)?, self.num(r, span)?);
        let ty = if op.is_comparison() {
            Type::Bool
        } else {
            Type::F64
        };
        Ok(Val::new(e, ty))
    }

    /// A template literal: the concatenation of its strings and the ToString of its expressions.
    fn template(&mut self, cx: &mut FnCx, t: &'a TemplateLiteral<'a>) -> R<Val> {
        let mut parts = Vec::new();
        let mut result = Ok(());
        for (i, q) in t.quasis.iter().enumerate() {
            let Some(cooked) = &q.value.cooked else {
                return Err(self.unsupported(
                    q.span,
                    "invalid escape in a template literal",
                    "never",
                ));
            };
            if !cooked.is_empty() {
                parts.push(Expr::Str(utf16(cooked.as_str(), q.lone_surrogates)));
            }
            if let Some(e) = t.expressions.get(i) {
                self.hint_string(cx, e);
                match self.expr(cx, e) {
                    Ok(v) => parts.push(to_str(v)),
                    Err(e) => result = Err(e),
                }
            }
        }
        result?;
        let e = parts
            .into_iter()
            .reduce(|a, b| Expr::intrinsic(Intrinsic::Concat, vec![a, b]))
            .unwrap_or_else(|| Expr::str(""));
        Ok(Val::js(e, Kind::Str))
    }

    /// `typeof x === "tag"` (also `==`, `!==`, `!=`, either side). `None` if `b` isn't one.
    fn typeof_compare(&mut self, cx: &mut FnCx, b: &'a BinaryExpression<'a>) -> R<Option<Val>> {
        let (u, s) = match (b.left.without_parentheses(), b.right.without_parentheses()) {
            (Expression::UnaryExpression(u), Expression::StringLiteral(s))
            | (Expression::StringLiteral(s), Expression::UnaryExpression(u))
                if u.operator == UnaryOperator::Typeof =>
            {
                (u, s)
            }
            _ => return Ok(None),
        };
        if let Expression::Identifier(id) = u.argument.without_parentheses()
            && self.undeclared(id)
        {
            // `typeof x` is `"undefined"` without reading `x` (F139): the general comparison.
            return Ok(None);
        }
        let v = self.expr(cx, &u.argument)?;
        let tag = TypeofTag::parse(s.value.as_str());
        let known = match v.kind {
            Kind::Num | Kind::Bottom => Some(TypeofTag::Number),
            Kind::Bool => Some(TypeofTag::Boolean),
            Kind::Str => Some(TypeofTag::String),
            Kind::Any => None,
        };
        let e = match (known, tag) {
            (None, Some(tag)) => Expr::TypeofIs {
                arg: Box::new(jsval_of(v)),
                tag,
            },
            // Known statically, or no value has that typeof.
            (known, tag) => Expr::Seq {
                body: vec![Stmt::Expr(v.e)],
                value: Box::new(Expr::bool(known.is_some() && known == tag)),
            },
        };
        let negate = matches!(
            b.operator,
            BinaryOperator::StrictInequality | BinaryOperator::Inequality
        );
        let e = if negate {
            Expr::unary(UnOp::Not, Type::Bool, e)
        } else {
            e
        };
        Ok(Some(Val::new(e, Type::Bool)))
    }

    fn logical(&mut self, cx: &mut FnCx, l: &'a LogicalExpression<'a>) -> R<Val> {
        let a = self.expr(cx, &l.left);
        let b = self.expr(cx, &l.right);
        let (a, b) = (a?, b?);
        if l.operator == LogicalOperator::Coalesce {
            if a.kind != Kind::Any {
                // A number, boolean or string is never null/undefined: `a ?? b` is `a`.
                // (`Bottom`: not known yet.)
                return Ok(a);
            }
            let tmp = cx.func.add_local("tmp", Type::JsVal);
            let nullish = Expr::binary(
                BinOp::LooseEq,
                Type::JsVal,
                Expr::Tee(tmp, Box::new(a.e)),
                Expr::Const(Const::Null),
            );
            let e = Expr::Cond {
                cond: Box::new(nullish),
                then: Box::new(jsval_of(b)),
                els: Box::new(Expr::Local(tmp)),
                ty: Type::JsVal,
            };
            return Ok(Val::js(e, Kind::Any));
        }
        let and = l.operator == LogicalOperator::And;
        // The result is one of the operands, so they get a common type.
        let (a, b, ty, kind) = unify(a, b);
        let e = if ty == Type::Bool {
            if and {
                bool_cond(a, b, Expr::bool(false))
            } else {
                bool_cond(a, Expr::bool(true), b)
            }
        } else {
            // Evaluate `a` once into a temp.
            let tmp = cx.func.add_local("tmp", ty);
            let cond = cond_of(Val {
                e: Expr::Tee(tmp, Box::new(a)),
                ty,
                kind,
            });
            let (then, els) = if and {
                (b, Expr::Local(tmp))
            } else {
                (Expr::Local(tmp), b)
            };
            Expr::Cond {
                cond: Box::new(cond),
                then: Box::new(then),
                els: Box::new(els),
                ty,
            }
        };
        Ok(Val { e, ty, kind })
    }

    fn call(&mut self, cx: &mut FnCx, c: &'a CallExpression<'a>) -> R<(Expr, Call)> {
        let span = c.span;
        if c.optional {
            return Err(self.unsupported(span, "optional call `?.()`", "T14"));
        }
        if let Some(stderr) = self.console_method(&c.callee) {
            return Ok((self.console(cx, c, stderr)?, Call::Console));
        }
        if matches!(c.callee, Expression::Super(_)) {
            return Ok((self.super_call(cx, c)?.e, Call::Value));
        }
        let callee = c.callee.without_parentheses();
        if let Expression::Identifier(id) = callee
            && id.name.starts_with("__intrinsic_")
            && (self.p.in_prelude(span.start) || self.p.is_generated(span.start))
            && self
                .scoping
                .get_reference(id.reference_id())
                .symbol_id()
                .is_none()
        {
            return self.intrinsic_call(cx, c, &id.name["__intrinsic_".len()..]);
        }
        if c.arguments.iter().any(|a| a.is_spread()) {
            return self.spread_call(cx, c);
        }
        if let Expression::StaticMemberExpression(m) = callee
            && !m.optional
            && m.property.name == "nextTick"
            && self.is_global(&m.object, "process")
        {
            // `process.nextTick(…)` (T25): `this` doesn't matter.
            let f = jsval_of(self.expr(cx, callee)?);
            let args = self.value_args(cx, &c.arguments)?;
            return Ok((
                Expr::CallRef {
                    callee: Box::new(f),
                    this: Box::new(Expr::undefined()),
                    args,
                    what: self.callee_text(&c.callee),
                },
                Call::Value,
            ));
        }
        if let Some(b) = builtins::NEW_ONLY
            .iter()
            .copied()
            .find(|b| self.is_global(callee, b))
        {
            // `Map(…)` without `new` (T25): the arguments, then node's TypeError.
            let mut body: Vec<Stmt> = self
                .value_args(cx, &c.arguments)?
                .into_iter()
                .map(Stmt::Expr)
                .collect();
            let msg = Expr::str(&format!("Constructor {b} requires 'new'"));
            body.push(Stmt::Throw(Expr::intrinsic(
                Intrinsic::NewError(ErrorKind::TypeError),
                vec![msg, Expr::undefined()],
            )));
            let e = Expr::Seq {
                body,
                value: Box::new(Expr::undefined()),
            };
            return Ok((e, Call::Intrinsic(Some(Kind::Any))));
        }
        if let Some(func) = self
            .builtin_static(callee)
            .or_else(|| self.builtin_global(callee))
        {
            // `Math.floor(x)`, `parseInt(s)`: a direct call of the prelude function (T21).
            self.check_builtin(callee.span(), func)?;
            return self.direct_call(cx, func, None, &c.arguments);
        }
        if let Expression::StaticMemberExpression(m) = callee
            && let Expression::Identifier(obj) = m.object.without_parentheses()
            && self.is_global(&m.object, &obj.name)
            && self.prelude.has_statics(&obj.name)
            && !(self.prelude.ctor_values.contains(obj.name.as_str())
                && builtins::inherited_prop(&obj.name, &m.property.name))
        {
            // `Math.nope(…)`, `JSON.parse(…)`: a static the prelude doesn't implement.
            let what = format!("`{}.{}`", obj.name, m.property.name);
            return Err(self.unsupported(m.span, &what, "T21"));
        }
        if let Expression::Identifier(id) = callee
            && let Some(sym) = self.scoping.get_reference(id.reference_id()).symbol_id()
        {
            if self.prelude_leak(id) {
                return Err(Unsupported);
            }
            if let Some(&func) = self.funcs.get(&sym) {
                return self.direct_call(cx, func, None, &c.arguments);
            }
            if let Some(&scope) = self.nested_decl.get(&sym) {
                let func = self.nested[&scope].id;
                let this = if self.hidden[&func].self_ {
                    // The function object: read the binding.
                    let var = self.resolve(cx, id)?;
                    Some(jsval_of(self.read_var(&var)))
                } else {
                    None
                };
                return self.direct_call(cx, func, this, &c.arguments);
            }
        }
        if let Some(MemberExpression::PrivateFieldExpression(pf)) = callee.as_member_expression() {
            // `o.#m(…)`: `this` is `o`.
            if pf.optional {
                return Err(self.unsupported(
                    pf.span,
                    "optional chain with a private field",
                    "later",
                ));
            }
            let obj = jsval_of(self.expr(cx, &pf.object)?);
            let (sym, msg, _) = self.private_sym(cx, &pf.field)?;
            let t = cx.func.add_local("this", Type::JsVal);
            let f = Expr::Seq {
                body: vec![Stmt::Set(t, obj)],
                value: Box::new(Expr::intrinsic(
                    Intrinsic::PrivateGet,
                    vec![Expr::Local(t), sym, Expr::str(&msg)],
                )),
            };
            let args = self.value_args(cx, &c.arguments)?;
            return Ok((
                Expr::CallRef {
                    callee: Box::new(f),
                    this: Box::new(Expr::Local(t)),
                    args,
                    what: self.callee_text(&c.callee),
                },
                Call::Value,
            ));
        }
        if let Some(m) = callee.as_member_expression()
            && matches!(m.object(), Expression::Super(_))
        {
            // `super.m(…)`: `this` is the current `this`.
            let (f, this) = self.super_member(cx, m)?;
            let args = self.value_args(cx, &c.arguments)?;
            return Ok((
                Expr::CallRef {
                    callee: Box::new(f),
                    this: Box::new(this),
                    args,
                    what: self.callee_text(&c.callee),
                },
                Call::Value,
            ));
        }
        if let Some(m) = callee.as_member_expression()
            && !matches!(m, MemberExpression::PrivateFieldExpression(_))
            && self.builtin_prototype(m).is_none()
        {
            // `o.m(…)`: `this` is `o`.
            let (obj, key) = self.member(cx, m)?;
            let t = cx.func.add_local("this", Type::JsVal);
            let f = Expr::Seq {
                body: vec![Stmt::Set(t, obj)],
                value: Box::new(Expr::intrinsic(
                    Intrinsic::GetProp,
                    vec![Expr::Local(t), key],
                )),
            };
            let args = self.value_args(cx, &c.arguments)?;
            return Ok((
                Expr::CallRef {
                    callee: Box::new(f),
                    this: Box::new(Expr::Local(t)),
                    args,
                    what: self.callee_text(&c.callee),
                },
                Call::Value,
            ));
        }
        let f = self.expr(cx, &c.callee)?;
        let args = self.value_args(cx, &c.arguments)?;
        Ok((
            Expr::CallRef {
                callee: Box::new(jsval_of(f)),
                this: Box::new(Expr::undefined()),
                args,
                what: self.callee_text(&c.callee),
            },
            Call::Value,
        ))
    }

    /// `f(...args)`: the arguments go in an array, the call through the function object.
    fn spread_call(&mut self, cx: &mut FnCx, c: &'a CallExpression<'a>) -> R<(Expr, Call)> {
        let callee = c.callee.without_parentheses();
        let (f, this, pre) = match callee.as_member_expression() {
            Some(m)
                if !matches!(m, MemberExpression::PrivateFieldExpression(_))
                    && self.builtin_prototype(m).is_none()
                    && self.builtin_static(callee).is_none() =>
            {
                let (obj, key) = self.member(cx, m)?;
                let t = cx.func.add_local("this", Type::JsVal);
                (
                    Expr::intrinsic(Intrinsic::GetProp, vec![Expr::Local(t), key]),
                    Expr::Local(t),
                    vec![Stmt::Set(t, obj)],
                )
            }
            _ => (
                jsval_of(self.expr(cx, &c.callee)?),
                Expr::undefined(),
                Vec::new(),
            ),
        };
        let tf = cx.func.add_local("callee", Type::JsVal);
        let mut body = pre;
        body.push(Stmt::Set(tf, f));
        let args = self.spread_args(cx, &c.arguments, c.span)?;
        let call = Expr::intrinsic(
            Intrinsic::CallList,
            vec![
                Expr::Local(tf),
                this,
                args,
                Expr::str(&self.callee_text(&c.callee)),
            ],
        );
        Ok((
            Expr::Seq {
                body,
                value: Box::new(call),
            },
            Call::Value,
        ))
    }

    /// The value of a call (`call` + what `Lowerer::call` said about it).
    pub(crate) fn call_result(&mut self, call: Expr, kind: Call) -> R<Val> {
        let func = match kind {
            Call::Direct(func) => func,
            Call::Value => return Ok(Val::js(call, Kind::Any)),
            Call::Intrinsic(Some(Kind::Num)) => return Ok(Val::num(call)),
            Call::Intrinsic(Some(k)) => return Ok(Val::js(call, k)),
            Call::Intrinsic(None) => {
                return Ok(Val::js(
                    Expr::Seq {
                        body: vec![Stmt::Expr(call)],
                        value: Box::new(Expr::undefined()),
                    },
                    Kind::Any,
                ));
            }
            // `console.log(…)` as a value: `undefined`.
            Call::Console => {
                return Ok(Val::js(
                    Expr::Seq {
                        body: vec![Stmt::Expr(call)],
                        value: Box::new(Expr::undefined()),
                    },
                    Kind::Any,
                ));
            }
        };
        match self.module.function(func).result {
            Some(Type::F64) => Ok(Val {
                e: call,
                ty: Type::F64,
                kind: self.slot_kind(Slot::Result(func)),
            }),
            Some(ty) => {
                let kind = self.slot_kind(Slot::Result(func));
                Ok(Val { e: call, ty, kind })
            }
            None => {
                // Its `undefined` result is used, so the function gets a result.
                self.observe(Slot::Result(func), Kind::Any);
                Ok(self.undefined_val())
            }
        }
    }

    /// A call of a known function with its typed signature. `self_obj`: its function object, if it
    /// takes one. `this` is `undefined`; missing arguments are `undefined`, extra ones are evaluated
    /// (after the others) and dropped (F16).
    fn direct_call(
        &mut self,
        cx: &mut FnCx,
        func: FuncId,
        self_obj: Option<Expr>,
        arguments: &'a [Argument<'a>],
    ) -> R<(Expr, Call)> {
        let params = self.func_params[&func].clone();
        let hidden = self.hidden[&func];
        let mut args: Vec<Expr> = self_obj.into_iter().collect();
        if hidden.this {
            args.push(Expr::undefined());
        }
        if hidden.new_target {
            args.push(Expr::undefined());
        }
        let mut extra = Vec::new();
        let rest = self.rest_param.contains(&func);
        let mut rest_values = Vec::new();
        let mut result = Ok(());
        for (i, a) in arguments.iter().enumerate() {
            let Some(e) = a.as_expression() else {
                unreachable!("calls with spread arguments go through `spread_call`");
            };
            let v = match self.expr(cx, e) {
                Ok(v) => v,
                Err(e) => {
                    result = Err(e);
                    continue;
                }
            };
            match params.get(i) {
                // A typed parameter.
                Some(Some(sym)) => {
                    let ty = self.module.function(func).locals[hidden.count() as usize + i].ty;
                    args.push(self.store(Slot::Var(*sym), ty, v));
                }
                // A default or a pattern: bound in the callee.
                Some(None) => args.push(jsval_of(v)),
                None if rest => rest_values.push(Some(jsval_of(v))),
                None => extra.push(Stmt::Expr(v.e)),
            }
        }
        result?;
        for (i, sym) in params.iter().enumerate().skip(arguments.len()) {
            // A missing argument is `undefined`.
            let v = self.undefined_val();
            match sym {
                Some(sym) => {
                    let ty = self.module.function(func).locals[hidden.count() as usize + i].ty;
                    args.push(self.store(Slot::Var(*sym), ty, v));
                }
                None => args.push(v.e),
            }
        }
        if rest {
            args.push(Expr::Array(rest_values));
        }
        if extra.is_empty() {
            return Ok((Expr::Call { func, args }, Call::Direct(func)));
        }
        if arguments.len() > params.len() && !params.is_empty() {
            // Evaluate the last real argument, then the extra ones.
            let last = args.pop().expect("a parameter");
            let ty = self.module.function(func).locals[args.len()].ty;
            let t = cx.func.add_local("arg", ty);
            let mut body = vec![Stmt::Set(t, last)];
            body.extend(extra);
            args.push(Expr::Seq {
                body,
                value: Box::new(Expr::Local(t)),
            });
            return Ok((Expr::Call { func, args }, Call::Direct(func)));
        }
        // No parameters: the extra arguments come first (the hidden ones are plain reads).
        Ok((
            Expr::Seq {
                body: extra,
                value: Box::new(Expr::Call { func, args }),
            },
            Call::Direct(func),
        ))
    }

    /// The arguments of a call through a value, as JS values.
    fn value_args(&mut self, cx: &mut FnCx, arguments: &'a [Argument<'a>]) -> R<Vec<Expr>> {
        let mut out = Vec::new();
        let mut result = Ok(());
        for a in arguments {
            let Some(e) = a.as_expression() else {
                result = Err(self.unsupported(a.span(), "spread argument", "T15"));
                continue;
            };
            match self.expr(cx, e) {
                Ok(v) => out.push(jsval_of(v)),
                Err(e) => result = Err(e),
            }
        }
        result.map(|_| out)
    }

    /// How node names a callee in "… is not a function": `o.a.b`, `f(...)`, `(1 , o.a)`.
    fn callee_text(&self, e: &Expression<'a>) -> String {
        if let Some(m) = e.as_member_expression() {
            return self.member_text(m);
        }
        match e {
            Expression::Identifier(id) => id.name.to_string(),
            Expression::ThisExpression(_) => "this".to_string(),
            Expression::FunctionExpression(_)
            | Expression::ArrowFunctionExpression(_)
            | Expression::ClassExpression(_) => "(intermediate value)".to_string(),
            // V8's CallPrinter: an object literal is `{` + `(intermediate value)` per property + `}`.
            Expression::ObjectExpression(o) => {
                format!("{{{}}}", "(intermediate value)".repeat(o.properties.len()))
            }
            Expression::CallExpression(c) => format!("{}(...)", self.callee_text(&c.callee)),
            Expression::ChainExpression(c) => match &c.expression {
                ChainElement::CallExpression(c) => {
                    format!("{}(...)", self.callee_text(&c.callee))
                }
                el => match el.as_member_expression() {
                    Some(m) => self.member_text(m),
                    None => "expression".to_string(),
                },
            },
            Expression::ParenthesizedExpression(p) => match &p.expression {
                Expression::SequenceExpression(sq) => format!(
                    "({})",
                    sq.expressions
                        .iter()
                        .map(|x| self.callee_text(x))
                        .collect::<Vec<_>>()
                        .join(" , ")
                ),
                x => self.callee_text(x),
            },
            other => {
                let sp = other.span();
                self.p.source[sp.start as usize..sp.end as usize].to_string()
            }
        }
    }

    fn member_text(&self, m: &MemberExpression<'a>) -> String {
        let dot = if m.optional() { "?." } else { "." };
        match m {
            MemberExpression::StaticMemberExpression(sm) => {
                format!("{}{dot}{}", self.callee_text(&sm.object), sm.property.name)
            }
            MemberExpression::ComputedMemberExpression(cm) => match &cm.expression {
                Expression::StringLiteral(k) => {
                    format!("{}{dot}{}", self.callee_text(&cm.object), k.value)
                }
                _ => {
                    let sp = cm.span;
                    self.p.source[sp.start as usize..sp.end as usize].to_string()
                }
            },
            MemberExpression::PrivateFieldExpression(p) => {
                let sp = p.span;
                self.p.source[sp.start as usize..sp.end as usize].to_string()
            }
        }
    }

    /// A built-in global function that is a value (T17): `Promise`, `queueMicrotask`; the timer
    /// functions (T19).
    fn global_function(&self, id: &IdentifierReference<'a>) -> Option<Intrinsic> {
        if self
            .scoping
            .get_reference(id.reference_id())
            .symbol_id()
            .is_some()
        {
            return None;
        }
        match id.name.as_str() {
            "Promise" => Some(Intrinsic::PromiseCtor),
            "queueMicrotask" => Some(Intrinsic::QueueMicrotask),
            name => ir::TIMER_NAMES
                .iter()
                .position(|n| *n == name && *n != "nextTick")
                .map(|k| Intrinsic::Timer(k as u8)),
        }
    }

    /// A timer function as a value (T19; T25: immediates, `process.nextTick`): commands, and
    /// libraries with an async export (T20: timers run while an async call waits).
    /// Notes the kinds used for the backend (`$timers`, `$immediates`, `$ticks` in `names`).
    fn timer_value(&mut self, span: Span, k: u8) -> R<Val> {
        let name = ir::TIMER_NAMES[k as usize];
        let async_export = self
            .exported
            .iter()
            .any(|f| self.module.function(*f).flags.is_async);
        if !self.command && !async_export {
            return Err(self.unsupported(
                span,
                &format!(
                    "`{name}` in a library without async exports (timers run while an async export's call waits)"
                ),
                "T20",
            ));
        }
        let what = match k {
            ir::TIMER_SET_IMMEDIATE | ir::TIMER_CLEAR_IMMEDIATE => "$immediates",
            ir::TIMER_NEXT_TICK => "$ticks",
            _ => "$timers",
        };
        self.module.names.insert(what.to_string());
        Ok(Val::js(
            Expr::intrinsic(Intrinsic::Timer(k), vec![]),
            Kind::Any,
        ))
    }

    /// The native error constructor `callee` names, if it's one (and not shadowed).
    fn error_ctor(&self, callee: &Expression<'a>) -> Option<ErrorKind> {
        let Expression::Identifier(id) = callee else {
            return None;
        };
        let global = self
            .scoping
            .get_reference(id.reference_id())
            .symbol_id()
            .is_none();
        global.then(|| ErrorKind::parse(id.name.as_str())).flatten()
    }

    /// `AggregateError` in the code: `console.log` shows the `errors` of errors (T25).
    fn note_error_kind(&mut self, k: ErrorKind) {
        if k == ErrorKind::AggregateError {
            self.module.names.insert(k.name().to_string());
        }
    }

    /// `new E(message)` or `E(message)` (the same for the native errors).
    fn new_error(
        &mut self,
        cx: &mut FnCx,
        k: ErrorKind,
        args: &'a [Argument<'a>],
        span: Span,
    ) -> R<Val> {
        let _ = span;
        self.note_error_kind(k);
        let n = if matches!(k, ErrorKind::AggregateError | ErrorKind::SuppressedError) { 3 } else { 2 };
        let args = self.args(cx, args, n)?;
        Ok(Val::js(
            Expr::intrinsic(Intrinsic::NewError(k), args),
            Kind::Any,
        ))
    }

    /// `console.log`/`info`/`debug` (`Some(false)`: stdout) or `console.error`/`warn`
    /// (`Some(true)`: stderr), when `console` is the global.
    fn console_method(&self, callee: &Expression<'a>) -> Option<bool> {
        let Expression::StaticMemberExpression(m) = callee else {
            return None;
        };
        let Expression::Identifier(obj) = &m.object else {
            return None;
        };
        if obj.name != "console"
            || m.optional
            || self
                .scoping
                .get_reference(obj.reference_id())
                .symbol_id()
                .is_some()
        {
            return None;
        }
        match m.property.name.as_str() {
            "log" | "info" | "debug" => Some(false),
            "error" | "warn" => Some(true),
            _ => None,
        }
    }

    /// `console.log(a, b, …)` on primitives: each argument formatted (`-0` prints as `-0`),
    /// joined by spaces, plus a newline. With several arguments a string first argument is a
    /// format string (`%s`, `%d`, `%%`…), which isn't implemented: rejected for a literal with
    /// `%`, a trap at run time for other strings containing `%`.
    fn console(&mut self, cx: &mut FnCx, c: &'a CallExpression<'a>, stderr: bool) -> R<Expr> {
        if !self.command {
            return Err(self.unsupported(
                c.span,
                "`console` in a module with exports (a library can't write to stdout yet)",
                "T20",
            ));
        }
        let several = c.arguments.len() > 1;
        // `$$consoleFormat`, linked when a string in the code has a `%` (T25).
        let format = self
            .prelude
            .names
            .get("$$consoleFormat")
            .filter(|_| self.prelude.is_live("$$consoleFormat"))
            .and_then(|sym| self.funcs.get(sym))
            .copied();
        let mut first_format = false;
        let mut parts = Vec::new();
        // The arguments as JS values (for `$$consoleFormat`).
        let mut raw = Vec::new();
        // Every argument is evaluated before any is formatted (`console.log(a, a.pop())`).
        let mut pre = Vec::new();
        let mut result = Ok(());
        for (i, a) in c.arguments.iter().enumerate() {
            let Some(e) = a.as_expression() else {
                result = Err(self.unsupported(a.span(), "spread argument", "T15"));
                continue;
            };
            let mut v = match self.expr(cx, e) {
                Ok(v) => v,
                Err(e) => {
                    result = Err(e);
                    continue;
                }
            };
            if several && !matches!(v.e, Expr::Const(_) | Expr::Str(_)) {
                let t = cx.func.add_local("arg", v.ty);
                pre.push(Stmt::Set(t, v.e));
                v.e = Expr::Local(t);
            }
            if format.is_some() {
                raw.push(jsval_of(Val {
                    e: v.e.clone(),
                    ty: v.ty,
                    kind: v.kind,
                }));
            }
            if i == 0 && several && matches!(v.kind, Kind::Str | Kind::Any) && format.is_some() {
                // A possible format string (T25): the whole line from `$$consoleFormat`.
                first_format = true;
                parts.push(jsval_of(v));
                continue;
            }
            if i == 0 && several && matches!(v.kind, Kind::Str | Kind::Any) {
                match static_string(e) {
                    Some(text) if text.contains('%') => {
                        result = Err(self.unsupported(
                            e.span(),
                            "`console.log` format string (a first argument containing `%`, followed by more arguments)",
                            "T21",
                        ));
                        continue;
                    }
                    Some(_) => parts.push(v.e),
                    None => parts.push(Expr::intrinsic(
                        Intrinsic::Inspect,
                        vec![Expr::intrinsic(Intrinsic::NoFormat, vec![jsval_of(v)])],
                    )),
                }
                continue;
            }
            parts.push(match v.kind {
                Kind::Str => v.e,
                _ => Expr::intrinsic(Intrinsic::Inspect, vec![jsval_of(v)]),
            });
        }
        result?;
        if let Some(func) = format.filter(|_| first_format) {
            // Every argument as a JS value, formatted by the prelude.
            for p in self.func_params[&func].clone().into_iter().flatten() {
                self.observe(Slot::Var(p), Kind::Any);
            }
            let line = Expr::Call {
                func,
                args: vec![Expr::Array(raw.into_iter().map(Some).collect())],
            };
            parts = vec![Expr::intrinsic(Intrinsic::ToString, vec![line])];
        }
        let mut line = Vec::new();
        for (i, p) in parts.into_iter().enumerate() {
            if i > 0 {
                line.push(Expr::str(" "));
            }
            line.push(p);
        }
        line.push(Expr::str("\n"));
        let text = line
            .into_iter()
            .reduce(|a, b| Expr::intrinsic(Intrinsic::Concat, vec![a, b]))
            .expect("the newline");
        let write = Expr::intrinsic(Intrinsic::ConsoleWrite { stderr }, vec![text]);
        Ok(if pre.is_empty() {
            write
        } else {
            Expr::Seq {
                body: pre,
                value: Box::new(write),
            }
        })
    }

    // ---- objects (T13) -------------------------------------------------------------------

    /// The built-in a global identifier names (`Object`, `Error`, …), if `e` is one.
    fn builtin_ident(&self, e: &Expression<'a>) -> Option<Builtin> {
        let Expression::Identifier(id) = e.without_parentheses() else {
            return None;
        };
        let global = self
            .scoping
            .get_reference(id.reference_id())
            .symbol_id()
            .is_none();
        global.then(|| Builtin::parse(id.name.as_str())).flatten()
    }

    /// Is `e` the undeclared global `name`?
    fn is_global(&self, e: &Expression<'a>, name: &str) -> bool {
        matches!(e.without_parentheses(), Expression::Identifier(id)
            if id.name == name
                && self.scoping.get_reference(id.reference_id()).symbol_id().is_none())
    }

    /// `A.b` (`A` an undeclared global) implemented by a prelude function (`$$A_b`, T21).
    fn builtin_static(&self, e: &Expression<'a>) -> Option<FuncId> {
        let Expression::StaticMemberExpression(m) = e.without_parentheses() else {
            return None;
        };
        if m.optional {
            return None;
        }
        let name = builtins::static_name(self.p, &m.object, &m.property.name)?;
        self.funcs.get(self.prelude.names.get(&name)?).copied()
    }

    /// `A.b` read as a value: `$$A_b$value` when calls of `A.b` are intrinsics (T28), else
    /// [`Self::builtin_static`].
    fn builtin_static_value(&self, e: &Expression<'a>) -> Option<FuncId> {
        if let Expression::StaticMemberExpression(m) = e.without_parentheses()
            && !m.optional
            && let Some(name) = builtins::static_name(self.p, &m.object, &m.property.name)
            && let Some(sym) = self.prelude.names.get(&format!("{name}{}", builtins::VALUE))
        {
            return self.funcs.get(sym).copied();
        }
        self.builtin_static(e)
    }

    /// A global function implemented by the prelude (`parseInt` → `$$parseInt`, T21).
    fn builtin_global(&self, e: &Expression<'a>) -> Option<FuncId> {
        let Expression::Identifier(id) = e.without_parentheses() else {
            return None;
        };
        if self
            .scoping
            .get_reference(id.reference_id())
            .symbol_id()
            .is_some()
        {
            return None;
        }
        // `Date(…)` without `new` gives a string (T26): `$$DateCall`.
        let name = if id.name == "Date" { "$$DateCall".to_string() } else { format!("$${}", id.name) };
        let sym = self.prelude.names.get(&name)?;
        self.funcs.get(sym).copied()
    }

    /// The user's code names a prelude declaration: already reported (see `run`).
    fn prelude_leak(&self, id: &IdentifierReference<'a>) -> bool {
        !self.p.in_prelude(id.span.start)
            && !self.p.is_generated(id.span.start)
            && self
                .scoping
                .get_reference(id.reference_id())
                .symbol_id()
                .is_some_and(|s| self.p.in_prelude(self.scoping.symbol_span(s).start))
    }

    /// Reject a built-in the module can't use (reported where the user's code names it).
    pub(crate) fn check_builtin(&mut self, span: Span, func: FuncId) -> R<()> {
        let name = &self.module.function(func).name;
        if name == "$$Math_random" && !self.command {
            // It imports `wasi:random`: commands only, like the timers.
            return Err(self.unsupported(span, "`Math.random` in a module with exports", "T20"));
        }
        Ok(())
    }

    /// `__intrinsic_map_<op>(…)` (T25): `Map`/`Set` storage. JS-value arguments, then `i32`s.
    fn map_intrinsic(
        &mut self,
        cx: &mut FnCx,
        c: &'a CallExpression<'a>,
        name: &str,
        i: Intrinsic,
    ) -> R<(Expr, Call)> {
        let (params, result) = i.signature();
        if c.arguments.len() != params.len() {
            return Err(self.unsupported(c.span, &format!("intrinsic `{name}` arity"), "T25"));
        }
        let mut args = Vec::new();
        for (a, ty) in c.arguments.iter().zip(params) {
            let Some(e) = a.as_expression() else {
                return Err(self.unsupported(a.span(), "spread argument", "T25"));
            };
            let v = self.expr(cx, e)?;
            args.push(match ty {
                Type::JsVal => jsval_of(v),
                _ => to_i32(self.num(v, e.span())?),
            });
        }
        if matches!(i, Intrinsic::Map(ir::MapOp::New(_))) {
            self.module.maps = true;
        }
        if matches!(
            i,
            Intrinsic::Typed(ir::TypedOp::New | ir::TypedOp::View | ir::TypedOp::BufferNew)
        ) {
            // The backend links typed arrays' element access and printing (T20).
            self.module.names.insert("$typed".to_string());
        }
        let e = Expr::intrinsic(i, args);
        Ok(match result {
            None => (e, Call::Intrinsic(None)),
            Some(Type::I32) => (from_i32(e), Call::Intrinsic(Some(Kind::Num))),
            Some(Type::Bool) => (
                jsval_of(Val::new(e, Type::Bool)),
                Call::Intrinsic(Some(Kind::Bool)),
            ),
            Some(_) => (e, Call::Intrinsic(Some(Kind::Any))),
        })
    }

    /// `__intrinsic_<name>(…)` in the prelude (T21): an IR operation on numbers.
    fn intrinsic_call(
        &mut self,
        cx: &mut FnCx,
        c: &'a CallExpression<'a>,
        name: &str,
    ) -> R<(Expr, Call)> {
        if let Some(op) = map_op(name) {
            return self.map_intrinsic(cx, c, name, Intrinsic::Map(op));
        }
        if let Some(op) = typed_op(name) {
            return self.map_intrinsic(cx, c, name, Intrinsic::Typed(op));
        }
        if let Some(op) = stream_op(name) {
            return self.map_intrinsic(cx, c, name, Intrinsic::Stream(op));
        }
        if name == "named" {
            // Does the user's code name this property (a string literal argument)? A constant
            // (T35): the prelude skips what can't happen, e.g. species without `species`.
            let Some(Expression::StringLiteral(sl)) = c.arguments.first().and_then(|a| a.as_expression())
            else {
                return Err(self.unsupported(c.span, "intrinsic `named` argument", "T35"));
            };
            let named = self.prelude.user_props.contains(sl.value.as_str());
            return Ok((
                jsval_of(Val::new(Expr::bool(named), Type::Bool)),
                Call::Intrinsic(Some(Kind::Bool)),
            ));
        }
        if let Some((op, n)) = match name {
            "bigint_from_number" => Some((Intrinsic::BigIntFromNumber, 1)),
            "bigint_to_number" => Some((Intrinsic::BigIntToNumber, 1)),
            "bigint_to_string" => Some((Intrinsic::BigIntToString, 2)),
            _ => None,
        } {
            // T40: `(x: number) -> bigint`, `(b: bigint) -> number`, `(b, radix) -> string`.
            if c.arguments.len() != n {
                return Err(self.unsupported(c.span, &format!("intrinsic `{name}` arity"), "T40"));
            }
            let mut args = Vec::new();
            for (i, a) in c.arguments.iter().enumerate() {
                let Some(e) = a.as_expression() else {
                    return Err(self.unsupported(a.span(), "spread argument", "T40"));
                };
                let v = self.expr(cx, e)?;
                args.push(match (op, i) {
                    (Intrinsic::BigIntFromNumber, _) => self.num(v, e.span())?,
                    (Intrinsic::BigIntToString, 1) => to_i32(self.num(v, e.span())?),
                    _ => jsval_of(v),
                });
            }
            let e = Expr::intrinsic(op, args);
            return Ok(match op {
                Intrinsic::BigIntToNumber => (e, Call::Intrinsic(Some(Kind::Num))),
                Intrinsic::BigIntToString => (e, Call::Intrinsic(Some(Kind::Str))),
                _ => (e, Call::Intrinsic(Some(Kind::Any))),
            });
        }
        let arity = match name {
            "abs" | "sqrt" | "floor" | "ceil" | "trunc" | "fround" | "clz32" | "hi" | "lo" => 1,
            "min" | "max" | "copysign" | "imul" | "from_words" => 2,
            "random_seed" => 0,
            "to_string" | "str_new" | "number_to_string" | "str_len" => 1,
            "trap" => 0,
            "builtin_tag" | "builtin_to_string" | "to_object" | "unwrap" | "is_constructor" => 1,
            "regexp_new" | "regexp_data" | "regexp_builtin_exec" => 1,
            "date_new" | "date_data" => 1,
            "stack_new" | "async_stack_new" | "stack_data" => 1,
            "date_now" | "module_new" | "module_bodies" => 0,
            "to_primitive" => 2,
            "to_key" | "object_desc" => 1,
            "set_proto" => 2,
            "call_list" | "construct_list" => 3,
            "inspect" => 2,
            "unit" | "i32_get" => 2,
            "sub" | "str_set" | "define" | "i32_set" => 3,
            "global_init" => 1,
            "i32_new" | "i32_len" => 1,
            _ => return Err(self.unsupported(c.span, &format!("intrinsic `{name}`"), "T21")),
        };
        if c.arguments.len() != arity {
            return Err(self.unsupported(c.span, &format!("intrinsic `{name}` arity"), "T21"));
        }
        if name == "random_seed" {
            // An import (`wasi:random`): commands only. `check_builtin` rejects `Math.random` where
            // the user's code names it; reached another way in a library (`const M = Math;
            // M.random()`), it throws: a bare `Err(Unsupported)` here would drop the statement
            // without a diagnostic.
            let e = if self.command {
                from_i32(Expr::intrinsic(Intrinsic::RandomSeed, vec![]))
            } else {
                library_throws("random numbers (`Math.random`) aren't available in a module with exports yet (js2wasi3)")
            };
            return Ok((e, Call::Intrinsic(Some(Kind::Num))));
        }
        if name == "module_bodies" {
            // T41 slice 3: the modules' functions (lowered before `main`), as function objects.
            let bodies = self.module_bodies.clone();
            let items = bodies
                .into_iter()
                .map(|f| {
                    let code = self.adapter(f);
                    let kind = if self.module.function(f).flags.is_async {
                        ir::FnKind::Async
                    } else {
                        ir::FnKind::Plain
                    };
                    Some(Expr::Closure(Box::new(ir::Closure {
                        code,
                        name: String::new(),
                        length: 0,
                        kind,
                        env: Vec::new(),
                        shared: false,
                        source: None,
                    })))
                })
                .collect();
            return Ok((Expr::Array(items), Call::Intrinsic(Some(Kind::Any))));
        }
        if name == "module_new" {
            // T41: `console.log`, descriptors and writes recognize namespaces only then.
            self.module.names.insert("$module".to_string());
            return Ok((Expr::intrinsic(Intrinsic::ModuleNew, vec![]), Call::Intrinsic(Some(Kind::Any))));
        }
        if name == "date_now" {
            // An import (`wasi:clocks/system-clock`, T26): commands only; in a library, reading the
            // clock throws (the rest of `Date` works).
            let e = if self.command {
                Expr::intrinsic(Intrinsic::DateNow, vec![])
            } else {
                library_throws("the current time (the system clock) isn't available in a module with exports yet (js2wasi3)")
            };
            return Ok((e, Call::Intrinsic(Some(Kind::Num))));
        }
        if name == "to_primitive" {
            // ToPrimitive(v, hint) (T26): hint 0 default, 1 number, 2 string.
            let (Some(v), Some(h)) = (c.arguments[0].as_expression(), c.arguments[1].as_expression())
            else {
                return Err(self.unsupported(c.span, "spread argument", "T21"));
            };
            let v = jsval_of(self.expr(cx, v)?);
            let hv = self.expr(cx, h)?;
            let h = to_i32(self.num(hv, h.span())?);
            let e = Expr::intrinsic(Intrinsic::ToPrimitive, vec![v, h]);
            return Ok((e, Call::Intrinsic(Some(Kind::Any))));
        }
        if name == "inspect" {
            // `util.inspect(v)` from a literal depth (T25; -1: `console.log`'s own).
            let Some(e) = c.arguments[0].as_expression() else {
                return Err(self.unsupported(c.span, "spread argument", "T21"));
            };
            let v = jsval_of(self.expr(cx, e)?);
            let d = match c.arguments[1].as_expression() {
                Some(Expression::NumericLiteral(n)) => Some(n.value as u8),
                Some(Expression::UnaryExpression(u))
                    if u.operator == UnaryOperator::UnaryNegation =>
                {
                    None
                }
                _ => return Err(self.unsupported(c.span, "inspect depth", "T21")),
            };
            let e = match d {
                Some(d) => Expr::intrinsic(Intrinsic::InspectAt(d), vec![v]),
                None => Expr::intrinsic(Intrinsic::Inspect, vec![v]),
            };
            return Ok((e, Call::Intrinsic(Some(Kind::Str))));
        }
        if let Some(op) = match name {
            "call_list" => Some(Intrinsic::CallList),
            "construct_list" => Some(Intrinsic::ConstructList),
            "set_proto" => Some(Intrinsic::SetProto),
            "to_key" => Some(Intrinsic::ToPropertyKey),
            "object_desc" => Some(Intrinsic::ObjectDesc),
            _ => None,
        } {
            // `Reflect` (T36): `call_list(f, this, args)`, `construct_list(f, args, new_target)`
            // (the prelude checked `f`, so no "not a function" text), `set_proto(o, p)`,
            // `to_key(v)` (ToPropertyKey), `object_desc(o)` (`#<Foo>`).
            let mut args = Vec::new();
            for a in &c.arguments {
                let Some(e) = a.as_expression() else {
                    return Err(self.unsupported(a.span(), "spread argument", "T21"));
                };
                args.push(jsval_of(self.expr(cx, e)?));
            }
            if matches!(op, Intrinsic::CallList | Intrinsic::ConstructList) {
                args.push(Expr::str(""));
            }
            let kind = op.signature().1.map(|_| Kind::Any);
            return Ok((Expr::intrinsic(op, args), Call::Intrinsic(kind)));
        }
        if name == "is_constructor" {
            // IsConstructor (T35).
            let Some(e) = c.arguments[0].as_expression() else {
                return Err(self.unsupported(c.span, "spread argument", "T21"));
            };
            let v = jsval_of(self.expr(cx, e)?);
            let e = jsval_of(Val::new(
                Expr::intrinsic(Intrinsic::IsConstructor, vec![v]),
                Type::Bool,
            ));
            return Ok((e, Call::Intrinsic(Some(Kind::Bool))));
        }
        if name == "regexp_builtin_exec" {
            // T27: is it the built-in `RegExp.prototype.exec`?
            let Some(e) = c.arguments[0].as_expression() else {
                return Err(self.unsupported(c.span, "spread argument", "T21"));
            };
            let v = jsval_of(self.expr(cx, e)?);
            let e = jsval_of(Val::new(
                Expr::intrinsic(Intrinsic::RegExpBuiltinExec, vec![v]),
                Type::Bool,
            ));
            return Ok((e, Call::Intrinsic(Some(Kind::Bool))));
        }
        if name == "builtin_to_string" {
            let Some(e) = c.arguments[0].as_expression() else {
                return Err(self.unsupported(c.span, "spread argument", "T21"));
            };
            let v = jsval_of(self.expr(cx, e)?);
            let e = jsval_of(Val::new(
                Expr::intrinsic(Intrinsic::BuiltinToString, vec![v]),
                Type::Bool,
            ));
            return Ok((e, Call::Intrinsic(Some(Kind::Bool))));
        }
        if matches!(
            name,
            "to_object"
                | "unwrap"
                | "regexp_new"
                | "regexp_data"
                | "date_new"
                | "date_data"
                | "stack_new"
                | "async_stack_new"
                | "stack_data"
        ) {
            // T30: wrapper objects; T27: RegExp objects and their data.
            let Some(e) = c.arguments[0].as_expression() else {
                return Err(self.unsupported(c.span, "spread argument", "T21"));
            };
            let v = jsval_of(self.expr(cx, e)?);
            let op = match name {
                "to_object" => Intrinsic::ToObject,
                "unwrap" => Intrinsic::Unwrap,
                "regexp_new" => {
                    // `console.log` recognizes RegExps only then.
                    self.module.names.insert("$regexp".to_string());
                    Intrinsic::RegExpNew
                }
                "date_new" => {
                    // T26: `console.log`, ToPrimitive and `Object.prototype.toString` recognize Dates
                    // only then.
                    self.module.names.insert("$date".to_string());
                    Intrinsic::DateNew
                }
                "date_data" => Intrinsic::DateData,
                // T45: `DisposableStack`/`AsyncDisposableStack` objects.
                "stack_new" => Intrinsic::StackNew(false),
                "async_stack_new" => Intrinsic::StackNew(true),
                "stack_data" => Intrinsic::StackData,
                _ => Intrinsic::RegExpData,
            };
            return Ok((Expr::intrinsic(op, vec![v]), Call::Intrinsic(Some(Kind::Any))));
        }
        if name == "builtin_tag" {
            // An object's `Symbol.toStringTag` (T25).
            let Some(e) = c.arguments[0].as_expression() else {
                return Err(self.unsupported(c.span, "spread argument", "T21"));
            };
            let v = jsval_of(self.expr(cx, e)?);
            let e = Expr::intrinsic(Intrinsic::BuiltinTag, vec![v]);
            return Ok((e, Call::Intrinsic(Some(Kind::Any))));
        }
        if name == "global_init" {
            // The global object's built-in properties (T42): non-enumerable data properties,
            // except node's own globals (timers, `global`), which node makes enumerable.
            let Some(e) = c.arguments[0].as_expression() else {
                return Err(self.unsupported(c.span, "spread argument", "T21"));
            };
            let g = cx.func.add_local("global", Type::JsVal);
            let mut body = vec![Stmt::Set(g, jsval_of(self.expr(cx, e)?))];
            for name in self.prelude.global_props.clone() {
                let Some(v) = self.global_value(&name, &Expr::Local(g)) else {
                    continue;
                };
                let enumerable = name == "global"
                    || name == "queueMicrotask"
                    || ir::TIMER_NAMES.contains(&name.as_str());
                let op = if enumerable {
                    Intrinsic::DefineProp
                } else {
                    Intrinsic::DefineMethod
                };
                body.push(Stmt::Expr(Expr::intrinsic(op, vec![Expr::Local(g), Expr::str(&name), v])));
            }
            let e = Expr::Seq {
                body,
                value: Box::new(Expr::undefined()),
            };
            return Ok((e, Call::Intrinsic(Some(Kind::Any))));
        }
        if name == "define" {
            // CreateDataProperty(o, k, v) on an object.
            let mut args = Vec::new();
            for a in &c.arguments {
                let Some(e) = a.as_expression() else {
                    return Err(self.unsupported(a.span(), "spread argument", "T21"));
                };
                args.push(jsval_of(self.expr(cx, e)?));
            }
            let e = Expr::intrinsic(Intrinsic::DefineProp, args);
            return Ok((e, Call::Intrinsic(None)));
        }
        // String intrinsics: the first argument is a JS value (a string), the rest numbers.
        if name == "trap" {
            let e = Expr::intrinsic(Intrinsic::Trap, vec![]);
            return Ok((e, Call::Intrinsic(None)));
        }
        let strings = matches!(
            name,
            "to_string" | "unit" | "sub" | "str_set" | "str_len" | "i32_get" | "i32_set" | "i32_len"
        );
        let mut args = Vec::new();
        for (i, a) in c.arguments.iter().enumerate() {
            let Some(e) = a.as_expression() else {
                return Err(self.unsupported(a.span(), "spread argument", "T21"));
            };
            let v = self.expr(cx, e)?;
            if strings && i == 0 {
                args.push(jsval_of(v));
            } else {
                args.push(self.num(v, e.span())?);
            }
        }
        let str_op = |op, args: Vec<Expr>| {
            let mut args = args.into_iter();
            let mut out = vec![];
            if strings {
                out.push(args.next().expect("arity checked"));
            }
            out.extend(args.map(to_i32));
            Expr::intrinsic(op, out)
        };
        let str_kind = Call::Intrinsic(Some(Kind::Str));
        match name {
            "to_string" => {
                return Ok((str_op(Intrinsic::ToString, args), str_kind));
            }
            "number_to_string" => {
                let e = Expr::intrinsic(Intrinsic::NumberToString, args);
                return Ok((e, str_kind));
            }
            "str_new" => return Ok((str_op(Intrinsic::StrNew, args), str_kind)),
            // T27: `$i32s` arrays (the first argument is one, the rest numbers).
            "i32_new" => {
                let e = str_op(Intrinsic::I32New, args);
                return Ok((e, Call::Intrinsic(Some(Kind::Any))));
            }
            "i32_set" => return Ok((str_op(Intrinsic::I32Set, args), Call::Intrinsic(None))),
            "i32_get" | "i32_len" => {
                let op = if name == "i32_get" {
                    Intrinsic::I32Get
                } else {
                    Intrinsic::I32Len
                };
                let e = from_i32(str_op(op, args));
                return Ok((e, Call::Intrinsic(Some(Kind::Num))));
            }
            "sub" => return Ok((str_op(Intrinsic::StrSub, args), str_kind)),
            "str_set" => return Ok((str_op(Intrinsic::StrSet, args), Call::Intrinsic(None))),
            "str_len" => {
                let e = Expr::intrinsic(Intrinsic::StringLength, args);
                return Ok((e, Call::Intrinsic(Some(Kind::Num))));
            }
            "unit" => {
                let e = from_i32(str_op(Intrinsic::StrUnit, args));
                return Ok((e, Call::Intrinsic(Some(Kind::Num))));
            }
            _ => {}
        }
        let mut args = args.into_iter();
        let mut arg = || args.next().expect("arity checked");
        let mut f64_un = |op| Expr::unary(op, Type::F64, arg());
        let e = match name {
            "abs" => f64_un(UnOp::Abs),
            "sqrt" => f64_un(UnOp::Sqrt),
            "floor" => f64_un(UnOp::Floor),
            "ceil" => f64_un(UnOp::Ceil),
            "trunc" => f64_un(UnOp::Trunc),
            "fround" => f64_un(UnOp::Fround),
            "clz32" => from_i32(Expr::unary(UnOp::Clz, Type::I32, to_i32(arg()))),
            "hi" => from_i32(Expr::intrinsic(Intrinsic::F64Hi, vec![arg()])),
            "lo" => from_i32(Expr::intrinsic(Intrinsic::F64Lo, vec![arg()])),
            "min" | "max" | "copysign" => {
                let op = match name {
                    "min" => BinOp::Min,
                    "max" => BinOp::Max,
                    _ => BinOp::CopySign,
                };
                let a = arg();
                Expr::binary(op, Type::F64, a, arg())
            }
            "imul" => {
                let a = to_i32(arg());
                from_i32(Expr::binary(BinOp::Mul, Type::I32, a, to_i32(arg())))
            }
            _ => {
                let hi = to_i32(arg());
                Expr::intrinsic(Intrinsic::F64FromWords, vec![hi, to_i32(arg())])
            }
        };
        Ok((e, Call::Intrinsic(Some(Kind::Num))))
    }

    /// `B.prototype` for a built-in constructor `B`.
    fn builtin_prototype(&self, m: &MemberExpression<'a>) -> Option<Builtin> {
        let MemberExpression::StaticMemberExpression(sm) = m else {
            return None;
        };
        if sm.property.name != "prototype" || sm.optional {
            return None;
        }
        self.builtin_ident(&sm.object)
    }

    /// `Object.create` / `Object.getPrototypeOf` / `Object.hasOwn` as a callee: the method.
    fn object_static(&self, callee: &Expression<'a>) -> Option<Intrinsic> {
        let Expression::StaticMemberExpression(m) = callee else {
            return None;
        };
        if m.optional {
            return None;
        }
        match (self.builtin_ident(&m.object)?, m.property.name.as_str()) {
            (Builtin::Object, "create") => Some(Intrinsic::ObjectCreate),
            (Builtin::Object, "getPrototypeOf") => Some(Intrinsic::GetPrototypeOf),
            (Builtin::Object, "getOwnPropertySymbols") => Some(Intrinsic::GetOwnPropertySymbols),
            (Builtin::Object, "hasOwn") => Some(Intrinsic::HasOwn),
            (Builtin::Object, "keys") => Some(Intrinsic::ObjectKeys),
            (Builtin::Object, "values") => Some(Intrinsic::ObjectValues),
            (Builtin::Object, "entries") => Some(Intrinsic::ObjectEntries),
            (Builtin::Object, "getOwnPropertyNames") => Some(Intrinsic::GetOwnPropertyNames),
            (Builtin::Object, "defineProperty") => Some(Intrinsic::DefineProperty),
            (Builtin::Object, "getOwnPropertyDescriptor") => {
                Some(Intrinsic::GetOwnPropertyDescriptor)
            }
            (Builtin::Object, "preventExtensions") => Some(Intrinsic::Integrity(0)),
            (Builtin::Object, "seal") => Some(Intrinsic::Integrity(1)),
            (Builtin::Object, "freeze") => Some(Intrinsic::Integrity(2)),
            (Builtin::Object, "isExtensible") => Some(Intrinsic::TestIntegrity(0)),
            (Builtin::Object, "isSealed") => Some(Intrinsic::TestIntegrity(1)),
            (Builtin::Object, "isFrozen") => Some(Intrinsic::TestIntegrity(2)),
            (Builtin::Array, "isArray") => Some(Intrinsic::IsArray),
            _ => None,
        }
    }

    fn object_call(&mut self, cx: &mut FnCx, c: &'a CallExpression<'a>) -> R<Val> {
        let op = self
            .object_static(&c.callee)
            .expect("checked by the caller");
        if op == Intrinsic::ObjectCreate && c.arguments.len() > 1 {
            return Err(self.unsupported(
                c.span,
                "`Object.create` with property descriptors",
                "not scheduled",
            ));
        }
        let n = op.signature().0.len();
        let args = self.args(cx, &c.arguments, n)?;
        let e = Expr::intrinsic(op, args);
        Ok(
            if matches!(
                op,
                Intrinsic::HasOwn | Intrinsic::IsArray | Intrinsic::TestIntegrity(_)
            ) {
                Val::new(e, Type::Bool)
            } else {
                Val::js(e, Kind::Any)
            },
        )
    }

    /// The arguments of a built-in taking `n` JS values: missing ones are `undefined`, extra ones
    /// are evaluated (after the others) and dropped.
    fn args(&mut self, cx: &mut FnCx, args: &'a [Argument<'a>], n: usize) -> R<Vec<Expr>> {
        let mut out = Vec::new();
        let mut extra = Vec::new();
        let mut result = Ok(());
        for (i, a) in args.iter().enumerate() {
            let Some(e) = a.as_expression() else {
                result = Err(self.unsupported(a.span(), "spread argument", "T15"));
                continue;
            };
            match self.expr(cx, e) {
                Ok(v) if i < n => out.push(jsval_of(v)),
                Ok(v) => extra.push(Stmt::Expr(v.e)),
                Err(e) => result = Err(e),
            }
        }
        result?;
        if !extra.is_empty()
            && let Some(last) = out.pop()
        {
            let t = cx.func.add_local("arg", Type::JsVal);
            let mut body = vec![Stmt::Set(t, last)];
            body.extend(extra);
            out.push(Expr::Seq {
                body,
                value: Box::new(Expr::Local(t)),
            });
        }
        while out.len() < n {
            out.push(Expr::undefined());
        }
        Ok(out)
    }

    /// The object and the key of `o.k` / `o[k]`, as JS values.
    fn member(&mut self, cx: &mut FnCx, m: &'a MemberExpression<'a>) -> R<(Expr, Expr)> {
        if let Some(b) = self.builtin_prototype(m) {
            let _ = b;
            return Err(self.unsupported(
                m.span(),
                "assigning or deleting a built-in constructor's `prototype`",
                "not scheduled",
            ));
        }
        match m {
            MemberExpression::StaticMemberExpression(sm) => {
                let obj = self.expr(cx, &sm.object)?;
                Ok((jsval_of(obj), Expr::str(sm.property.name.as_str())))
            }
            MemberExpression::ComputedMemberExpression(cm) => {
                let obj = self.expr(cx, &cm.object);
                let key = self.expr(cx, &cm.expression);
                let (obj, key) = (obj?, key?);
                Ok((jsval_of(obj), jsval_of(key)))
            }
            MemberExpression::PrivateFieldExpression(p) => {
                Err(self.unsupported(p.span, "private field", "T16"))
            }
        }
    }

    /// The object and key of a member expression that is assigned to, and for a private field
    /// (`o.#x`, T16) the messages for reading and writing it on an object without it.
    fn member_ref(
        &mut self,
        cx: &mut FnCx,
        m: &'a MemberExpression<'a>,
    ) -> R<(Expr, Expr, Member)> {
        if matches!(m.object(), Expression::Super(_)) {
            // `super.x = v`: the home object's prototype, `this` as the receiver.
            let Some(home) = cx.home else {
                return Err(self.unsupported(m.span(), "`super` outside a method", "never"));
            };
            let key = match m {
                MemberExpression::StaticMemberExpression(sm) => {
                    Expr::str(sm.property.name.as_str())
                }
                MemberExpression::ComputedMemberExpression(cm) => {
                    jsval_of(self.expr(cx, &cm.expression)?)
                }
                MemberExpression::PrivateFieldExpression(p) => {
                    return Err(self.unsupported(p.span, "private field", "never"));
                }
            };
            let t = cx.func.add_local("this", Type::JsVal);
            let this = self.this_value(cx);
            let home = Expr::Seq {
                body: vec![Stmt::Set(t, this)],
                value: Box::new(Expr::Local(home)),
            };
            return Ok((home, key, Member::Super(t)));
        }
        let MemberExpression::PrivateFieldExpression(pf) = m else {
            let (obj, key) = self.member(cx, m)?;
            return Ok((obj, key, Member::Plain));
        };
        let obj = jsval_of(self.expr(cx, &pf.object)?);
        let (sym, read, write) = self.private_sym(cx, &pf.field)?;
        Ok((obj, sym, Member::Private(read, write)))
    }

    /// `a?.b.c` (member accesses only): `undefined` as soon as an optional link's object is
    /// `null` or `undefined`.
    fn chain_member(&mut self, cx: &mut FnCx, m: &'a MemberExpression<'a>) -> R<Val> {
        let exit = cx.label();
        let res = cx.func.add_local("chain", Type::JsVal);
        let mut body = Vec::new();
        let v = self.chain_link(cx, m, exit, &mut body)?;
        body.push(Stmt::Set(res, jsval_of(v)));
        Ok(Val::js(
            Expr::Seq {
                body: vec![
                    Stmt::Set(res, Expr::undefined()),
                    Stmt::Block { label: exit, body },
                ],
                value: Box::new(Expr::Local(res)),
            },
            Kind::Any,
        ))
    }

    fn chain_link(
        &mut self,
        cx: &mut FnCx,
        m: &'a MemberExpression<'a>,
        exit: LabelId,
        body: &mut Vec<Stmt>,
    ) -> R<Val> {
        let obj = self.chain_expr(cx, m.object(), exit, body)?;
        let mut obj = jsval_of(obj);
        if m.optional() {
            let t = cx.func.add_local("tmp", Type::JsVal);
            body.push(Stmt::Set(t, obj));
            body.push(Stmt::If {
                cond: Expr::binary(
                    BinOp::LooseEq,
                    Type::JsVal,
                    Expr::Local(t),
                    Expr::Const(Const::Null),
                ),
                then: vec![Stmt::Break(exit)],
                els: vec![],
            });
            obj = Expr::Local(t);
        }
        let key = match m {
            MemberExpression::StaticMemberExpression(sm) => Expr::str(sm.property.name.as_str()),
            MemberExpression::ComputedMemberExpression(cm) => {
                jsval_of(self.expr(cx, &cm.expression)?)
            }
            // `o?.#x`, `o?.a.#x` (T31).
            MemberExpression::PrivateFieldExpression(p) => {
                let (sym, read, _) = self.private_sym(cx, &p.field)?;
                return Ok(Val::js(
                    Expr::intrinsic(Intrinsic::PrivateGet, vec![obj, sym, Expr::str(&read)]),
                    Kind::Any,
                ));
            }
        };
        Ok(Val::js(
            Expr::intrinsic(Intrinsic::GetProp, vec![obj, key]),
            Kind::Any,
        ))
    }

    /// A value inside an optional chain: member accesses and calls continue the chain.
    fn chain_expr(
        &mut self,
        cx: &mut FnCx,
        e: &'a Expression<'a>,
        exit: LabelId,
        body: &mut Vec<Stmt>,
    ) -> R<Val> {
        match e {
            Expression::CallExpression(c) => self.chain_call_link(cx, c, exit, body),
            _ => match e.as_member_expression() {
                Some(m) if self.builtin_prototype(m).is_none() => {
                    self.chain_link(cx, m, exit, body)
                }
                _ => self.expr(cx, e),
            },
        }
    }

    /// An optional chain ending in a call: `o?.m()`, `f?.()`, `a?.b.c()`.
    fn chain_call(&mut self, cx: &mut FnCx, c: &'a CallExpression<'a>) -> R<Val> {
        let exit = cx.label();
        let res = cx.func.add_local("chain", Type::JsVal);
        let mut body = Vec::new();
        let v = self.chain_call_link(cx, c, exit, &mut body)?;
        body.push(Stmt::Set(res, jsval_of(v)));
        Ok(Val::js(
            Expr::Seq {
                body: vec![
                    Stmt::Set(res, Expr::undefined()),
                    Stmt::Block { label: exit, body },
                ],
                value: Box::new(Expr::Local(res)),
            },
            Kind::Any,
        ))
    }

    fn chain_call_link(
        &mut self,
        cx: &mut FnCx,
        c: &'a CallExpression<'a>,
        exit: LabelId,
        body: &mut Vec<Stmt>,
    ) -> R<Val> {
        let nullish_exit = |t: LocalId| Stmt::If {
            cond: Expr::binary(
                BinOp::LooseEq,
                Type::JsVal,
                Expr::Local(t),
                Expr::Const(Const::Null),
            ),
            then: vec![Stmt::Break(exit)],
            els: vec![],
        };
        let callee = c.callee.without_parentheses();
        let (f, this) = match callee.as_member_expression() {
            Some(m) if self.builtin_prototype(m).is_none() => {
                let obj = self.chain_expr(cx, m.object(), exit, body)?;
                let t = cx.func.add_local("this", Type::JsVal);
                body.push(Stmt::Set(t, jsval_of(obj)));
                if m.optional() {
                    body.push(nullish_exit(t));
                }
                let f = match m {
                    MemberExpression::StaticMemberExpression(sm) => Expr::intrinsic(
                        Intrinsic::GetProp,
                        vec![Expr::Local(t), Expr::str(sm.property.name.as_str())],
                    ),
                    MemberExpression::ComputedMemberExpression(cm) => Expr::intrinsic(
                        Intrinsic::GetProp,
                        vec![Expr::Local(t), jsval_of(self.expr(cx, &cm.expression)?)],
                    ),
                    // `o?.#m()` (T31).
                    MemberExpression::PrivateFieldExpression(p) => {
                        let (sym, read, _) = self.private_sym(cx, &p.field)?;
                        Expr::intrinsic(
                            Intrinsic::PrivateGet,
                            vec![Expr::Local(t), sym, Expr::str(&read)],
                        )
                    }
                };
                (f, Expr::Local(t))
            }
            _ => (
                jsval_of(self.chain_expr(cx, callee, exit, body)?),
                Expr::undefined(),
            ),
        };
        let tf = cx.func.add_local("callee", Type::JsVal);
        body.push(Stmt::Set(tf, f));
        if c.optional {
            body.push(nullish_exit(tf));
        }
        let args = self.value_args(cx, &c.arguments)?;
        Ok(Val::js(
            Expr::CallRef {
                callee: Box::new(Expr::Local(tf)),
                this: Box::new(this),
                args,
                what: self.callee_text(&c.callee),
            },
            Kind::Any,
        ))
    }

    fn object_literal(&mut self, cx: &mut FnCx, o: &'a ObjectExpression<'a>) -> R<Val> {
        let t = cx.func.add_local("obj", Type::JsVal);
        let mut body = vec![Stmt::Set(t, Expr::intrinsic(Intrinsic::NewObject, vec![]))];
        let mut result = Ok(());
        for p in &o.properties {
            let r = match p {
                ObjectPropertyKind::SpreadProperty(sp) => self.expr(cx, &sp.argument).map(|v| {
                    body.push(Stmt::Expr(Expr::intrinsic(
                        Intrinsic::CopyProps,
                        vec![Expr::Local(t), jsval_of(v)],
                    )))
                }),
                ObjectPropertyKind::ObjectProperty(op) => {
                    self.literal_property(cx, t, op, &mut body)
                }
            };
            if let Err(e) = r {
                result = Err(e);
            }
        }
        result?;
        Ok(Val::js(
            Expr::Seq {
                body,
                value: Box::new(Expr::Local(t)),
            },
            Kind::Any,
        ))
    }

    /// One `key: value` of a literal: an own property, or the prototype for `__proto__: v`.
    fn literal_property(
        &mut self,
        cx: &mut FnCx,
        t: LocalId,
        p: &'a ObjectProperty<'a>,
        body: &mut Vec<Stmt>,
    ) -> R<()> {
        let (key, proto) = match &p.key {
            PropertyKey::StaticIdentifier(id) => (
                Expr::str(id.name.as_str()),
                id.name == "__proto__" && !p.shorthand,
            ),
            PropertyKey::PrivateIdentifier(id) => {
                return Err(self.unsupported(id.span, "private name", "T16"));
            }
            k => {
                let e = k.as_expression().expect("the other property keys");
                match e {
                    Expression::StringLiteral(sl) if !p.computed => (
                        Expr::Str(utf16(sl.value.as_str(), sl.lone_surrogates)),
                        sl.value == "__proto__",
                    ),
                    _ => (jsval_of(self.expr(cx, e)?), false),
                }
            }
        };
        // A function gets the key as its name (static keys only).
        let name = match &p.key {
            PropertyKey::StaticIdentifier(id) if !p.computed => Some(id.name.as_str()),
            PropertyKey::StringLiteral(sl) if !p.computed => Some(sl.value.as_str()),
            PropertyKey::StaticMemberExpression(m)
                if p.computed
                    && m.property.name == "iterator"
                    && self.builtin_ident(&m.object) == Some(Builtin::Symbol) =>
            {
                Some("[Symbol.iterator]")
            }
            _ => None,
        };
        // An anonymous function under a computed key: named from the key at run time (F59).
        let key = if name.is_none() && anonymous_function(&p.value) {
            let tk = cx.func.add_local("key", Type::JsVal);
            body.push(Stmt::Set(tk, key));
            Expr::Local(tk)
        } else {
            key
        };
        let runtime_name = |f: Expr, prefix: &str| match name {
            None if anonymous_function(&p.value) => Expr::intrinsic(
                Intrinsic::SetFunctionName,
                vec![f, key.clone(), Expr::str(prefix)],
            ),
            _ => f,
        };
        if p.kind != PropertyKind::Init {
            // A getter or setter (T16): named `get x` / `set x`.
            let setter = p.kind == PropertyKind::Set;
            let prefix = if setter { "set " } else { "get " };
            let fname = name.map(|name| format!("{prefix}{name}"));
            let saved_home = cx.home;
            cx.home = Some(t);
            let f = self.named(cx, &p.value, fname.as_deref());
            cx.home = saved_home;
            let f = runtime_name(jsval_of(f?), prefix);
            body.push(Stmt::Expr(Expr::intrinsic(
                Intrinsic::DefineAccessor {
                    setter,
                    enumerable: true,
                },
                vec![Expr::Local(t), key, f],
            )));
            return Ok(());
        }
        // A method's home object (`super.x`) is the literal.
        let saved_home = cx.home;
        if p.method {
            cx.home = Some(t);
        }
        let v = self.named(cx, &p.value, name);
        cx.home = saved_home;
        let v = runtime_name(jsval_of(v?), "");
        body.push(Stmt::Expr(if proto {
            Expr::intrinsic(Intrinsic::SetProto, vec![Expr::Local(t), v])
        } else {
            Expr::intrinsic(Intrinsic::DefineProp, vec![Expr::Local(t), key, v])
        }));
        Ok(())
    }

    /// `o.k = v` / `o[k] op= v`: the stored value.
    fn member_assign(&mut self, cx: &mut FnCx, a: &'a AssignmentExpression<'a>) -> R<Val> {
        let m = a
            .left
            .as_member_expression()
            .expect("checked by the caller");
        let (obj, key, private) = self.member_ref(cx, m)?;
        let to = cx.func.add_local("obj", Type::JsVal);
        let tk = cx.func.add_local("key", Type::JsVal);
        let tv = cx.func.add_local("value", Type::JsVal);
        let mut body = vec![Stmt::Set(to, obj), Stmt::Set(tk, key)];
        let rhs = self.expr(cx, &a.right)?;
        let old = (a.operator != AssignmentOperator::Assign).then(|| {
            Val::js(
                member_get(&private, Expr::Local(to), Expr::Local(tk)),
                Kind::Any,
            )
        });
        let value = self.combine(cx, a.operator, old, rhs, a.span)?;
        let kind = match value.kind {
            Kind::Str | Kind::Bool => value.kind,
            _ => Kind::Any,
        };
        body.push(Stmt::Expr(member_set(
            &private,
            Expr::Local(to),
            Expr::Local(tk),
            Expr::Tee(tv, Box::new(jsval_of(value))),
        )));
        Ok(Val::js(
            Expr::Seq {
                body,
                value: Box::new(Expr::Local(tv)),
            },
            kind,
        ))
    }

    /// `o.k++` / `--o[k]`: the old or new value, as a number.
    fn member_update(&mut self, cx: &mut FnCx, u: &'a UpdateExpression<'a>) -> R<Val> {
        let m = u
            .argument
            .as_member_expression()
            .expect("checked by the caller");
        let (obj, key, private) = self.member_ref(cx, m)?;
        let to = cx.func.add_local("obj", Type::JsVal);
        let tk = cx.func.add_local("key", Type::JsVal);
        if self.module.bigint {
            // T40: the property may hold a BigInt: ToNumeric, then `± 1` of its kind.
            let old = cx.func.add_local("old", Type::JsVal);
            let new = cx.func.add_local("new", Type::JsVal);
            let n = if u.operator == UpdateOperator::Increment {
                ir::NumericOp::Inc
            } else {
                ir::NumericOp::Dec
            };
            let get = member_get(&private, Expr::Local(to), Expr::Local(tk));
            let body = vec![
                Stmt::Set(to, obj),
                Stmt::Set(tk, key),
                Stmt::Set(old, Expr::intrinsic(Intrinsic::ToNumeric, vec![get])),
                Stmt::Set(
                    new,
                    Expr::intrinsic(Intrinsic::Numeric(n), vec![Expr::Local(old)]),
                ),
                Stmt::Expr(member_set(
                    &private,
                    Expr::Local(to),
                    Expr::Local(tk),
                    Expr::Local(new),
                )),
            ];
            let value = Box::new(Expr::Local(if u.prefix { new } else { old }));
            return Ok(Val::js(Expr::Seq { body, value }, Kind::Any));
        }
        let old = cx.func.add_local("old", Type::F64);
        let new = cx.func.add_local("new", Type::F64);
        let op = if u.operator == UpdateOperator::Increment {
            BinOp::Add
        } else {
            BinOp::Sub
        };
        let get = member_get(&private, Expr::Local(to), Expr::Local(tk));
        let body = vec![
            Stmt::Set(to, obj),
            Stmt::Set(tk, key),
            Stmt::Set(old, Expr::convert(Type::JsVal, Type::F64, get)),
            Stmt::Set(
                new,
                Expr::binary(op, Type::F64, Expr::Local(old), Expr::f64(1.0)),
            ),
            Stmt::Expr(member_set(
                &private,
                Expr::Local(to),
                Expr::Local(tk),
                Expr::convert(Type::F64, Type::JsVal, Expr::Local(new)),
            )),
        ];
        let value = Box::new(Expr::Local(if u.prefix { new } else { old }));
        Ok(Val::num(Expr::Seq { body, value }))
    }

    /// `for (k in o) body`: the keys are collected first ([`Intrinsic::ForInKeys`]); a key
    /// deleted before its turn is skipped.
    fn for_in(
        &mut self,
        cx: &mut FnCx,
        name: Option<String>,
        f: &'a ForInStatement<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let right = self.expr(cx, &f.right)?;
        let obj = cx.func.add_local("for_in", Type::JsVal);
        let keys = cx.func.add_local("keys", Type::JsVal);
        let i = cx.func.add_local("i", Type::I32);
        let n = cx.func.add_local("n", Type::I32);
        let k = cx.func.add_local("key", Type::JsVal);
        out.push(Stmt::Set(obj, jsval_of(right)));
        out.push(Stmt::Set(
            keys,
            Expr::intrinsic(Intrinsic::ForInKeys, vec![Expr::Local(obj)]),
        ));
        out.push(Stmt::Set(
            n,
            Expr::intrinsic(Intrinsic::KeysLen, vec![Expr::Local(keys)]),
        ));
        out.push(Stmt::Set(i, Expr::i32(0)));
        let brk = cx.label();
        let mut then = Vec::new();
        let target = self.for_in_target(cx, &f.left, Val::js(Expr::Local(k), Kind::Str), &mut then);
        cx.targets.push(Target {
            js_label: name,
            brk,
            kind: TargetKind::While,
        });
        let body = self.stmt_body(cx, &f.body);
        cx.targets.pop();
        target?;
        then.extend(body);
        let present = bool_cond(
            Expr::TypeofIs {
                arg: Box::new(Expr::Local(obj)),
                tag: TypeofTag::Object,
            },
            Expr::intrinsic(Intrinsic::HasProp, vec![Expr::Local(k), Expr::Local(obj)]),
            Expr::bool(true),
        );
        let inner = vec![
            Stmt::If {
                cond: Expr::binary(BinOp::Ge, Type::I32, Expr::Local(i), Expr::Local(n)),
                then: vec![Stmt::Break(brk)],
                els: vec![],
            },
            Stmt::Set(
                k,
                Expr::intrinsic(Intrinsic::KeysGet, vec![Expr::Local(keys), Expr::Local(i)]),
            ),
            Stmt::Set(
                i,
                Expr::binary(BinOp::Add, Type::I32, Expr::Local(i), Expr::i32(1)),
            ),
            Stmt::If {
                cond: present,
                then,
                els: vec![],
            },
            Stmt::Continue(brk),
        ];
        out.push(Stmt::Loop {
            label: brk,
            body: inner,
        });
        Ok(())
    }

    /// Store the key into the `for-in` variable.
    fn for_in_target(
        &mut self,
        cx: &mut FnCx,
        left: &'a ForStatementLeft<'a>,
        key: Val,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        match left {
            ForStatementLeft::VariableDeclaration(d) => {
                let decl = &d.declarations[0];
                let BindingPattern::BindingIdentifier(b) = &decl.id else {
                    let fresh = d.kind != VariableDeclarationKind::Var;
                    return self.pattern(
                        cx,
                        Pat::Bind(&decl.id),
                        jsval_of(key),
                        Mode::Decl { fresh },
                        None,
                        out,
                    );
                };
                let sym = b
                    .symbol_id
                    .get()
                    .expect("semantic analysis assigns symbols");
                let slot = Slot::Var(sym);
                let ty = self.slot_ty(slot);
                let v = self.store(slot, ty, key);
                let fresh = d.kind != VariableDeclarationKind::Var;
                out.extend(self.declare_store(cx, sym, ty, v, fresh));
                Ok(())
            }
            ForStatementLeft::AssignmentTargetIdentifier(id) => {
                let var = self.resolve(cx, id)?;
                self.check_assignable(&var, id.span)?;
                let v = self.store(Slot::Var(var.sym), var.ty, key);
                out.extend(self.write_var(cx, &var, v));
                Ok(())
            }
            left => match left.as_assignment_target() {
                Some(t) => self.pattern(cx, Pat::Target(t), jsval_of(key), Mode::Assign, None, out),
                None => Err(self.unsupported(left.span(), "`for-in` target", "later")),
            },
        }
    }

    /// `t &&= e`, `t ||= e`, `t ??= e` (T24): `e` is evaluated and `t` written only when the
    /// old value asks for it (truthy / falsy / nullish); the result is the old value or the stored
    /// one. A member target's object and key are evaluated once, before `e`.
    fn logical_assign(&mut self, cx: &mut FnCx, a: &'a AssignmentExpression<'a>) -> R<Val> {
        let op = a.operator;
        // Is the old value (`old`, read once) kept? Else the assignment runs.
        let keep = |old: Val| match op {
            AssignmentOperator::LogicalOr => cond_of(old),
            AssignmentOperator::LogicalAnd => Expr::unary(UnOp::Not, Type::Bool, cond_of(old)),
            _ => Expr::unary(
                UnOp::Not,
                Type::Bool,
                Expr::binary(
                    BinOp::LooseEq,
                    Type::JsVal,
                    jsval_of(old),
                    Expr::Const(Const::Null),
                ),
            ),
        };
        if let Some(m) = a.left.as_member_expression() {
            let (obj, key, private) = self.member_ref(cx, m)?;
            let to = cx.func.add_local("obj", Type::JsVal);
            let tk = cx.func.add_local("key", Type::JsVal);
            let old = cx.func.add_local("old", Type::JsVal);
            let tv = cx.func.add_local("value", Type::JsVal);
            let body = vec![
                Stmt::Set(to, obj),
                Stmt::Set(tk, key),
                Stmt::Set(old, member_get(&private, Expr::Local(to), Expr::Local(tk))),
            ];
            let rhs = jsval_of(self.expr(cx, &a.right)?);
            let assign = Expr::Seq {
                body: vec![Stmt::Expr(member_set(
                    &private,
                    Expr::Local(to),
                    Expr::Local(tk),
                    Expr::Tee(tv, Box::new(rhs)),
                ))],
                value: Box::new(Expr::Local(tv)),
            };
            let e = Expr::Cond {
                cond: Box::new(keep(Val::js(Expr::Local(old), Kind::Any))),
                then: Box::new(Expr::Local(old)),
                els: Box::new(assign),
                ty: Type::JsVal,
            };
            return Ok(Val::js(
                Expr::Seq {
                    body,
                    value: Box::new(e),
                },
                Kind::Any,
            ));
        }
        let AssignmentTarget::AssignmentTargetIdentifier(id) = &a.left else {
            return Err(self.unsupported(a.left.span(), "assignment target", "never"));
        };
        let var = self.resolve(cx, id)?;
        let old = self.read_var(&var);
        if op == AssignmentOperator::LogicalNullish
            && (var.ty != Type::JsVal || matches!(var.kind, Kind::Num | Kind::Bool | Kind::Str))
        {
            // Never null or undefined: nothing is assigned.
            return Ok(old);
        }
        self.check_assignable(&var, id.span)?;
        let rhs = self.named(cx, &a.right, Some(id.name.as_str()))?;
        let value = self.store(Slot::Var(var.sym), var.ty, rhs);
        let assign = self.assign_value(cx, &var, value);
        let t = cx.func.add_local("old", var.ty);
        let cond = keep(Val {
            e: Expr::Tee(t, Box::new(old.e)),
            ty: var.ty,
            kind: var.kind,
        });
        Ok(Val {
            e: Expr::Cond {
                cond: Box::new(cond),
                then: Box::new(Expr::Local(t)),
                els: Box::new(assign),
                ty: var.ty,
            },
            ty: var.ty,
            kind: var.kind,
        })
    }

    /// `x = e` / `x op= e`: the variable and the converted value to store.
    fn assignment(&mut self, cx: &mut FnCx, a: &'a AssignmentExpression<'a>) -> R<(Var, Expr)> {
        let AssignmentTarget::AssignmentTargetIdentifier(id) = &a.left else {
            return Err(self.unsupported(
                a.left.span(),
                "assignment to something other than a variable or a property (destructuring)",
                "T15",
            ));
        };
        let var = self.resolve(cx, id)?;
        self.check_assignable(&var, id.span)?;
        let rhs = if a.operator == AssignmentOperator::Assign {
            self.named(cx, &a.right, Some(id.name.as_str()))?
        } else {
            self.expr(cx, &a.right)?
        };
        if a.operator == AssignmentOperator::Addition && var.kind == Kind::Str {
            self.hint_string(cx, &a.right);
        }
        let old = (a.operator != AssignmentOperator::Assign).then(|| self.read_var(&var));
        let value = self.combine(cx, a.operator, old, rhs, a.span)?;
        Ok((var, self.store(Slot::Var(var.sym), var.ty, value)))
    }

    /// The value `x op= rhs` stores, given the old value (`None` for `=`).
    fn combine(
        &mut self,
        cx: &mut FnCx,
        op: AssignmentOperator,
        old: Option<Val>,
        rhs: Val,
        span: Span,
    ) -> R<Val> {
        use AssignmentOperator as A;
        let arith = match op {
            A::Subtraction => Some(BinOp::Sub),
            A::Multiplication => Some(BinOp::Mul),
            A::Division => Some(BinOp::Div),
            A::Remainder => Some(BinOp::Rem),
            A::Exponential => Some(BinOp::Pow),
            _ => None,
        };
        let bitwise = match op {
            A::BitwiseOR => Some(BinOp::Or),
            A::BitwiseAnd => Some(BinOp::And),
            A::BitwiseXOR => Some(BinOp::Xor),
            A::ShiftLeft => Some(BinOp::Shl),
            A::ShiftRight => Some(BinOp::Shr),
            _ => None,
        };
        let num = Val::num;
        let Some(old) = old else {
            return Ok(rhs);
        };
        if self.may_be_bigint(&old) || self.may_be_bigint(&rhs) {
            // T40: a BigInt may meet the operator.
            let bop = arith.or(bitwise).or((op == A::ShiftRightZeroFill).then_some(BinOp::ShrU));
            if let Some(bop) = bop {
                let e = Expr::intrinsic(
                    Intrinsic::Numeric(ir::NumericOp::Binary(bop)),
                    vec![jsval_of(old), jsval_of(rhs)],
                );
                return Ok(Val::js(e, Kind::Any));
            }
        }
        Ok(if op == A::Addition {
            add(old, rhs)
        } else if let Some(bop) = arith {
            let (l, r) = (self.num(old, span)?, self.num(rhs, span)?);
            num(Expr::binary(bop, Type::F64, l, r))
        } else if let Some(bop) = bitwise {
            let (l, r) = (self.num(old, span)?, self.num(rhs, span)?);
            num(from_i32(Expr::binary(bop, Type::I32, to_i32(l), to_i32(r))))
        } else if op == A::ShiftRightZeroFill {
            let (l, r) = (self.num(old, span)?, self.num(rhs, span)?);
            num(ushr(cx, l, r))
        } else {
            return Err(self.unsupported(span, &format!("operator `{}`", op.as_str()), "later"));
        })
    }

    fn update_target(&mut self, cx: &mut FnCx, u: &'a UpdateExpression<'a>) -> R<(Var, BinOp)> {
        let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &u.argument else {
            return Err(self.unsupported(
                u.argument.span(),
                "`++`/`--` on something other than a variable or a property",
                "later",
            ));
        };
        let var = self.resolve(cx, id)?;
        self.check_assignable(&var, id.span)?;
        let op = if u.operator == UpdateOperator::Increment {
            BinOp::Add
        } else {
            BinOp::Sub
        };
        Ok((var, op))
    }

    /// `++x` / `x++` as a statement: the variable and the converted new value.
    fn update(&mut self, cx: &mut FnCx, u: &'a UpdateExpression<'a>) -> R<(Var, Expr)> {
        let (var, op) = self.update_target(cx, u)?;
        let old = self.read_var(&var);
        if self.may_be_bigint(&old) {
            // T40: `± 1` of the kind of its ToNumeric.
            let new = Val::js(
                Expr::intrinsic(Intrinsic::Numeric(inc_dec(op)), vec![jsval_of(old)]),
                Kind::Any,
            );
            return Ok((var, self.store(Slot::Var(var.sym), var.ty, new)));
        }
        let old = self.num(old, u.span)?;
        let new = Val::num(Expr::binary(op, Type::F64, old, Expr::f64(1.0)));
        Ok((var, self.store(Slot::Var(var.sym), var.ty, new)))
    }

    fn resolve(&mut self, cx: &FnCx, id: &IdentifierReference<'a>) -> R<Var> {
        let span = id.span;
        if self.prelude_leak(id) {
            return Err(Unsupported);
        }
        let Some(sym) = self.scoping.get_reference(id.reference_id()).symbol_id() else {
            if id.name == "arguments" && cx.scope.is_some() {
                // Reads are handled in `expr`; arrow functions and classes were reported by the
                // closure analysis (T25).
                if cx.arguments.is_none() {
                    return Err(Unsupported);
                }
                return Err(self.unsupported(span, "`arguments` used other than read", "later"));
            }
            return Err(self.unsupported(span, &format!("global `{}`", id.name), "T21"));
        };
        let constant = self.scoping.symbol_flags(sym).is_const_variable() || self.p.is_import(span.start);
        if let Some(&l) = cx.values.get(&sym) {
            // A captured value or the function's own name: never changes.
            return Ok(Var {
                place: Place::Local(l),
                sym,
                ty: Type::JsVal,
                kind: Kind::Any,
                guard: None,
                constant,
            });
        }
        if let Some(&(l, checked)) = cx.cells.get(&sym) {
            if cx.pending.contains(&sym) {
                return Err(self.unsupported(
                    span,
                    &format!("`{}` used before its declaration", id.name),
                    "later",
                ));
            }
            return Ok(Var {
                place: Place::Cell(l),
                sym,
                ty: self.slot_ty(Slot::Var(sym)),
                kind: self.slot_kind(Slot::Var(sym)),
                guard: checked.then_some((Guard::Cell(l), sym, span.start)),
                constant,
            });
        }
        let place = if let Some(l) = cx.locals.get(&sym) {
            Some((Place::Local(*l), cx.func.local(*l).ty))
        } else {
            self.globals
                .get(&sym)
                .map(|g| (Place::Global(*g), self.module.globals[g.0 as usize].ty))
        };
        let in_main = self.is_top_code(cx.id);
        let guard = if in_main {
            // `main` runs top to bottom: before the declaration it's always in the TDZ.
            self.lexical_end
                .get(&sym)
                .is_some_and(|end| span.start < *end)
                .then_some(Guard::Always)
        } else {
            self.tdz_flags.get(&sym).map(|g| Guard::Flag(*g))
        };
        if let Some((place, ty)) = place {
            // f64 slots are `Num` or `Bottom`, matching their type.
            let kind = self.slot_kind(Slot::Var(sym));
            return Ok(Var {
                place,
                sym,
                ty,
                kind,
                guard: guard.map(|g| (g, sym, span.start)),
                constant,
            });
        }
        if self.funcs.contains_key(&sym) {
            return Err(self.unsupported(
                span,
                &format!(
                    "assignment to function `{}`",
                    self.p.written_name(span.start).unwrap_or_else(|| self.p.original_name(&id.name))
                ),
                "later",
            ));
        }
        Err(self.unsupported(
            span,
            &format!("`{}` used before its declaration", id.name),
            "later",
        ))
    }

    /// Is `id` a name declared nowhere that isn't a global in node either (F139)? Reading it throws
    /// `ReferenceError: x is not defined` at run time. node's globals that aren't implemented stay
    /// "unsupported" (`global \`Reflect\``).
    fn undeclared(&self, id: &IdentifierReference<'a>) -> bool {
        self.scoping.get_reference(id.reference_id()).symbol_id().is_none()
            && id.name != "arguments"
            && !id.name.starts_with("__intrinsic_")
            && NODE_GLOBALS.binary_search(&id.name.as_str()).is_err()
    }

    /// An assignment target that is a name no scope declares (T42): an undeclared name, or the
    /// read-only `NaN`/`Infinity`/`undefined`.
    fn global_target<'b>(&self, t: &'b SimpleAssignmentTarget<'a>) -> Option<&'b IdentifierReference<'a>> {
        let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = t else {
            return None;
        };
        let unresolved = self.scoping.get_reference(id.reference_id()).symbol_id().is_none();
        (unresolved
            && (self.undeclared(id) || matches!(id.name.as_str(), "NaN" | "Infinity" | "undefined")))
        .then_some(&**id)
    }

    /// Reading the unresolvable name `id` (T42): the global object's property (a ReferenceError if
    /// absent), or without a global object always the ReferenceError (F139).
    fn global_read(&mut self, id: &IdentifierReference<'a>) -> Val {
        if let Some(c) = self.global_constant(id) {
            return Val::new(Expr::Const(c), c.ty());
        }
        if self.prelude.global_object {
            return Val::js(self.prelude_call("$$GlobalGet", vec![Expr::str(&id.name)]), Kind::Any);
        }
        Val::js(not_defined(&id.name), Kind::Any)
    }

    /// Writing `v` (a `jsval`) to the unresolvable name `name` (strict code, T42).
    fn global_write(&mut self, name: &str, v: Expr) -> Vec<Stmt> {
        if matches!(name, "NaN" | "Infinity" | "undefined") {
            return vec![
                Stmt::Expr(v),
                Stmt::Throw(Expr::intrinsic(
                    Intrinsic::NewError(ErrorKind::TypeError),
                    vec![
                        Expr::str(&format!(
                            "Cannot assign to read only property '{name}' of object '#<Object>'"
                        )),
                        Expr::undefined(),
                    ],
                )),
            ];
        }
        if self.prelude.global_object {
            return vec![Stmt::Expr(self.prelude_call("$$GlobalSet", vec![Expr::str(name), v]))];
        }
        vec![
            Stmt::Expr(v),
            Stmt::Throw(Expr::intrinsic(
                Intrinsic::NewError(ErrorKind::ReferenceError),
                vec![Expr::str(&format!("{name} is not defined")), Expr::undefined()],
            )),
        ]
    }

    /// `x = e`, `x op= e` for an unresolvable name `x` (T42). The old value is read before `e`.
    fn global_assign(&mut self, cx: &mut FnCx, a: &'a AssignmentExpression<'a>) -> R<Val> {
        let id = a
            .left
            .as_simple_assignment_target()
            .and_then(|t| self.global_target(t))
            .expect("checked by the caller");
        if a.operator.is_logical() {
            return Err(self.unsupported(a.span, "logical assignment to a global", "later"));
        }
        let old = (a.operator != AssignmentOperator::Assign).then(|| self.global_read(id));
        let rhs = if a.operator == AssignmentOperator::Assign {
            self.named(cx, &a.right, Some(id.name.as_str()))?
        } else {
            self.expr(cx, &a.right)?
        };
        let value = self.combine(cx, a.operator, old, rhs, a.span)?;
        let kind = match value.kind {
            Kind::Str | Kind::Bool => value.kind,
            _ => Kind::Any,
        };
        let tv = cx.func.add_local("value", Type::JsVal);
        let mut body = vec![Stmt::Set(tv, jsval_of(value))];
        body.extend(self.global_write(&id.name, Expr::Local(tv)));
        Ok(Val::js(
            Expr::Seq {
                body,
                value: Box::new(Expr::Local(tv)),
            },
            kind,
        ))
    }

    /// `x++` / `--x` for an unresolvable name `x` (T42).
    fn global_update(&mut self, cx: &mut FnCx, u: &'a UpdateExpression<'a>) -> R<Val> {
        let id = self.global_target(&u.argument).expect("checked by the caller");
        let read = jsval_of(self.global_read(id));
        let old = cx.func.add_local("old", Type::JsVal);
        let new = cx.func.add_local("new", Type::JsVal);
        let (to_old, step) = if self.module.bigint {
            let n = if u.operator == UpdateOperator::Increment {
                ir::NumericOp::Inc
            } else {
                ir::NumericOp::Dec
            };
            (
                Expr::intrinsic(Intrinsic::ToNumeric, vec![read]),
                Expr::intrinsic(Intrinsic::Numeric(n), vec![Expr::Local(old)]),
            )
        } else {
            let op = if u.operator == UpdateOperator::Increment {
                BinOp::Add
            } else {
                BinOp::Sub
            };
            let num = Expr::convert(Type::JsVal, Type::F64, Expr::Local(old));
            (
                Expr::convert(Type::F64, Type::JsVal, Expr::convert(Type::JsVal, Type::F64, read)),
                Expr::convert(
                    Type::F64,
                    Type::JsVal,
                    Expr::binary(op, Type::F64, num, Expr::f64(1.0)),
                ),
            )
        };
        let mut body = vec![Stmt::Set(old, to_old), Stmt::Set(new, step)];
        body.extend(self.global_write(&id.name, Expr::Local(new)));
        let value = Box::new(Expr::Local(if u.prefix { new } else { old }));
        Ok(Val::js(Expr::Seq { body, value }, Kind::Any))
    }

    /// A global-object property's value (T42): a built-in the code names, or `None` if it isn't
    /// a value here. `g`: the global object itself (`globalThis`, `global`).
    fn global_value(&mut self, name: &str, g: &Expr) -> Option<Expr> {
        if matches!(name, "globalThis" | "global") {
            return Some(g.clone());
        }
        if let Some(ns) = ir::Namespace::parse(name)
            && self.prelude.ctor_values.contains(name)
        {
            self.module.namespaces = true;
            return Some(Expr::intrinsic(Intrinsic::Namespace(ns), vec![]));
        }
        if let Some(k) = ErrorKind::parse(name) {
            self.note_error_kind(k);
            return Some(Expr::intrinsic(Intrinsic::ErrorCtor(k), vec![]));
        }
        if let Some(b) = Builtin::parse(name)
            && self.prelude.ctor_values.contains(name)
        {
            return Some(Expr::intrinsic(Intrinsic::BuiltinCtor(b), vec![]));
        }
        match name {
            "Promise" => {
                self.module.promises = true;
                return Some(Expr::intrinsic(Intrinsic::PromiseCtor, vec![]));
            }
            "queueMicrotask" => return Some(Expr::intrinsic(Intrinsic::QueueMicrotask, vec![])),
            _ => {}
        }
        if let Some(k) = ir::TIMER_NAMES.iter().position(|n| *n == name) {
            let async_export = self
                .exported
                .iter()
                .any(|f| self.module.function(*f).flags.is_async);
            if !self.command && !async_export {
                return None;
            }
            return self.timer_value(Span::default(), k as u8).ok().map(|v| v.e);
        }
        let sym = self.prelude.names.get(&format!("$${name}"))?;
        let func = *self.funcs.get(sym)?;
        Some(self.func_value(func))
    }

    /// The TDZ check of a guarded variable access: throws a ReferenceError like node's.
    /// `at`: where the variable is used (a joined program names an import by its local name there).
    fn tdz_check(&self, guard: Guard, sym: SymbolId, at: u32) -> Stmt {
        let name = self
            .p
            .written_name(at)
            .unwrap_or_else(|| self.p.original_name(self.scoping.symbol_name(sym)));
        let throw = Stmt::Throw(Expr::intrinsic(
            Intrinsic::NewError(ErrorKind::ReferenceError),
            vec![
                Expr::str(&format!("Cannot access '{name}' before initialization")),
                Expr::undefined(),
            ],
        ));
        match guard {
            Guard::Always => throw,
            Guard::Flag(g) => Stmt::If {
                cond: Expr::unary(UnOp::Not, Type::Bool, Expr::Global(g)),
                then: vec![throw],
                els: vec![],
            },
            Guard::Cell(l) => Stmt::If {
                cond: Expr::intrinsic(Intrinsic::CellIsUninit, vec![Expr::Local(l)]),
                then: vec![throw],
                els: vec![],
            },
        }
    }

    /// Read a variable.
    fn read_var(&self, var: &Var) -> Val {
        let e = read(var.place, var.ty);
        let e = match var.guard {
            Some((g, sym, at)) => Expr::Seq {
                body: vec![self.tdz_check(g, sym, at)],
                value: Box::new(e),
            },
            None => e,
        };
        Val {
            e,
            ty: var.ty,
            kind: var.kind,
        }
    }

    /// Store `value` (already converted to the variable's type) into a variable. The TDZ check
    /// comes after the value is computed, as in JS.
    fn write_var(&self, cx: &mut FnCx, var: &Var, value: Expr) -> Vec<Stmt> {
        if var.constant {
            // T24: the value is evaluated, then (after the TDZ check) the write throws.
            let mut out = vec![Stmt::Expr(value)];
            out.extend(var.guard.map(|(g, sym, at)| self.tdz_check(g, sym, at)));
            out.push(Stmt::Throw(Expr::intrinsic(
                Intrinsic::NewError(ErrorKind::TypeError),
                vec![
                    Expr::str("Assignment to constant variable."),
                    Expr::undefined(),
                ],
            )));
            return out;
        }
        match var.guard {
            None => vec![set(var.place, var.ty, value)],
            Some((g, sym, at)) => {
                let tmp = cx.func.add_local("tmp", var.ty);
                vec![
                    Stmt::Set(tmp, value),
                    self.tdz_check(g, sym, at),
                    set(var.place, var.ty, Expr::Local(tmp)),
                ]
            }
        }
    }

    /// An assignment used as a value: the stored value.
    fn assign_value(&self, cx: &mut FnCx, var: &Var, value: Expr) -> Expr {
        let constant = var.constant;
        match (var.place, var.guard) {
            (Place::Local(l), None) if !constant => Expr::Tee(l, Box::new(value)),
            _ => Expr::Seq {
                body: self.write_var(cx, var, value),
                value: Box::new(read(var.place, var.ty)),
            },
        }
    }

    /// `undefined`, `NaN` and `Infinity` when they refer to the global constants.
    fn global_constant(&self, id: &IdentifierReference<'a>) -> Option<Const> {
        if self
            .scoping
            .get_reference(id.reference_id())
            .symbol_id()
            .is_some()
        {
            return None; // shadowed by a declaration
        }
        match id.name.as_str() {
            "undefined" => Some(Const::Undefined),
            "NaN" => Some(Const::F64(f64::NAN)),
            "Infinity" => Some(Const::F64(f64::INFINITY)),
            _ => None,
        }
    }

    /// A literal allowed as a top-level initializer: number (possibly negated), boolean,
    /// `null`, `undefined`, `NaN`, `Infinity`.
    fn literal(&self, e: &Expression<'a>) -> Option<Const> {
        match e {
            Expression::NumericLiteral(n) => Some(Const::F64(n.value)),
            Expression::BooleanLiteral(b) => Some(Const::Bool(b.value)),
            Expression::NullLiteral(_) => Some(Const::Null),
            Expression::Identifier(id) => self.global_constant(id),
            Expression::ParenthesizedExpression(p) => self.literal(&p.expression),
            Expression::UnaryExpression(u) if u.operator == UnaryOperator::UnaryNegation => {
                match self.literal(&u.argument)? {
                    Const::F64(v) => Some(Const::F64(-v)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn undefined_val(&self) -> Val {
        Val::js(Expr::undefined(), Kind::Any)
    }

    /// `e`, where an anonymous function or arrow function gets `name` (`const f = () => …`).
    fn named(&mut self, cx: &mut FnCx, e: &'a Expression<'a>, name: Option<&str>) -> R<Val> {
        let scope = match e.without_parentheses() {
            Expression::FunctionExpression(f) => f.scope_id.get(),
            Expression::ArrowFunctionExpression(a) => a.scope_id.get(),
            Expression::ClassExpression(c) => {
                return Ok(Val::js(self.class_value(cx, c, name, None)?, Kind::Any));
            }
            _ => return self.expr(cx, e),
        };
        let scope = scope.expect("functions have scopes");
        let c = self.closure(cx, scope, name, e.span())?;
        Ok(Val::js(c, Kind::Any))
    }

    /// Reject assigning to a function's binding: calls to it are direct.
    fn check_assignable(&mut self, var: &Var, span: Span) -> R<()> {
        if var.constant {
            return Ok(()); // the write throws
        }
        if self.nested_decl.contains_key(&var.sym) || self.an.self_names.contains_key(&var.sym) {
            return Err(self.unsupported(
                span,
                &format!(
                    "assignment to function `{}`",
                    self.p.original_name(self.scoping.symbol_name(var.sym))
                ),
                "later",
            ));
        }
        Ok(())
    }

    /// Store the value of a declaration of `sym` (`let x = v`, a `for-in` key, a `catch`
    /// parameter). `fresh`: a block-scoped binding, which gets a new cell each time the declaration
    /// runs (a loop body's `let` is a new variable in every iteration).
    fn declare_store(
        &mut self,
        cx: &mut FnCx,
        sym: SymbolId,
        ty: Type,
        v: Expr,
        fresh: bool,
    ) -> Vec<Stmt> {
        if let Some(g) = self.globals.get(&sym) {
            return vec![Stmt::SetGlobal(*g, v)];
        }
        if !self.an.captured.contains(&sym) {
            let local = self.local_for(cx, sym, ty);
            return vec![Stmt::Set(local, v)];
        }
        match cx.cells.get(&sym) {
            Some(&(l, _)) if cx.pending.remove(&sym) || !fresh => vec![Stmt::Expr(
                Expr::intrinsic(Intrinsic::CellSet(ty), vec![Expr::Local(l), v]),
            )],
            Some(&(l, _)) => vec![Stmt::Set(
                l,
                Expr::intrinsic(Intrinsic::NewCell(ty), vec![v]),
            )],
            None => {
                let name = format!("{}.cell", self.scoping.symbol_name(sym));
                let l = cx.func.add_local(name, Type::JsVal);
                cx.cells.insert(sym, (l, false));
                vec![Stmt::Set(
                    l,
                    Expr::intrinsic(Intrinsic::NewCell(ty), vec![v]),
                )]
            }
        }
    }

    /// The local of an uncaptured variable, created on first use.
    fn local_for(&mut self, cx: &mut FnCx, sym: SymbolId, ty: Type) -> LocalId {
        match cx.locals.get(&sym) {
            Some(l) => *l,
            None => {
                let l = cx.func.add_local(self.scoping.symbol_name(sym), ty);
                cx.locals.insert(sym, l);
                l
            }
        }
    }
}

// ---- conversions -----------------------------------------------------------------------

/// What an assigned member expression is (T16).
enum Member {
    Plain,
    /// A private field: the messages for reading and writing it on an object without it.
    Private(String, String),
    /// `super.x`: the object is the home object; the local holding `this`.
    Super(LocalId),
}

/// Read a member.
/// The global names of node 24 in an ES module (sorted, for `binary_search`): a name declared
/// nowhere that is one of these is a built-in js2wasi3 may lack ("unsupported"); any other name
/// throws a ReferenceError when read (F139).
const NODE_GLOBALS: &[&str] = &[
    "AbortController", "AbortSignal", "AggregateError", "Array", "ArrayBuffer",
    "AsyncDisposableStack", "Atomics", "BigInt", "BigInt64Array", "BigUint64Array", "Blob",
    "Boolean", "BroadcastChannel", "Buffer", "ByteLengthQueuingStrategy", "CloseEvent",
    "CompressionStream", "CountQueuingStrategy", "Crypto", "CryptoKey", "CustomEvent",
    "DOMException", "DataView", "Date", "DecompressionStream", "DisposableStack", "Error",
    "EvalError", "Event", "EventTarget", "File", "FinalizationRegistry", "Float16Array",
    "Float32Array", "Float64Array", "FormData", "Function", "Headers", "Infinity", "Int16Array",
    "Int32Array", "Int8Array", "Intl", "Iterator", "JSON", "Map", "Math", "MessageChannel",
    "MessageEvent", "MessagePort", "NaN", "Navigator", "Number", "Object", "Performance",
    "PerformanceEntry", "PerformanceMark", "PerformanceMeasure", "PerformanceObserver",
    "PerformanceObserverEntryList", "PerformanceResourceTiming", "Promise", "Proxy", "RangeError",
    "ReadableByteStreamController", "ReadableStream", "ReadableStreamBYOBReader",
    "ReadableStreamBYOBRequest", "ReadableStreamDefaultController", "ReadableStreamDefaultReader",
    "ReferenceError", "Reflect", "RegExp", "Request", "Response", "Set", "SharedArrayBuffer",
    "String", "SubtleCrypto", "SuppressedError", "Symbol", "SyntaxError", "TextDecoder",
    "TextDecoderStream", "TextEncoder", "TextEncoderStream", "TransformStream",
    "TransformStreamDefaultController", "TypeError", "URIError", "URL", "URLPattern",
    "URLSearchParams", "Uint16Array", "Uint32Array", "Uint8Array", "Uint8ClampedArray", "WeakMap",
    "WeakRef", "WeakSet", "WebAssembly", "WebSocket", "WritableStream",
    "WritableStreamDefaultController", "WritableStreamDefaultWriter", "atob", "btoa",
    "clearImmediate", "clearInterval", "clearTimeout", "console", "crypto", "decodeURI",
    "decodeURIComponent", "encodeURI", "encodeURIComponent", "escape", "eval", "fetch", "global",
    "globalThis", "isFinite", "isNaN", "navigator", "parseFloat", "parseInt", "performance",
    "process", "queueMicrotask", "setImmediate", "setInterval", "setTimeout", "structuredClone",
    "undefined", "unescape",
];

/// Reading an unresolvable name: `throw new ReferenceError("x is not defined")` (node's message).
fn not_defined(name: &str) -> Expr {
    Expr::Seq {
        body: vec![Stmt::Throw(Expr::intrinsic(
            Intrinsic::NewError(ErrorKind::ReferenceError),
            vec![Expr::str(&format!("{name} is not defined")), Expr::undefined()],
        ))],
        value: Box::new(Expr::undefined()),
    }
}

fn member_get(m: &Member, obj: Expr, key: Expr) -> Expr {
    match m {
        Member::Private(read, _) => {
            Expr::intrinsic(Intrinsic::PrivateGet, vec![obj, key, Expr::str(read)])
        }
        Member::Super(this) => {
            Expr::intrinsic(Intrinsic::SuperGet, vec![obj, key, Expr::Local(*this)])
        }
        Member::Plain => Expr::intrinsic(Intrinsic::GetProp, vec![obj, key]),
    }
}

/// Write a member (no result).
fn member_set(m: &Member, obj: Expr, key: Expr, v: Expr) -> Expr {
    match m {
        Member::Private(_, write) => {
            Expr::intrinsic(Intrinsic::PrivateSet, vec![obj, key, v, Expr::str(write)])
        }
        Member::Super(this) => {
            Expr::intrinsic(Intrinsic::SuperSet, vec![obj, key, v, Expr::Local(*this)])
        }
        Member::Plain => Expr::intrinsic(Intrinsic::SetProp, vec![obj, key, v]),
    }
}

/// The text of a string literal or a template literal without substitutions.
fn static_string<'x>(e: &'x Expression) -> Option<&'x str> {
    match e.without_parentheses() {
        Expression::StringLiteral(s) => Some(s.value.as_str()),
        Expression::TemplateLiteral(t) if t.expressions.is_empty() => {
            t.quasis[0].value.cooked.as_ref().map(|c| c.as_str())
        }
        _ => None,
    }
}

/// A function's `length`: the parameters before the first default (a rest parameter doesn't count).
fn js_length(params: &FormalParameters) -> u32 {
    params
        .items
        .iter()
        .position(|p| p.initializer.is_some())
        .unwrap_or(params.items.len()) as u32
}

/// JS ToNumber.
fn number_of(v: Val) -> Expr {
    match v.ty {
        Type::F64 => v.e,
        Type::Bool => Expr::Cond {
            cond: Box::new(v.e),
            then: Box::new(Expr::f64(1.0)),
            els: Box::new(Expr::f64(0.0)),
            ty: Type::F64,
        },
        _ => Expr::convert(Type::JsVal, Type::F64, v.e),
    }
}

/// The value as a JS value.
fn jsval_of(v: Val) -> Expr {
    match v.ty {
        Type::JsVal => v.e,
        t => Expr::convert(t, Type::JsVal, v.e),
    }
}

/// JS ToBoolean.
fn cond_of(v: Val) -> Expr {
    match v.ty {
        Type::Bool => v.e,
        Type::F64 => truthy(v.e),
        _ => Expr::convert(Type::JsVal, Type::Bool, v.e),
    }
}

/// Two values that must share a type (branches of `?:`, operands of `&&`/`||`):
/// the type if they agree, otherwise both as JS values.
fn unify(a: Val, b: Val) -> (Expr, Expr, Type, Kind) {
    let kind = a.kind.join(b.kind);
    if a.ty == b.ty {
        let ty = a.ty;
        (a.e, b.e, ty, kind)
    } else {
        (jsval_of(a), jsval_of(b), Type::JsVal, kind)
    }
}

/// JS ToString of `v`.
fn to_str(v: Val) -> Expr {
    match (v.kind, v.ty) {
        (Kind::Str, _) => v.e,
        (_, Type::F64) => Expr::intrinsic(Intrinsic::NumberToString, vec![v.e]),
        _ => Expr::intrinsic(Intrinsic::ToString, vec![jsval_of(v)]),
    }
}

/// JS `l + r`: concatenation if either is known to be a string, addition if both are known
/// to be numbers or booleans, otherwise decided at run time.
fn add(l: Val, r: Val) -> Val {
    if (l.kind == Kind::Str && r.kind == Kind::Any) || (l.kind == Kind::Any && r.kind == Kind::Str)
    {
        // An object operand converts with the "default" hint (`Symbol.toPrimitive`, T25), not
        // ToString's "string": the run-time `+`.
        let e = Expr::intrinsic(Intrinsic::JsAdd, vec![jsval_of(l), jsval_of(r)]);
        Val::js(e, Kind::Str)
    } else if l.kind == Kind::Str || r.kind == Kind::Str {
        let e = Expr::intrinsic(Intrinsic::Concat, vec![to_str(l), to_str(r)]);
        Val::js(e, Kind::Str)
    } else if l.kind == Kind::Bottom || r.kind == Kind::Bottom {
        // Not known yet (could still be a string): any f64 will do, this round is thrown away.
        Val {
            e: Expr::binary(BinOp::Add, Type::F64, number_of(l), number_of(r)),
            ty: Type::F64,
            kind: Kind::Bottom,
        }
    } else if l.kind == Kind::Any || r.kind == Kind::Any {
        let e = Expr::intrinsic(Intrinsic::JsAdd, vec![jsval_of(l), jsval_of(r)]);
        Val::js(e, Kind::Any)
    } else {
        Val::num(Expr::binary(
            BinOp::Add,
            Type::F64,
            number_of(l),
            number_of(r),
        ))
    }
}

/// UTF-16 code units of a string literal's value. With `lone_surrogates`, oxc encodes a lone
/// surrogate as U+FFFD followed by 4 hex digits (and U+FFFD itself as U+FFFD `fffd`).
fn utf16(value: &str, lone_surrogates: bool) -> Vec<u16> {
    if !lone_surrogates {
        return value.encode_utf16().collect();
    }
    let mut out = Vec::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\u{fffd}' {
            let hex: String = chars.by_ref().take(4).collect();
            out.push(u16::from_str_radix(&hex, 16).expect("oxc lone-surrogate escape"));
        } else {
            let mut buf = [0u16; 2];
            out.extend_from_slice(c.encode_utf16(&mut buf));
        }
    }
    out
}

/// The `@param {type} name` and `@returns {type}` tags of a JSDoc comment. Only `string` and
/// `number` are boundary types so far; anything else is recorded as an error.
fn parse_jsdoc(text: &str) -> JsDoc {
    let mut doc = JsDoc::default();
    let mut rest = text;
    while let Some(at) = rest.find('@') {
        rest = &rest[at + 1..];
        let tag: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        rest = &rest[tag.len()..];
        if !matches!(tag.as_str(), "param" | "returns" | "return") {
            continue;
        }
        let after = rest.trim_start();
        let Some(ty_text) = after.strip_prefix('{').and_then(|t| t.split_once('}')) else {
            continue; // no type
        };
        let (ty, tail) = ty_text;
        rest = tail;
        let ty = ty.trim();
        let inner = (tag != "param")
            .then(|| ty.strip_prefix("Promise<")?.strip_suffix('>'))
            .flatten()
            .map(str::trim);
        if matches!(inner, Some("void" | "undefined")) {
            doc.promise = true;
            doc.result = None;
            continue;
        }
        let abi = match inner.unwrap_or(ty) {
            "string" => Some(ir::AbiType::String),
            "number" => Some(ir::AbiType::F64),
            _ => None,
        };
        if inner.is_some() && abi.is_some() {
            doc.promise = true;
        }
        let name: String = tail
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        match (tag.as_str(), abi) {
            ("param", Some(t)) => {
                doc.params.insert(name, t);
            }
            (_, Some(t)) => doc.result = Some(t),
            ("param", None) => doc.errors.push(format!(
                "JSDoc type `{{{ty}}}` of parameter `{name}` at the WIT boundary (only `string` and `number` so far)"
            )),
            (_, None) => doc.errors.push(format!(
                "JSDoc return type `{{{ty}}}` at the WIT boundary (only `string`, `number` and `Promise<string|number|void>` so far)"
            )),
        }
    }
    doc
}

/// `===`/`!==` (`strict`) or `==`/`!=`. Same-typed numbers or booleans compare directly
/// (loose and strict agree then); anything else goes through the JS value helpers.
fn equality(l: Val, r: Val, op: BinOp, strict: bool) -> Expr {
    if l.ty == r.ty && l.ty != Type::JsVal {
        let ty = l.ty;
        return Expr::binary(op, ty, l.e, r.e);
    }
    let op = match (op, strict) {
        (op, true) => op,
        (BinOp::Eq, false) => BinOp::LooseEq,
        (_, false) => BinOp::LooseNe,
    };
    Expr::binary(op, Type::JsVal, jsval_of(l), jsval_of(r))
}

/// JS `a >>> b` on numbers: the unsigned 32-bit result as an `f64`.
fn ushr(cx: &mut FnCx, a: Expr, b: Expr) -> Expr {
    let tmp = cx.func.add_local("u32", Type::F64);
    let signed = from_i32(Expr::binary(BinOp::ShrU, Type::I32, to_i32(a), to_i32(b)));
    // As signed it's negative when bit 31 is set: add 2^32.
    let negative = Expr::binary(
        BinOp::Lt,
        Type::F64,
        Expr::Tee(tmp, Box::new(signed)),
        Expr::f64(0.0),
    );
    Expr::Cond {
        cond: Box::new(negative),
        then: Box::new(Expr::binary(
            BinOp::Add,
            Type::F64,
            Expr::Local(tmp),
            Expr::f64(4294967296.0),
        )),
        els: Box::new(Expr::Local(tmp)),
        ty: Type::F64,
    }
}

fn bool_cond(cond: Expr, then: Expr, els: Expr) -> Expr {
    Expr::Cond {
        cond: Box::new(cond),
        then: Box::new(then),
        els: Box::new(els),
        ty: Type::Bool,
    }
}

/// JS ToBoolean of a number: false for ±0 and NaN.
fn truthy(e: Expr) -> Expr {
    Expr::binary(
        BinOp::Gt,
        Type::F64,
        Expr::unary(UnOp::Abs, Type::F64, e),
        Expr::f64(0.0),
    )
}

/// `Number.<name>` constants (T21).
fn number_constant(name: &str) -> Option<f64> {
    ir::NUMBER_CONSTANTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
}

/// Convert `e` from `from` to `to` (the IR converts directly only to and from `jsval` and
/// between `f64` and `i32`).
fn convert_any(from: Type, to: Type, e: Expr) -> Expr {
    match (from, to) {
        _ if from == to => e,
        (Type::F64, Type::I32)
        | (Type::I32, Type::F64)
        | (Type::F64, Type::JsVal)
        | (Type::JsVal, Type::F64)
        | (Type::JsVal, Type::Bool)
        | (Type::Bool, Type::JsVal) => Expr::convert(from, to, e),
        (Type::I32, _) => convert_any(Type::F64, to, from_i32(e)),
        (_, Type::I32) => to_i32(convert_any(from, Type::F64, e)),
        _ => convert_any(Type::JsVal, to, convert_any(from, Type::JsVal, e)),
    }
}

/// An export's result `e` (IR type `ty`, kind `kind`) as the boundary type `a` takes it
/// ([`ir::AbiType::result_type`]): JS ToNumber / ToInt32 / ToBoolean / ToString.
fn to_boundary(e: Expr, ty: Type, kind: Kind, a: ir::AbiType) -> Expr {
    match a.result_type() {
        // The adapter lowers a compound value (T20).
        Type::JsVal if matches!(a, ir::AbiType::Compound(_)) => convert_any(ty, Type::JsVal, e),
        Type::JsVal => match ty {
            Type::F64 => Expr::intrinsic(ir::Intrinsic::NumberToString, vec![e]),
            Type::JsVal if kind == Kind::Str => e,
            _ => Expr::intrinsic(
                ir::Intrinsic::ToString,
                vec![convert_any(ty, Type::JsVal, e)],
            ),
        },
        t => convert_any(ty, t, e),
    }
}

/// `++`/`--`'s [`ir::NumericOp`] (T40).
fn inc_dec(op: BinOp) -> ir::NumericOp {
    if op == BinOp::Add {
        ir::NumericOp::Inc
    } else {
        ir::NumericOp::Dec
    }
}

fn to_i32(e: Expr) -> Expr {
    Expr::convert(Type::F64, Type::I32, e)
}

fn from_i32(e: Expr) -> Expr {
    Expr::convert(Type::I32, Type::F64, e)
}

fn read(p: Place, ty: Type) -> Expr {
    match p {
        Place::Local(l) => Expr::Local(l),
        Place::Global(g) => Expr::Global(g),
        Place::Cell(l) => Expr::intrinsic(Intrinsic::CellGet(ty), vec![Expr::Local(l)]),
    }
}

fn set(p: Place, ty: Type, e: Expr) -> Stmt {
    match p {
        Place::Local(l) => Stmt::Set(l, e),
        Place::Global(g) => Stmt::SetGlobal(g, e),
        Place::Cell(l) => Stmt::Expr(Expr::intrinsic(
            Intrinsic::CellSet(ty),
            vec![Expr::Local(l), e],
        )),
    }
}

/// `if (!cond) break label`, with `!true` folded away for `while (true)`.
fn exit_unless(cond: Expr, label: LabelId) -> Vec<Stmt> {
    if cond == Expr::bool(true) {
        return Vec::new();
    }
    vec![Stmt::If {
        cond: Expr::unary(UnOp::Not, Type::Bool, cond),
        then: vec![Stmt::Break(label)],
        els: vec![],
    }]
}

/// Wrap a loop body in the `continue` block if any `continue` used it.
fn wrap_continue(kind: TargetKind, body: Vec<Stmt>) -> Vec<Stmt> {
    match kind {
        TargetKind::Wrapped(Some(label)) => vec![Stmt::Block { label, body }],
        _ => body,
    }
}

fn find_target(targets: &[Target], name: Option<&str>, is_continue: bool) -> Option<usize> {
    targets
        .iter()
        .rposition(|t| match name {
            Some(n) => t.js_label.as_deref() == Some(n),
            None if is_continue => !matches!(t.kind, TargetKind::Block | TargetKind::Switch),
            None => !matches!(t.kind, TargetKind::Block),
        })
        .filter(|&i| !is_continue || !matches!(targets[i].kind, TargetKind::Block))
}

/// Does this statement contain `return <expr>` (not looking into nested functions)?
fn has_value_return(s: &Statement) -> bool {
    match s {
        Statement::ReturnStatement(r) => r.argument.is_some(),
        Statement::BlockStatement(b) => b.body.iter().any(has_value_return),
        Statement::IfStatement(i) => {
            has_value_return(&i.consequent) || i.alternate.as_ref().is_some_and(has_value_return)
        }
        Statement::WhileStatement(w) => has_value_return(&w.body),
        Statement::DoWhileStatement(w) => has_value_return(&w.body),
        Statement::ForStatement(f) => has_value_return(&f.body),
        Statement::ForInStatement(f) => has_value_return(&f.body),
        Statement::ForOfStatement(f) => has_value_return(&f.body),
        Statement::LabeledStatement(l) => has_value_return(&l.body),
        Statement::TryStatement(t) => {
            t.block.body.iter().any(has_value_return)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.body.iter().any(has_value_return))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.body.iter().any(has_value_return))
        }
        Statement::SwitchStatement(s) => s
            .cases
            .iter()
            .any(|c| c.consequent.iter().any(has_value_return)),
        _ => false,
    }
}

/// Conservative: true only if control provably can't fall off the end of `list`
/// (it returns or throws).
fn always_returns(list: &[Statement]) -> bool {
    list.iter().any(|s| match s {
        Statement::ReturnStatement(_) | Statement::ThrowStatement(_) => true,
        Statement::TryStatement(t) => {
            t.finalizer
                .as_ref()
                .is_some_and(|f| always_returns(&f.body))
                || (always_returns(&t.block.body)
                    && t.handler
                        .as_ref()
                        .is_none_or(|h| always_returns(&h.body.body)))
        }
        Statement::BlockStatement(b) => always_returns(&b.body),
        Statement::IfStatement(i) => i.alternate.as_ref().is_some_and(|a| {
            always_returns(std::slice::from_ref(&i.consequent))
                && always_returns(std::slice::from_ref(a))
        }),
        _ => false,
    })
}

fn expression_kind(e: &Expression) -> String {
    let dbg = format!("{e:?}");
    dbg.split(['(', ' ', '{'])
        .next()
        .unwrap_or("Expression")
        .to_string()
}

/// The [`ir::MapOp`] of `__intrinsic_<name>` (T25): `map_new_map`, `map_iter_entries`, ….
/// `__intrinsic_typed_*` (T20): see [`ir::TypedOp`].
fn typed_op(name: &str) -> Option<ir::TypedOp> {
    use ir::TypedOp as T;
    Some(match name {
        "typed_new" => T::New,
        "typed_view" => T::View,
        "typed_kind" => T::Kind,
        "typed_length" => T::Length,
        "typed_byte_offset" => T::ByteOffset,
        "typed_buffer" => T::Buffer,
        "is_array_buffer" => T::IsBuffer,
        "buffer_new" => T::BufferNew,
        "buffer_length" => T::BufferLength,
        "buffer_slice" => T::BufferSlice,
        "typed_copy" => T::Copy,
        _ => return None,
    })
}

/// `__intrinsic_stream_*` (T20 slice 6b): see [`ir::StreamOp`].
fn stream_op(name: &str) -> Option<ir::StreamOp> {
    use ir::StreamOp as S;
    Some(match name {
        "stream_read" => S::Read,
        "stream_write" => S::Write,
        "stream_drop_readable" => S::DropReadable,
        "stream_drop_writable" => S::DropWritable,
        _ => return None,
    })
}

fn map_op(name: &str) -> Option<ir::MapOp> {
    use ir::MapOp as M;
    Some(match name.strip_prefix("map_")? {
        "new_map" => M::New(ir::MAP_KIND_MAP),
        "new_set" => M::New(ir::MAP_KIND_SET),
        "kind" => M::Kind,
        "get" => M::Get,
        "has" => M::Has,
        "set" => M::Set,
        "delete" => M::Delete,
        "clear" => M::Clear,
        "size" => M::Size,
        "iter_keys" => M::Iter(ir::MAP_ITER_KEYS),
        "iter_values" => M::Iter(ir::MAP_ITER_VALUES),
        "iter_entries" => M::Iter(ir::MAP_ITER_ENTRIES),
        "iter_kind" => M::IterKind,
        "step" => M::Step,
        "key" => M::Key,
        "value" => M::Value,
        _ => return None,
    })
}

/// An anonymous function or class expression: it takes its `name` from where it's defined.
fn anonymous_function(e: &Expression) -> bool {
    match e.without_parentheses() {
        Expression::FunctionExpression(f) => f.id.is_none(),
        Expression::ArrowFunctionExpression(_) => true,
        Expression::ClassExpression(c) => c.id.is_none(),
        _ => false,
    }
}

/// A host import a library can't make yet (T26: the clock, `Math.random`): throws an `Error`
/// with `msg` (the expression's value, NaN, is never used).
fn library_throws(msg: &str) -> Expr {
    Expr::Seq {
        body: vec![Stmt::Throw(Expr::intrinsic(
            Intrinsic::NewError(ErrorKind::Error),
            vec![Expr::str(msg), Expr::undefined()],
        ))],
        value: Box::new(Expr::f64(f64::NAN)),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn node_globals_are_sorted() {
        assert!(super::NODE_GLOBALS.is_sorted(), "binary_search needs a sorted list");
    }
}

