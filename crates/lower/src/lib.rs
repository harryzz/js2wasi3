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
//! Supported: top-level `function` declarations (exported or not), simple parameters (missing
//! arguments are `undefined`), `var`/`let`/`const`, top-level variables with a literal
//! initializer (they become globals), number/boolean/string/`null` literals, template literals,
//! `undefined`, `NaN`, `Infinity`, `+` (addition or concatenation), `- * / %` (ToNumber),
//! unary `- + ! ~ void typeof`, `| & ^ << >> >>>`, comparisons, `=== !== == !=`, `.length`,
//! `&& || ??`, `?:`, comma, assignment and compound assignment, `++`/`--`, direct calls, `if`,
//! `while`, `do/while`, `for`, labels, `break`, `continue`, `return`.
//! Converting a string to a number (`"5" * 2`, `"a" < "b"`) is rejected when the operand is known
//! to be a string, and traps at run time otherwise (F20).
//!
//! An export whose IR signature differs from its boundary types gets a wrapper `<name>$export`
//! that converts (box an `f64`, ToNumber / ToString of the result).
//!
//! **Commands** (T10): a file without exports is a `wasi:cli/run` command. Its top-level
//! statements (all but function declarations) become a synthetic `main` ([`ir::Module::main`]).
//! Top-level variables stay globals: `main` assigns them where they're declared, and a literal
//! initializer is the global's initial value when no code can run first (a `var` that could be
//! read before its declaration starts as `undefined`). `main` reading a `let`/`const` before its
//! declaration is rejected (TDZ); a function doing so isn't detected yet (F-item, T12).
//! `console.log`/`info`/`debug` (stdout) and `console.error`/`warn` (stderr) take primitives:
//! each is shown like node (`-0` as `-0`), joined by spaces, plus a newline. A library (a file
//! with exports) can't have top-level code or `console` yet (T20).
//!
//! Anything else produces `unsupported: <construct> (planned: <task>)` at its source
//! position. The lowering never guesses: a construct it can't lower exactly is rejected.
//! All diagnostics of a file are collected, not just the first.

use std::collections::{HashMap, HashSet};

use js2wasi3_frontend::{Diagnostic, Diagnostics, Program, line_col, statement_kind};
use js2wasi3_ir::{
    self as ir, BinOp, Const, Expr, FuncId, GlobalId, Intrinsic, LabelId, LocalId, Pos, Stmt, Type,
    TypeofTag, UnOp,
};
use oxc_ast::ast::*;
use oxc_semantic::Scoping;
use oxc_span::{GetSpan, Span};
use oxc_syntax::{
    operator::{
        AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator,
    },
    symbol::SymbolId,
};

/// Lower a whole module. Returns every unsupported construct as a diagnostic.
pub fn lower(program: &Program) -> Result<ir::Module, Diagnostics> {
    let mut facts = Facts::default();
    loop {
        let mut l = Lowerer {
            p: program,
            scoping: program.semantic.scoping(),
            module: ir::Module::default(),
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
            lexical_end: HashMap::new(),
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
}

/// Boundary types from an exported function's JSDoc.
#[derive(Default)]
struct JsDoc {
    params: HashMap<String, ir::AbiType>,
    result: Option<ir::AbiType>,
    /// Unsupported types, reported if the function is exported.
    errors: Vec<String>,
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
    /// Top-level `let`/`const` of a command: where the declaration ends. `main` reading one
    /// before that is a TDZ error.
    lexical_end: HashMap<SymbolId, u32>,
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

/// A `break`/`continue` target.
struct Target {
    js_label: Option<String>,
    brk: LabelId,
    kind: TargetKind,
}

enum TargetKind {
    /// Labelled non-loop statement: only `break label` may target it.
    Block,
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
}

impl FnCx {
    fn label(&mut self) -> LabelId {
        self.next_label += 1;
        LabelId(self.next_label - 1)
    }
}

#[derive(Clone, Copy)]
enum Place {
    Local(LocalId),
    Global(GlobalId),
}

/// A resolved variable: where it lives, its slot, its type and kind.
#[derive(Clone, Copy)]
struct Var {
    place: Place,
    sym: SymbolId,
    ty: Type,
    kind: Kind,
}

impl Var {
    fn val(&self) -> Val {
        Val {
            e: read(self.place),
            ty: self.ty,
            kind: self.kind,
        }
    }
}

impl<'p, 'a> Lowerer<'p, 'a> {
    // ---- diagnostics -------------------------------------------------------------------

    fn unsupported(&mut self, span: Span, what: &str, planned: &str) -> Unsupported {
        let (line, col) = line_col(self.p.source, span.start);
        self.diags.push(Diagnostic {
            path: self.p.path.clone(),
            line,
            col,
            message: format!("unsupported: {what} (planned: {planned})"),
            help: None,
        });
        Unsupported
    }

    fn pos(&self, span: Span) -> Pos {
        let (line, col) = line_col(self.p.source, span.start);
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
        let mut next = Facts {
            kinds: self.facts.kinds.clone(),
            string_params: self.facts.string_params.clone(),
        };
        let mut changed = false;
        for (s, k) in &self.seen {
            let joined = next.kinds.get(s).map_or(*k, |f| f.join(*k));
            if joined != Kind::Bottom && next.kinds.get(s) != Some(&joined) {
                next.kinds.insert(*s, joined);
                changed = true;
            }
        }
        for p in &self.new_string_params {
            changed |= next.string_params.insert(*p);
        }
        changed.then_some(next)
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

    /// JS ToNumber of `v`, rejected for strings (not implemented yet: F20).
    fn num(&mut self, v: Val, span: Span) -> R<Expr> {
        if v.kind == Kind::Str {
            return Err(self.unsupported(span, "converting a string to a number", "T21"));
        }
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
        let body = &self.p.ast.body;
        // Pass 1: declare every top-level function and global so bodies can refer to them in any order.
        let mut fns: Vec<(&'a Function<'a>, FuncId)> = Vec::new();
        // A command's top-level code, in order (everything but function declarations).
        let mut top: Vec<&'a Statement<'a>> = Vec::new();
        for stmt in body.iter() {
            if self.command
                && !matches!(stmt, Statement::FunctionDeclaration(_))
                && stmt.as_module_declaration().is_none()
            {
                if let Statement::VariableDeclaration(d) = stmt {
                    let _ = self.declare_command_globals(d, &top);
                }
                top.push(stmt);
                continue;
            }
            let _ = self.declare_top(stmt, &mut fns);
        }
        if self.command {
            let mut f = ir::Function::new("main", &[], None);
            f.pos = Some(Pos { line: 1, col: 1 });
            self.main = Some(self.module.add_function(f));
            self.module.main = self.main;
            self.func_params.insert(self.main.unwrap(), Vec::new());
        }
        for e in &self.p.exports {
            if e.from.is_none()
                && let Some(Binding::Func(f)) = e.local.as_deref().and_then(|l| self.names.get(l))
            {
                self.exported.insert(*f);
            }
        }
        // Boundary facts: string parameters (JSDoc or earlier usage) hold strings.
        for func in self.exported.clone() {
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
            for e in &doc.errors {
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
        }
        // Pass 3: exports.
        for e in &self.p.exports {
            let _ = self.export(e);
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
                    "T16/T20",
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
            Statement::ImportDeclaration(i) => Err(self.unsupported(
                i.span,
                "import of another module (multi-file modules, F7)",
                "not scheduled",
            )),
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
    fn declare_function(
        &mut self,
        f: &'a Function<'a>,
        doc_at: u32,
        fns: &mut Vec<(&'a Function<'a>, FuncId)>,
    ) -> R<()> {
        let id_node = f.id.as_ref().expect("function declarations have a name");
        if f.r#async {
            return Err(self.unsupported(f.span, "async function", "T19"));
        }
        if f.generator {
            return Err(self.unsupported(f.span, "generator function", "T18"));
        }
        let mut params = Vec::new();
        let mut bad = false;
        for p in &f.params.items {
            match (&p.pattern, &p.initializer) {
                (BindingPattern::BindingIdentifier(b), None) => {
                    params.push((b.name.as_str(), b.symbol_id.get()))
                }
                _ => {
                    self.unsupported(p.span, "default or destructured parameter", "T15");
                    bad = true;
                }
            }
        }
        if let Some(rest) = &f.params.rest {
            self.unsupported(rest.span, "rest parameter", "T15");
            bad = true;
        }
        let body = f.body.as_ref().expect("function declarations have a body");
        let id = FuncId(self.module.functions.len() as u32);
        let result = Slot::Result(id);
        // A function has a result if it returns a value, or if its (undefined) result is used.
        let returns_value =
            body.statements.iter().any(has_value_return) || self.slot_ty(result) == Type::JsVal;
        let sig: Vec<(&str, Type)> = params
            .iter()
            .map(|(n, sym)| {
                let ty = sym.map_or(Type::F64, |s| self.slot_ty(Slot::Var(s)));
                (*n, ty)
            })
            .collect();
        let mut func = ir::Function::new(
            id_node.name.as_str(),
            &sig,
            returns_value.then(|| self.slot_ty(result)),
        );
        func.pos = Some(self.pos(f.span));
        self.module.add_function(func);
        if let Some(text) = self.p.jsdoc(doc_at) {
            self.jsdoc.insert(id, parse_jsdoc(text));
        }
        self.func_params
            .insert(id, params.iter().map(|(_, s)| *s).collect());
        if let Some(sym) = id_node.symbol_id.get() {
            self.funcs.insert(sym, id);
        }
        self.names
            .insert(id_node.name.to_string(), Binding::Func(id));
        if bad {
            return Err(Unsupported);
        }
        fns.push((f, id));
        Ok(())
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
        let mut result = Ok(());
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(b) = &decl.id else {
                result = Err(self.unsupported(decl.span, "destructuring declaration", "T15"));
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

    /// `main`: a command's top-level code. Top-level variables are globals set in place.
    fn lower_main(&mut self, id: FuncId, top: &[&'a Statement<'a>]) {
        let func = self.module.functions[id.0 as usize].clone();
        let mut cx = FnCx {
            id,
            func,
            locals: HashMap::new(),
            next_label: 0,
            targets: Vec::new(),
            var_undefined: Vec::new(),
        };
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
        let mut result = Ok(());
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(b) = &decl.id else {
                continue; // reported when declared
            };
            let sym = b
                .symbol_id
                .get()
                .expect("semantic analysis assigns symbols");
            let g = self.globals[&sym];
            let global = &self.module.globals[g.0 as usize];
            let (ty, init) = (global.ty, global.init);
            let Some(e) = &decl.init else {
                continue; // `undefined` from the start
            };
            if init != Const::Undefined && self.literal(e).is_some() {
                continue; // the literal is the initial value
            }
            let v = match self.expr(cx, e) {
                Ok(v) => v,
                Err(e) => {
                    result = Err(e);
                    continue;
                }
            };
            let e = self.store(Slot::Var(sym), ty, v);
            out.push(Stmt::SetGlobal(g, e));
        }
        result
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

    /// Set each export's boundary signature. Where the function's IR signature differs, the
    /// export goes through a wrapper `<name>$export` that converts.
    fn wrap_exports(&mut self) {
        use ir::AbiType;
        let mut done: HashMap<FuncId, (FuncId, ir::AbiSig)> = HashMap::new();
        for i in 0..self.module.exports.len() {
            let func = self.module.exports[i].func;
            if let Some((w, sig)) = done.get(&func) {
                self.module.exports[i].func = *w;
                self.module.exports[i].sig = sig.clone();
                continue;
            }
            let f = self.module.function(func).clone();
            let params = &f.locals[..f.params as usize];
            let syms = &self.func_params[&func];
            let doc = self.jsdoc.get(&func);
            let result_kind = self.slot_kind(Slot::Result(func));
            let sig = ir::AbiSig {
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
                result: f.result.map(|_| match doc.and_then(|d| d.result) {
                    Some(t) => t,
                    None if result_kind == Kind::Str => AbiType::String,
                    None => AbiType::F64,
                }),
            };
            let direct = params
                .iter()
                .zip(&sig.params)
                .all(|(l, a)| l.ty == a.ir_type())
                && f.result == sig.result.map(|a| a.ir_type())
                && (sig.result != Some(AbiType::String) || result_kind == Kind::Str);
            let target = if direct {
                func
            } else {
                let wsig: Vec<(&str, Type)> = params
                    .iter()
                    .zip(&sig.params)
                    .map(|(l, a)| (l.name.as_str(), a.ir_type()))
                    .collect();
                let mut w = ir::Function::new(
                    format!("{}$export", f.name),
                    &wsig,
                    sig.result.map(|a| a.ir_type()),
                );
                w.pos = f.pos;
                let args = params
                    .iter()
                    .zip(&sig.params)
                    .enumerate()
                    .map(|(j, (l, a))| {
                        let p = Expr::local(LocalId(j as u32));
                        if l.ty == a.ir_type() {
                            p
                        } else {
                            Expr::convert(a.ir_type(), l.ty, p)
                        }
                    })
                    .collect();
                let call = Expr::Call { func, args };
                w.body = vec![match (sig.result, f.result) {
                    (None, _) | (_, None) => Stmt::Expr(call),
                    (Some(AbiType::F64), Some(Type::F64)) => Stmt::Return(Some(call)),
                    (Some(AbiType::F64), Some(_)) => {
                        Stmt::Return(Some(Expr::convert(Type::JsVal, Type::F64, call)))
                    }
                    (Some(AbiType::String), Some(Type::F64)) => Stmt::Return(Some(
                        Expr::intrinsic(ir::Intrinsic::NumberToString, vec![call]),
                    )),
                    (Some(AbiType::String), Some(_)) if result_kind == Kind::Str => {
                        Stmt::Return(Some(call))
                    }
                    (Some(AbiType::String), Some(_)) => {
                        Stmt::Return(Some(Expr::intrinsic(ir::Intrinsic::ToString, vec![call])))
                    }
                }];
                self.module.add_function(w)
            };
            self.module.exports[i].func = target;
            self.module.exports[i].sig = sig.clone();
            done.insert(func, (target, sig));
        }
    }

    // ---- functions ---------------------------------------------------------------------

    fn lower_function(&mut self, f: &'a Function<'a>, id: FuncId) -> R<()> {
        let func = self.module.functions[id.0 as usize].clone();
        let mut cx = FnCx {
            id,
            func,
            locals: HashMap::new(),
            next_label: 0,
            targets: Vec::new(),
            var_undefined: Vec::new(),
        };
        for (i, p) in f.params.items.iter().enumerate() {
            if let BindingPattern::BindingIdentifier(b) = &p.pattern
                && let Some(sym) = b.symbol_id.get()
            {
                cx.locals.insert(sym, cx.func.param(i as u32));
            }
        }
        let body = f.body.as_ref().expect("function declarations have a body");
        let before = self.diags.len();
        let mut stmts = self.stmts(&mut cx, &body.statements);
        if let Some(ty) = cx.func.result
            && !always_returns(&body.statements)
        {
            // Falling off the end returns `undefined`.
            let v = self.undefined_val();
            let e = self.store(Slot::Result(id), ty, v);
            stmts.push(Stmt::Return(Some(e)));
        }
        if self.diags.len() > before {
            return Err(Unsupported);
        }
        let mut body: Vec<Stmt> = cx
            .var_undefined
            .iter()
            .map(|l| Stmt::Set(*l, Expr::undefined()))
            .collect();
        body.extend(stmts);
        cx.func.body = body;
        self.module.functions[id.0 as usize] = cx.func;
        Ok(())
    }

    // ---- statements --------------------------------------------------------------------

    /// Lower a statement list, continuing after errors so every problem is reported.
    fn stmts(&mut self, cx: &mut FnCx, list: &'a [Statement<'a>]) -> Vec<Stmt> {
        let mut out = Vec::new();
        for s in list {
            let _ = self.stmt(cx, s, &mut out);
        }
        out
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
                            Some(a) => self.expr(cx, a)?,
                            None => self.undefined_val(),
                        };
                        Some(self.store(Slot::Result(cx.id), ty, v))
                    }
                };
                out.push(Stmt::Return(v));
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
            Statement::WhileStatement(w) => self.while_loop(cx, None, w, out),
            Statement::DoWhileStatement(w) => self.do_while_loop(cx, None, w, out),
            Statement::ForStatement(f) => self.for_loop(cx, None, f, out),
            Statement::LabeledStatement(l) => {
                let name = Some(l.label.name.to_string());
                match &l.body {
                    Statement::WhileStatement(w) => self.while_loop(cx, name, w, out),
                    Statement::DoWhileStatement(w) => self.do_while_loop(cx, name, w, out),
                    Statement::ForStatement(f) => self.for_loop(cx, name, f, out),
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
                        "`break` outside a loop (e.g. in `switch`)",
                        "later",
                    ));
                };
                out.push(Stmt::Break(cx.targets[t].brk));
                Ok(())
            }
            Statement::ContinueStatement(c) => {
                let name = c.label.as_ref().map(|l| l.name.as_str());
                let t = find_target(&cx.targets, name, true)
                    .expect("semantic analysis checks continue targets");
                let jump = match cx.targets[t].kind {
                    TargetKind::While => Stmt::Continue(cx.targets[t].brk),
                    TargetKind::Wrapped(Some(l)) => Stmt::Break(l),
                    TargetKind::Wrapped(None) => {
                        let l = cx.label();
                        cx.targets[t].kind = TargetKind::Wrapped(Some(l));
                        Stmt::Break(l)
                    }
                    TargetKind::Block => unreachable!("find_target skips blocks for continue"),
                };
                out.push(jump);
                Ok(())
            }
            Statement::FunctionDeclaration(f) => {
                Err(self.unsupported(f.span, "nested function (closures)", "T14"))
            }
            other => Err(self.unsupported(
                other.span(),
                &format!("statement `{}`", statement_kind(other)),
                "later",
            )),
        }
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
                result = Err(self.unsupported(decl.span, "destructuring declaration", "T15"));
                continue;
            };
            let sym = b
                .symbol_id
                .get()
                .expect("semantic analysis assigns symbols");
            let slot = Slot::Var(sym);
            let v = match &decl.init {
                Some(init) => self.expr(cx, init),
                None => Ok(self.undefined_val()),
            };
            let ty = self.slot_ty(slot);
            // Declared even if the initializer failed, so later uses don't report more errors.
            let local = match cx.locals.get(&sym) {
                Some(l) => *l,
                None => {
                    let l = cx.func.add_local(b.name.as_str(), ty);
                    cx.locals.insert(sym, l);
                    l
                }
            };
            let v = match v {
                Ok(v) => v,
                Err(e) => {
                    result = Err(e);
                    continue;
                }
            };
            let v = self.store(slot, ty, v);
            if decl.init.is_none() && d.kind == VariableDeclarationKind::Var {
                // `var x;` doesn't reset `x`: it's `undefined` from function entry on.
                if !cx.var_undefined.contains(&local) {
                    cx.var_undefined.push(local);
                }
                continue;
            }
            out.push(Stmt::Set(local, v));
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
        match &f.init {
            None => {}
            Some(ForStatementInit::VariableDeclaration(d)) => result = self.var_decl(cx, d, out),
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
            Expression::AssignmentExpression(a) => {
                let (var, value) = self.assignment(cx, a)?;
                out.push(set(var.place, value));
                Ok(())
            }
            Expression::UpdateExpression(u) => {
                let (var, value) = self.update(cx, u)?;
                out.push(set(var.place, value));
                Ok(())
            }
            Expression::CallExpression(c) => {
                let call = self.call(cx, c)?;
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
                if let Some(c) = self.global_constant(id) {
                    return Ok(val(Expr::Const(c), c.ty()));
                }
                let var = self.resolve(cx, id)?;
                Ok(var.val())
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
                UnaryOperator::Delete => Err(self.unsupported(span, "operator `delete`", "T13")),
                op => {
                    let v = self.expr(cx, &u.argument)?;
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
            Expression::AssignmentExpression(a) => {
                let (var, value) = self.assignment(cx, a)?;
                Ok(Val {
                    e: assign_value(var, value),
                    ..var.val()
                })
            }
            Expression::UpdateExpression(u) if u.prefix => {
                let (var, value) = self.update(cx, u)?;
                Ok(Val {
                    e: assign_value(var, value),
                    ..var.val()
                })
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
                let body = vec![
                    Stmt::Set(old, self.num(var.val(), span)?),
                    set(var.place, new),
                ];
                let value = Box::new(Expr::Local(old));
                Ok(val(Expr::Seq { body, value }, Type::F64))
            }
            Expression::CallExpression(c) => {
                let call = self.call(cx, c)?;
                let &Expr::Call { func, .. } = &call else {
                    // `console.log(…)` as a value: `undefined`.
                    return Ok(Val::js(
                        Expr::Seq {
                            body: vec![Stmt::Expr(call)],
                            value: Box::new(Expr::undefined()),
                        },
                        Kind::Any,
                    ));
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
            Expression::StaticMemberExpression(m) if m.property.name == "length" && !m.optional => {
                self.hint_string(cx, &m.object);
                let obj = self.expr(cx, &m.object)?;
                let kind = obj.kind;
                let len = Expr::intrinsic(Intrinsic::Length, vec![jsval_of(obj)]);
                Ok(match kind {
                    Kind::Str => Val::num(Expr::convert(Type::JsVal, Type::F64, len)),
                    // Not known yet: any f64 will do, this round is thrown away.
                    Kind::Bottom => Val {
                        e: Expr::convert(Type::JsVal, Type::F64, len),
                        ty: Type::F64,
                        kind: Kind::Bottom,
                    },
                    _ => Val::js(len, Kind::Any),
                })
            }
            Expression::BigIntLiteral(_) => Err(self.unsupported(span, "BigInt", "not scheduled")),
            Expression::ObjectExpression(_) => Err(self.unsupported(span, "object literal", "T13")),
            Expression::ArrayExpression(_) => Err(self.unsupported(span, "array literal", "T15")),
            Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {
                Err(self.unsupported(span, "function expression (closures)", "T14"))
            }
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => {
                Err(self.unsupported(span, "property access", "T13"))
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
        let l = self.expr(cx, &b.left);
        let r = self.expr(cx, &b.right);
        let (l, r) = (l?, r?);
        let num = Val::num;
        let bitwise = match b.operator {
            B::BitwiseOR => Some(BinOp::Or),
            B::BitwiseAnd => Some(BinOp::And),
            B::BitwiseXOR => Some(BinOp::Xor),
            B::ShiftLeft => Some(BinOp::Shl),
            B::ShiftRight => Some(BinOp::Shr),
            _ => None,
        };
        if let Some(op) = bitwise {
            let (l, r) = (to_i32(self.num(l, span)?), to_i32(self.num(r, span)?));
            return Ok(num(from_i32(Expr::binary(op, Type::I32, l, r))));
        }
        if b.operator == B::ShiftRightZeroFill {
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
        // Arithmetic and relational operators on primitives: ToNumber on both operands.
        // (`<` on two strings compares them as strings: F20.)
        let op = match b.operator {
            B::Subtraction => BinOp::Sub,
            B::Multiplication => BinOp::Mul,
            B::Division => BinOp::Div,
            B::Remainder => BinOp::Rem,
            B::LessThan => BinOp::Lt,
            B::LessEqualThan => BinOp::Le,
            B::GreaterThan => BinOp::Gt,
            B::GreaterEqualThan => BinOp::Ge,
            _ => {
                let planned = match b.operator {
                    B::Exponential => "T21 (Math.pow)",
                    _ => "T13",
                };
                return Err(self.unsupported(
                    span,
                    &format!("operator `{}`", b.operator.as_str()),
                    planned,
                ));
            }
        };
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

    fn call(&mut self, cx: &mut FnCx, c: &'a CallExpression<'a>) -> R<Expr> {
        let span = c.span;
        if c.optional {
            return Err(self.unsupported(span, "optional call `?.()`", "T14"));
        }
        if let Some(stderr) = self.console_method(&c.callee) {
            return self.console(cx, c, stderr);
        }
        let Expression::Identifier(id) = &c.callee else {
            return Err(self.unsupported(
                c.callee.span(),
                "call of something other than a named function",
                "T14",
            ));
        };
        let sym = self.scoping.get_reference(id.reference_id()).symbol_id();
        let Some(func) = sym.and_then(|s| self.funcs.get(&s)).copied() else {
            let what = match sym {
                None => format!("call of global `{}`", id.name),
                Some(_) => format!("call of `{}`, which is not a top-level function", id.name),
            };
            return Err(self.unsupported(span, &what, "T14/T21"));
        };
        let params = self.func_params[&func].clone();
        if c.arguments.len() > params.len() {
            return Err(self.unsupported(
                span,
                &format!(
                    "call with {} argument(s) to a function with {} parameter(s)",
                    c.arguments.len(),
                    params.len()
                ),
                "later",
            ));
        }
        let mut args = Vec::new();
        let mut result = Ok(());
        for (i, sym) in params.iter().enumerate() {
            // A missing argument is `undefined`.
            let v = match c.arguments.get(i) {
                None => self.undefined_val(),
                Some(a) => {
                    let Some(e) = a.as_expression() else {
                        result = Err(self.unsupported(a.span(), "spread argument", "T15"));
                        continue;
                    };
                    match self.expr(cx, e) {
                        Ok(v) => v,
                        Err(e) => {
                            result = Err(e);
                            continue;
                        }
                    }
                }
            };
            let ty = self.module.function(func).locals[i].ty;
            let sym = sym.expect("parameters have symbols");
            args.push(self.store(Slot::Var(sym), ty, v));
        }
        result?;
        Ok(Expr::Call { func, args })
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
        let mut parts = Vec::new();
        let mut result = Ok(());
        for (i, a) in c.arguments.iter().enumerate() {
            let Some(e) = a.as_expression() else {
                result = Err(self.unsupported(a.span(), "spread argument", "T15"));
                continue;
            };
            let v = match self.expr(cx, e) {
                Ok(v) => v,
                Err(e) => {
                    result = Err(e);
                    continue;
                }
            };
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
        Ok(Expr::intrinsic(
            Intrinsic::ConsoleWrite { stderr },
            vec![text],
        ))
    }

    /// `x = e` / `x op= e`: the variable and the converted value to store.
    fn assignment(&mut self, cx: &mut FnCx, a: &'a AssignmentExpression<'a>) -> R<(Var, Expr)> {
        let AssignmentTarget::AssignmentTargetIdentifier(id) = &a.left else {
            return Err(self.unsupported(
                a.left.span(),
                "assignment to something other than a variable",
                "T13/T15",
            ));
        };
        let var = self.resolve(cx, id)?;
        let rhs = self.expr(cx, &a.right)?;
        let span = a.span;
        use AssignmentOperator as A;
        let arith = match a.operator {
            A::Subtraction => Some(BinOp::Sub),
            A::Multiplication => Some(BinOp::Mul),
            A::Division => Some(BinOp::Div),
            A::Remainder => Some(BinOp::Rem),
            _ => None,
        };
        let bitwise = match a.operator {
            A::BitwiseOR => Some(BinOp::Or),
            A::BitwiseAnd => Some(BinOp::And),
            A::BitwiseXOR => Some(BinOp::Xor),
            A::ShiftLeft => Some(BinOp::Shl),
            A::ShiftRight => Some(BinOp::Shr),
            _ => None,
        };
        let num = Val::num;
        let value = if a.operator == A::Assign {
            rhs
        } else if a.operator == A::Addition {
            if var.kind == Kind::Str {
                self.hint_string(cx, &a.right);
            }
            add(var.val(), rhs)
        } else if let Some(op) = arith {
            let (l, r) = (self.num(var.val(), span)?, self.num(rhs, span)?);
            num(Expr::binary(op, Type::F64, l, r))
        } else if let Some(op) = bitwise {
            let (l, r) = (self.num(var.val(), span)?, self.num(rhs, span)?);
            num(from_i32(Expr::binary(op, Type::I32, to_i32(l), to_i32(r))))
        } else if a.operator == A::ShiftRightZeroFill {
            let (l, r) = (self.num(var.val(), span)?, self.num(rhs, span)?);
            num(ushr(cx, l, r))
        } else {
            return Err(self.unsupported(
                span,
                &format!("operator `{}`", a.operator.as_str()),
                "later",
            ));
        };
        Ok((var, self.store(Slot::Var(var.sym), var.ty, value)))
    }

    fn update_target(&mut self, cx: &mut FnCx, u: &'a UpdateExpression<'a>) -> R<(Var, BinOp)> {
        let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &u.argument else {
            return Err(self.unsupported(
                u.argument.span(),
                "`++`/`--` on something other than a variable",
                "T13",
            ));
        };
        let var = self.resolve(cx, id)?;
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
        let old = self.num(var.val(), u.span)?;
        let new = Val::num(Expr::binary(op, Type::F64, old, Expr::f64(1.0)));
        Ok((var, self.store(Slot::Var(var.sym), var.ty, new)))
    }

    fn resolve(&mut self, cx: &FnCx, id: &IdentifierReference<'a>) -> R<Var> {
        let span = id.span;
        let Some(sym) = self.scoping.get_reference(id.reference_id()).symbol_id() else {
            return Err(self.unsupported(span, &format!("global `{}`", id.name), "T21"));
        };
        let place = if let Some(l) = cx.locals.get(&sym) {
            Some((Place::Local(*l), cx.func.local(*l).ty))
        } else {
            self.globals
                .get(&sym)
                .map(|g| (Place::Global(*g), self.module.globals[g.0 as usize].ty))
        };
        if cx.id == self.main.unwrap_or(FuncId(u32::MAX))
            && self
                .lexical_end
                .get(&sym)
                .is_some_and(|end| span.start < *end)
        {
            return Err(self.unsupported(
                span,
                &format!(
                    "`{}` used before its `let`/`const` declaration (a ReferenceError at run time)",
                    id.name
                ),
                "T12",
            ));
        }
        if let Some((place, ty)) = place {
            // f64 slots are `Num` or `Bottom`, matching their type.
            let kind = self.slot_kind(Slot::Var(sym));
            return Ok(Var {
                place,
                sym,
                ty,
                kind,
            });
        }
        if self.funcs.contains_key(&sym) {
            return Err(self.unsupported(
                span,
                &format!("function `{}` used as a value", id.name),
                "T14",
            ));
        }
        Err(self.unsupported(
            span,
            &format!("`{}` used before its declaration", id.name),
            "later",
        ))
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
}

// ---- conversions -----------------------------------------------------------------------

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
    if l.kind == Kind::Str || r.kind == Kind::Str {
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
        let abi = match ty {
            "string" => Some(ir::AbiType::String),
            "number" => Some(ir::AbiType::F64),
            _ => None,
        };
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
                "JSDoc return type `{{{ty}}}` at the WIT boundary (only `string` and `number` so far)"
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

fn to_i32(e: Expr) -> Expr {
    Expr::convert(Type::F64, Type::I32, e)
}

fn from_i32(e: Expr) -> Expr {
    Expr::convert(Type::I32, Type::F64, e)
}

fn read(p: Place) -> Expr {
    match p {
        Place::Local(l) => Expr::Local(l),
        Place::Global(g) => Expr::Global(g),
    }
}

fn set(p: Place, e: Expr) -> Stmt {
    match p {
        Place::Local(l) => Stmt::Set(l, e),
        Place::Global(g) => Stmt::SetGlobal(g, e),
    }
}

/// An assignment used as a value: the stored value.
fn assign_value(var: Var, value: Expr) -> Expr {
    match var.place {
        Place::Local(l) => Expr::Tee(l, Box::new(value)),
        Place::Global(g) => Expr::Seq {
            body: vec![Stmt::SetGlobal(g, value)],
            value: Box::new(Expr::Global(g)),
        },
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

/// Conservative: true only if control provably can't fall off the end of `list`.
fn always_returns(list: &[Statement]) -> bool {
    list.iter().any(|s| match s {
        Statement::ReturnStatement(_) => true,
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
