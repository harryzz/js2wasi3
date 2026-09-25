//! Backend: IR → core Wasm bytes (WasmGC types, exception tag, linear memory for the ABI only).
//!
//! v0 (T06) handles the numbers-only IR from T05: `f64`/`i32`/`bool` locals and globals,
//! structured control flow, direct calls and exports. T08 adds `jsval` (see [`runtime`]).
//!
//! # Mapping
//! - `Bool` is an `i32` holding 0 or 1.
//! - `Block L { .. }` → `block`. `Loop L { .. }` → `block $brk (loop $cont ..)`: `Break L` branches
//!   to `$brk`, `Continue L` to `$cont`. `if` also opens a label level, which depth computation accounts for.
//! - `Throw e` → `throw $js`. `Try { body, catch_local, catch_body }` →
//!   `block (block (result jsval) (try_table (catch $js 0) body) (br 1)) (local.set catch_local) catch_body`:
//!   no `catch_all`, so Wasm traps are never caught (design D2).
//! - (T14) `Closure` → `ref.func` of its code, the environment as an `array.new_fixed $vals`, then
//!   `$new_function` (see [`func`]); a shared closure (a top-level function) is made once into its
//!   own global (after the module's globals, before the strings). `CallRef` → the arguments in a
//!   `$vals` array and `$call_value` (`call_ref $code`). Cells are `$cell_f64`/`$cell` structs; the
//!   IR carries them as `jsval`, so cell and environment accesses start with a `ref.cast`. Functions
//!   with the `$code` signature get the runtime's `$code` type, and every `ref.func` target is in a
//!   declarative element segment.
//! - A function with a result ends in `unreachable`: the lowering guarantees every path returns,
//!   but Wasm validation can't see through an `if/else` whose two arms both `return`.
//! - Runtime helpers are emitted after the user functions, and only when used:
//!   - `$js_rem(x, y)`: JS `%` on numbers, i.e. an exact `fmod` (a port of musl's bit-level algorithm).
//!     The shortcut `x - trunc(x/y)*y` is inexact for large quotients.
//!   - `$to_int32(x)`: JS `ToInt32`. `i64.trunc_sat_f64_s` + wrap when |x| < 2^63, otherwise
//!     `fmod(x, 2^32)` first. NaN and ±∞ give 0.
//!
//!   - JS value helpers (`$box_number`, `$to_number`, `$strict_eq`, …): see [`runtime`].
//! - A module that uses `jsval` starts its type section with the runtime `rec` group and its
//!   global section with the four singletons, so user types and globals are offset by those.
//!
//! Unsupported IR (placeholders) is reported with the IR kind, the function and
//! its source position before any bytes are emitted.

mod abi;
mod func;
mod host;
mod numstr;
mod object;
mod runtime;

use std::collections::HashMap;

use anyhow::{Context, bail};
use js2wasi3_ir::{self as ir, AbiType, BinOp, Const, Expr, Intrinsic, LabelId, Stmt, Type, UnOp};
use wasm_encoder::{
    BlockType, Catch, CodeSection, ConstExpr, DataCountSection, DataSection, ElementSection,
    Elements, ExportKind, ExportSection, Function, FunctionSection, GlobalSection, GlobalType,
    ImportSection, IndirectNameMap, InstructionSink, MemorySection, MemoryType, NameMap,
    NameSection, TagKind, TagSection, TagType, TypeSection, ValType,
};

/// Emit a core Wasm module for `module`.
pub fn emit(module: &ir::Module) -> anyhow::Result<Vec<u8>> {
    if let Err(errs) = ir::validate(module) {
        bail!(
            "internal error: invalid IR from the lowering:\n  {}",
            errs.join("\n  ")
        );
    }
    check_supported(module)?;

    // A command (`wasi:cli/run`) always has the JS runtime and memory (for output buffers).
    let command = module.main;
    let js = uses_jsval(module) || command.is_some();
    // Exports with a string at the boundary go through an ABI adapter (and need memory).
    let adapted: Vec<&ir::Export> = module
        .exports
        .iter()
        .filter(|x| {
            x.sig.params.contains(&AbiType::String)
                || x.sig.result == Some(AbiType::String)
                || x.sig.throws
        })
        .collect();
    let abi = !adapted.is_empty() || command.is_some();
    for x in &adapted {
        // More flat params than this are passed through memory: not implemented (F21).
        if abi::flat_params(&x.sig.params).len() > abi::MAX_FLAT_PARAMS {
            bail!(
                "unsupported: export `{}` has more than {} parameter values at the boundary (a string counts twice) (planned: F21)",
                x.name,
                abi::MAX_FLAT_PARAMS
            );
        }
    }
    let mut types = TypeSection::new();
    if js {
        runtime::types(&mut types);
    }
    let n = module.functions.len() as u32;
    // Imported functions come first in the function index space.
    let func_base = if command.is_some() {
        host::IMPORTS.len() as u32
    } else {
        0
    };
    // Globals: runtime, ABI bump pointer, host globals, then the module's own.
    let host_base = runtime::GLOBALS + abi as u32;
    let host_globals = if command.is_some() { host::GLOBALS } else { 0 };
    // Top-level function objects (shared closures): one global each, after the module's globals.
    let mut shared = Vec::new();
    for f in &module.functions {
        visit_exprs(&f.body, &mut |x| {
            if let Expr::Closure(c) = x
                && c.shared
                && !shared.contains(&c.code)
            {
                shared.push(c.code);
            }
        });
    }
    let type_base = if js { runtime::TYPES } else { 0 };
    let mut e = Emitter {
        m: module,
        types,
        next_type: type_base,
        global_base: if js { host_base + host_globals } else { 0 },
        bump: runtime::GLOBALS,
        host_base,
        func_base,
        type_ids: HashMap::new(),
        helper_base: func_base + n + adapted.len() as u32,
        helpers: Vec::new(),
        strings: Vec::new(),
        string_ids: HashMap::new(),
        tables: false,
        shared,
        declared: Vec::new(),
    };
    if js {
        // Functions with the `$code` signature use the runtime's type (`call_ref` needs it).
        let v = runtime::jsval();
        e.type_ids.insert((vec![v, v, v], vec![v]), runtime::T_CODE);
    }

    let mut imports = ImportSection::new();
    if command.is_some() {
        host::imports(&mut imports, &mut |p, r| e.type_id(p, r));
    }
    // The exception tag (`runtime::TAG`) comes with the JS runtime.
    let mut tags = TagSection::new();
    if js {
        tags.tag(TagType {
            kind: TagKind::Exception,
            func_type_idx: e.type_id(vec![runtime::jsval()], vec![]),
        });
    }
    let mut funcs = FunctionSection::new();
    let mut code = CodeSection::new();
    for f in &module.functions {
        let params: Vec<ValType> = f.locals[..f.params as usize]
            .iter()
            .map(|l| val_type(l.ty))
            .collect();
        let results: Vec<ValType> = f.result.map(val_type).into_iter().collect();
        funcs.function(e.type_id(params, results));
        code.function(
            &e.function(f)
                .with_context(|| format!("in function {}", f.name))?,
        );
    }
    let mut adapter_names = Vec::new();
    for x in &adapted {
        let params = abi::flat_params(&x.sig.params);
        let results = abi::flat_result(&x.sig);
        funcs.function(e.type_id(params, results));
        let bump = e.bump;
        code.function(&abi::adapter(&mut e, bump, func_base + x.func.0, &x.sig));
        adapter_names.push(format!("{}$abi", x.name));
    }
    let realloc =
        abi.then(|| e.helper_index(Helper::Js(runtime::Helper::Abi(abi::Helper::Realloc))));
    let run = command.map(|_| {
        (
            e.helper_index(Helper::Js(runtime::Helper::Host(host::Helper::Run))),
            e.helper_index(Helper::Js(runtime::Helper::Host(host::Helper::Callback))),
        )
    });
    // Helpers can request other helpers, so iterate until the list stops growing.
    let mut i = 0;
    while i < e.helpers.len() {
        let h = e.helpers[i];
        let (params, results) = h.signature();
        funcs.function(e.type_id(params, results));
        code.function(&e.helper(h));
        i += 1;
    }

    let mut globals = GlobalSection::new();
    if js {
        runtime::globals(&mut globals);
    }
    if abi {
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(abi::HEAP_BASE),
        );
    }
    for _ in 0..host_globals {
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(0),
        );
    }
    for g in &module.globals {
        let init = match (g.ty, g.init) {
            (Type::JsVal, Const::F64(v)) => runtime::const_number(v),
            (Type::JsVal, Const::Bool(v)) => {
                ConstExpr::global_get(if v { runtime::G_TRUE } else { runtime::G_FALSE })
            }
            (_, Const::Undefined) => ConstExpr::global_get(runtime::G_UNDEFINED),
            (_, Const::Null) => ConstExpr::global_get(runtime::G_NULL),
            (_, Const::F64(v)) => ConstExpr::f64_const(v.into()),
            (_, Const::I32(v)) => ConstExpr::i32_const(v),
            (_, Const::Bool(v)) => ConstExpr::i32_const(v as i32),
        };
        globals.global(
            GlobalType {
                val_type: val_type(g.ty),
                mutable: g.mutable,
                shared: false,
            },
            &init,
        );
    }

    for _ in &e.shared {
        globals.global(
            GlobalType {
                val_type: runtime::jsval(),
                mutable: true,
                shared: false,
            },
            &ConstExpr::ref_null(wasm_encoder::HeapType::Abstract {
                shared: false,
                ty: wasm_encoder::AbstractHeapType::Eq,
            }),
        );
    }
    for units in &e.strings {
        runtime::string_global(&mut globals, units);
    }

    let mut exports = ExportSection::new();
    let mut next_adapter = func_base + n;
    for x in &module.exports {
        let index = if adapted.iter().any(|a| std::ptr::eq(*a, x)) {
            next_adapter += 1;
            next_adapter - 1
        } else {
            func_base + x.func.0
        };
        exports.export(&x.name, ExportKind::Func, index);
    }
    if let Some((run, callback)) = run {
        exports.export("[async-lift]wasi:cli/run@0.3.1#run", ExportKind::Func, run);
        exports.export(
            "[callback][async-lift]wasi:cli/run@0.3.1#run",
            ExportKind::Func,
            callback,
        );
    }
    let mut memory = MemorySection::new();
    if let Some(realloc) = realloc {
        memory.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        exports.export("memory", ExportKind::Memory, 0);
        exports.export("cabi_realloc", ExportKind::Func, realloc);
    }

    let mut names = NameSection::new();
    let mut fnames = NameMap::new();
    let mut lnames = IndirectNameMap::new();
    if command.is_some() {
        for (i, imp) in host::IMPORTS.iter().enumerate() {
            let (module, field) = imp.names();
            fnames.append(i as u32, &format!("{module}#{field}"));
        }
    }
    for (i, f) in module.functions.iter().enumerate() {
        fnames.append(func_base + i as u32, &f.name);
        let mut locals = NameMap::new();
        for (j, l) in f.locals.iter().enumerate() {
            locals.append(j as u32, &l.name);
        }
        lnames.append(func_base + i as u32, &locals);
    }
    for (k, name) in adapter_names.iter().enumerate() {
        fnames.append(func_base + n + k as u32, name);
    }
    for (k, h) in e.helpers.iter().enumerate() {
        fnames.append(e.helper_base + k as u32, h.name());
    }
    names.functions(&fnames);
    names.locals(&lnames);

    let mut out = wasm_encoder::Module::new();
    out.section(&e.types);
    if command.is_some() {
        out.section(&imports);
    }
    out.section(&funcs);
    if abi {
        out.section(&memory);
    }
    if js {
        out.section(&tags);
    }
    if !globals.is_empty() {
        out.section(&globals);
    }
    out.section(&exports);
    if !e.declared.is_empty() {
        let mut elems = ElementSection::new();
        elems.declared(Elements::Functions(e.declared.clone().into()));
        out.section(&elems);
    }
    if e.tables {
        out.section(&DataCountSection { count: 1 });
    }
    out.section(&code);
    if e.tables {
        let mut data = DataSection::new();
        data.passive(numstr::table_bytes().iter().copied());
        out.section(&data);
    }
    out.section(&names);
    Ok(out.finish())
}

/// The cell struct for a variable of type `t`.
fn cell_type(t: Type) -> u32 {
    match t {
        Type::F64 => runtime::T_CELL_F64,
        _ => runtime::T_CELL,
    }
}

fn val_type(t: Type) -> ValType {
    match t {
        Type::F64 => ValType::F64,
        Type::I32 | Type::Bool => ValType::I32,
        Type::JsVal => runtime::jsval(),
        other => unreachable!("no Wasm type for {other}"),
    }
}

/// Does the module need the JS value runtime?
fn uses_jsval(m: &ir::Module) -> bool {
    fn expr(e: &Expr) -> bool {
        match e {
            Expr::Const(c) => c.ty() == Type::JsVal,
            Expr::Local(_) | Expr::Global(_) => false,
            Expr::Convert { from, to, arg } => {
                *from == Type::JsVal || *to == Type::JsVal || expr(arg)
            }
            Expr::Binary { ty, lhs, rhs, .. } => *ty == Type::JsVal || expr(lhs) || expr(rhs),
            Expr::Cond {
                cond,
                then,
                els,
                ty,
            } => *ty == Type::JsVal || expr(cond) || expr(then) || expr(els),
            Expr::TypeofIs { .. } | Expr::Suspend { .. } | Expr::Str(_) => true,
            Expr::Intrinsic { .. } => true,
            Expr::Tee(_, a) | Expr::Unary { arg: a, .. } => expr(a),
            Expr::Call { args, .. } => args.iter().any(expr),
            Expr::Seq { body, value } => stmts(body) || expr(value),
            Expr::Closure(_) | Expr::CallRef { .. } => true,
        }
    }
    fn stmts(list: &[Stmt]) -> bool {
        list.iter().any(|s| match s {
            Stmt::Throw(_) | Stmt::Try { .. } => true, // the tag's type is `jsval`
            Stmt::Expr(e) | Stmt::Set(_, e) | Stmt::SetGlobal(_, e) => expr(e),
            Stmt::Return(e) => e.as_ref().is_some_and(expr),
            Stmt::Block { body, .. } | Stmt::Loop { body, .. } => stmts(body),
            Stmt::If { cond, then, els } => expr(cond) || stmts(then) || stmts(els),
            Stmt::Break(_) | Stmt::Continue(_) => false,
        })
    }
    m.globals.iter().any(|g| g.ty == Type::JsVal)
        || m.functions.iter().any(|f| {
            f.result == Some(Type::JsVal)
                || f.locals.iter().any(|l| l.ty == Type::JsVal)
                || stmts(&f.body)
        })
}

// ---- unsupported IR ------------------------------------------------------------------------

fn check_supported(m: &ir::Module) -> anyhow::Result<()> {
    let mut errs = Vec::new();
    for f in &m.functions {
        let at = f
            .pos
            .map(|p| format!(" at {}:{}", p.line, p.col))
            .unwrap_or_default();
        let mut report = |kind: &str, task: &str| {
            errs.push(format!(
                "unsupported IR `{kind}` in function {}{at} (planned: {task})",
                f.name
            ))
        };
        if f.flags.is_async || f.flags.is_generator {
            report("async/generator function", "T18/T19");
        }
        let mut kinds = Vec::new();
        unsupported_stmts(&f.body, &mut kinds);
        kinds.dedup();
        for (kind, task) in kinds {
            report(kind, task);
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        bail!("{}", errs.join("\n"))
    }
}

fn unsupported_stmts(list: &[Stmt], out: &mut Vec<(&'static str, &'static str)>) {
    for s in list {
        match s {
            Stmt::Try {
                body, catch_body, ..
            } => {
                unsupported_stmts(body, out);
                unsupported_stmts(catch_body, out);
            }
            Stmt::Expr(e)
            | Stmt::Set(_, e)
            | Stmt::SetGlobal(_, e)
            | Stmt::Return(Some(e))
            | Stmt::Throw(e) => unsupported_expr(e, out),
            Stmt::Block { body, .. } | Stmt::Loop { body, .. } => unsupported_stmts(body, out),
            Stmt::If { cond, then, els } => {
                unsupported_expr(cond, out);
                unsupported_stmts(then, out);
                unsupported_stmts(els, out);
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(None) => {}
        }
    }
}

fn unsupported_expr(e: &Expr, out: &mut Vec<(&'static str, &'static str)>) {
    match e {
        Expr::Suspend { arg, .. } => {
            out.push(("Suspend", "T18/T19"));
            unsupported_expr(arg, out);
        }
        Expr::Const(_) | Expr::Local(_) | Expr::Global(_) => {}
        Expr::Tee(_, a)
        | Expr::Unary { arg: a, .. }
        | Expr::Convert { arg: a, .. }
        | Expr::TypeofIs { arg: a, .. } => unsupported_expr(a, out),
        Expr::Str(_) => {}
        Expr::Intrinsic { args, .. } => args.iter().for_each(|a| unsupported_expr(a, out)),
        Expr::Seq { body, value } => {
            unsupported_stmts(body, out);
            unsupported_expr(value, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            unsupported_expr(lhs, out);
            unsupported_expr(rhs, out);
        }
        Expr::Cond {
            cond, then, els, ..
        } => {
            unsupported_expr(cond, out);
            unsupported_expr(then, out);
            unsupported_expr(els, out);
        }
        Expr::Call { args, .. } => args.iter().for_each(|a| unsupported_expr(a, out)),
        Expr::Closure(c) => c.env.iter().for_each(|a| unsupported_expr(a, out)),
        Expr::CallRef {
            callee, this, args, ..
        } => {
            unsupported_expr(callee, out);
            unsupported_expr(this, out);
            args.iter().for_each(|a| unsupported_expr(a, out));
        }
    }
}

/// Call `f` on every expression in `list`, outer ones first.
fn visit_exprs(list: &[Stmt], f: &mut dyn FnMut(&Expr)) {
    fn expr(e: &Expr, f: &mut dyn FnMut(&Expr)) {
        f(e);
        match e {
            Expr::Const(_) | Expr::Local(_) | Expr::Global(_) | Expr::Str(_) => {}
            Expr::Tee(_, a)
            | Expr::Unary { arg: a, .. }
            | Expr::Convert { arg: a, .. }
            | Expr::TypeofIs { arg: a, .. }
            | Expr::Suspend { arg: a, .. } => expr(a, f),
            Expr::Binary { lhs, rhs, .. } => {
                expr(lhs, f);
                expr(rhs, f);
            }
            Expr::Cond {
                cond, then, els, ..
            } => {
                expr(cond, f);
                expr(then, f);
                expr(els, f);
            }
            Expr::Call { args, .. } | Expr::Intrinsic { args, .. } => {
                args.iter().for_each(|a| expr(a, f))
            }
            Expr::Seq { body, value } => {
                visit_exprs(body, f);
                expr(value, f);
            }
            Expr::Closure(c) => c.env.iter().for_each(|a| expr(a, f)),
            Expr::CallRef {
                callee, this, args, ..
            } => {
                expr(callee, f);
                expr(this, f);
                args.iter().for_each(|a| expr(a, f));
            }
        }
    }
    for s in list {
        match s {
            Stmt::Expr(e)
            | Stmt::Set(_, e)
            | Stmt::SetGlobal(_, e)
            | Stmt::Return(Some(e))
            | Stmt::Throw(e) => expr(e, f),
            Stmt::Block { body, .. } | Stmt::Loop { body, .. } => visit_exprs(body, f),
            Stmt::If { cond, then, els } => {
                expr(cond, f);
                visit_exprs(then, f);
                visit_exprs(els, f);
            }
            Stmt::Try {
                body, catch_body, ..
            } => {
                visit_exprs(body, f);
                visit_exprs(catch_body, f);
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(None) => {}
        }
    }
}

// ---- emitter -------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Helper {
    JsRem,
    ToInt32,
    Js(runtime::Helper),
}

impl Helper {
    fn name(self) -> &'static str {
        match self {
            Helper::JsRem => "js_rem",
            Helper::ToInt32 => "to_int32",
            Helper::Js(h) => h.name(),
        }
    }

    fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        match self {
            Helper::JsRem => (vec![ValType::F64, ValType::F64], vec![ValType::F64]),
            Helper::ToInt32 => (vec![ValType::F64], vec![ValType::I32]),
            Helper::Js(h) => h.signature(),
        }
    }
}

/// One entry per open Wasm label, innermost last.
#[derive(Clone, Copy, PartialEq)]
enum Frame {
    Break(LabelId),
    Continue(LabelId),
    /// `if` / typed `if` from `Cond`: not a branch target in the IR.
    Anon,
}

struct Emitter<'m> {
    m: &'m ir::Module,
    types: TypeSection,
    /// Runtime globals that come before the module's own (see `global_base`).
    /// Index of the next function type.
    next_type: u32,
    global_base: u32,
    /// Global index of the ABI bump pointer (when there are string exports).
    bump: u32,
    /// Global index of the first host global (commands).
    host_base: u32,
    /// Number of imported functions: the IR function `i` has index `func_base + i`.
    func_base: u32,
    type_ids: HashMap<(Vec<ValType>, Vec<ValType>), u32>,
    /// Function index of the first helper (after the user functions and ABI adapters).
    helper_base: u32,
    /// Helpers in index order. Helper `k` has function index `helper_base + k`.
    helpers: Vec<Helper>,
    /// Interned string constants, emitted as globals after the user globals.
    strings: Vec<Vec<u16>>,
    string_ids: HashMap<Vec<u16>, u32>,
    /// The number → string tables (data segment 0) are needed.
    tables: bool,
    /// The code of each shared closure: its object is global `shared_base + k`.
    shared: Vec<ir::FuncId>,
    /// Functions used by `ref.func` (a declarative element segment).
    declared: Vec<u32>,
}

impl runtime::Cx for Emitter<'_> {
    fn helper(&mut self, h: runtime::Helper) -> u32 {
        self.helper_index(Helper::Js(h))
    }

    fn string(&mut self, units: &[u16]) -> u32 {
        let base = self.global_base + (self.m.globals.len() + self.shared.len()) as u32;
        if let Some(k) = self.string_ids.get(units) {
            return base + k;
        }
        let k = self.strings.len() as u32;
        self.strings.push(units.to_vec());
        self.string_ids.insert(units.to_vec(), k);
        base + k
    }

    fn tables_data(&mut self) -> u32 {
        self.tables = true;
        0
    }

    fn bump_global(&mut self) -> u32 {
        self.bump
    }

    fn host_globals(&mut self) -> u32 {
        self.host_base
    }

    fn declare_func(&mut self, index: u32) {
        if !self.declared.contains(&index) {
            self.declared.push(index);
        }
    }

    fn main_func(&mut self) -> u32 {
        self.func_base
            + self
                .m
                .main
                .expect("host helpers are only used by commands")
                .0
    }
}

impl Emitter<'_> {
    fn type_id(&mut self, params: Vec<ValType>, results: Vec<ValType>) -> u32 {
        if let Some(t) = self.type_ids.get(&(params.clone(), results.clone())) {
            return *t;
        }
        let t = self.next_type;
        self.next_type += 1;
        self.types.ty().function(params.clone(), results.clone());
        self.type_ids.insert((params, results), t);
        t
    }

    fn helper_index(&mut self, h: Helper) -> u32 {
        let k = match self.helpers.iter().position(|x| *x == h) {
            Some(k) => k,
            None => {
                self.helpers.push(h);
                self.helpers.len() - 1
            }
        };
        self.helper_base + k as u32
    }

    fn function(&mut self, f: &ir::Function) -> anyhow::Result<Function> {
        let locals: Vec<(u32, ValType)> = f.locals[f.params as usize..]
            .iter()
            .map(|l| (1, val_type(l.ty)))
            .collect();
        let mut func = Function::new(locals);
        let mut frames = Vec::new();
        {
            let mut sink = func.instructions();
            for s in &f.body {
                self.stmt(&mut sink, &mut frames, s)?;
            }
            if f.result.is_some() {
                sink.unreachable();
            }
            sink.end();
        }
        Ok(func)
    }

    fn depth(frames: &[Frame], target: Frame) -> anyhow::Result<u32> {
        match frames.iter().rev().position(|f| *f == target) {
            Some(d) => Ok(d as u32),
            None => bail!("internal error: branch target not open"),
        }
    }

    fn stmt(
        &mut self,
        s: &mut InstructionSink,
        frames: &mut Vec<Frame>,
        st: &Stmt,
    ) -> anyhow::Result<()> {
        match st {
            Stmt::Expr(e) => {
                let has_value = self.expr(s, frames, e)?;
                if has_value {
                    s.drop();
                }
            }
            Stmt::Set(l, e) => {
                self.expr(s, frames, e)?;
                s.local_set(l.0);
            }
            Stmt::SetGlobal(g, e) => {
                self.expr(s, frames, e)?;
                s.global_set(self.global_base + g.0);
            }
            Stmt::Block { label, body } => {
                s.block(BlockType::Empty);
                frames.push(Frame::Break(*label));
                for x in body {
                    self.stmt(s, frames, x)?;
                }
                frames.pop();
                s.end();
            }
            Stmt::Loop { label, body } => {
                s.block(BlockType::Empty);
                frames.push(Frame::Break(*label));
                s.loop_(BlockType::Empty);
                frames.push(Frame::Continue(*label));
                for x in body {
                    self.stmt(s, frames, x)?;
                }
                frames.pop();
                s.end();
                frames.pop();
                s.end();
            }
            Stmt::If { cond, then, els } => {
                self.expr(s, frames, cond)?;
                s.if_(BlockType::Empty);
                frames.push(Frame::Anon);
                for x in then {
                    self.stmt(s, frames, x)?;
                }
                if !els.is_empty() {
                    s.else_();
                    for x in els {
                        self.stmt(s, frames, x)?;
                    }
                }
                frames.pop();
                s.end();
            }
            Stmt::Break(l) => {
                s.br(Self::depth(frames, Frame::Break(*l))?);
            }
            Stmt::Continue(l) => {
                s.br(Self::depth(frames, Frame::Continue(*l))?);
            }
            Stmt::Return(e) => {
                if let Some(e) = e {
                    self.expr(s, frames, e)?;
                }
                s.return_();
            }
            Stmt::Throw(e) => {
                self.expr(s, frames, e)?;
                s.throw(runtime::TAG);
            }
            Stmt::Try {
                body,
                catch_local,
                catch_body,
            } => {
                s.block(BlockType::Empty);
                frames.push(Frame::Anon);
                s.block(BlockType::Result(runtime::jsval()));
                frames.push(Frame::Anon);
                // Catch labels count from outside the `try_table`: 0 is the block above.
                s.try_table(
                    BlockType::Empty,
                    [Catch::One {
                        tag: runtime::TAG,
                        label: 0,
                    }],
                );
                frames.push(Frame::Anon);
                for x in body {
                    self.stmt(s, frames, x)?;
                }
                frames.pop();
                s.end();
                s.br(1);
                frames.pop();
                s.end();
                match catch_local {
                    Some(l) => s.local_set(l.0),
                    None => s.drop(),
                };
                for x in catch_body {
                    self.stmt(s, frames, x)?;
                }
                frames.pop();
                s.end();
            }
        }
        Ok(())
    }

    /// Emit `e`. Returns whether it left a value on the stack.
    fn expr(
        &mut self,
        s: &mut InstructionSink,
        frames: &mut Vec<Frame>,
        e: &Expr,
    ) -> anyhow::Result<bool> {
        match e {
            Expr::Const(Const::F64(v)) => {
                s.f64_const((*v).into());
            }
            Expr::Const(Const::I32(v)) => {
                s.i32_const(*v);
            }
            Expr::Const(Const::Bool(v)) => {
                s.i32_const(*v as i32);
            }
            Expr::Const(Const::Undefined) => {
                s.global_get(runtime::G_UNDEFINED);
            }
            Expr::Const(Const::Null) => {
                s.global_get(runtime::G_NULL);
            }
            Expr::Local(l) => {
                s.local_get(l.0);
            }
            Expr::Global(g) => {
                s.global_get(self.global_base + g.0);
            }
            Expr::Tee(l, v) => {
                self.expr(s, frames, v)?;
                s.local_tee(l.0);
            }
            Expr::Unary { op, arg, .. } => match op {
                UnOp::BitNot => {
                    self.expr(s, frames, arg)?;
                    s.i32_const(-1).i32_xor();
                }
                UnOp::Neg => {
                    self.expr(s, frames, arg)?;
                    s.f64_neg();
                }
                UnOp::Abs => {
                    self.expr(s, frames, arg)?;
                    s.f64_abs();
                }
                UnOp::Not => {
                    self.expr(s, frames, arg)?;
                    s.i32_eqz();
                }
            },
            Expr::Binary { op, ty, lhs, rhs } => {
                self.expr(s, frames, lhs)?;
                self.expr(s, frames, rhs)?;
                match ty {
                    Type::F64 => match op {
                        BinOp::Add => s.f64_add(),
                        BinOp::Sub => s.f64_sub(),
                        BinOp::Mul => s.f64_mul(),
                        BinOp::Div => s.f64_div(),
                        BinOp::Rem => s.call(self.helper_index(Helper::JsRem)),
                        BinOp::Eq => s.f64_eq(),
                        BinOp::Ne => s.f64_ne(),
                        BinOp::Lt => s.f64_lt(),
                        BinOp::Le => s.f64_le(),
                        BinOp::Gt => s.f64_gt(),
                        BinOp::Ge => s.f64_ge(),
                        _ => unreachable!("validator rejects {op:?} on f64"),
                    },
                    Type::I32 => match op {
                        BinOp::Add => s.i32_add(),
                        BinOp::Sub => s.i32_sub(),
                        BinOp::Mul => s.i32_mul(),
                        BinOp::Div => s.i32_div_s(),
                        BinOp::Rem => s.i32_rem_s(),
                        BinOp::Eq => s.i32_eq(),
                        BinOp::Ne => s.i32_ne(),
                        BinOp::Lt => s.i32_lt_s(),
                        BinOp::Le => s.i32_le_s(),
                        BinOp::Gt => s.i32_gt_s(),
                        BinOp::Ge => s.i32_ge_s(),
                        BinOp::And => s.i32_and(),
                        BinOp::Or => s.i32_or(),
                        BinOp::Xor => s.i32_xor(),
                        BinOp::Shl => s.i32_shl(),
                        BinOp::Shr => s.i32_shr_s(),
                        BinOp::ShrU => s.i32_shr_u(),
                        BinOp::LooseEq | BinOp::LooseNe => {
                            unreachable!("validator rejects {op:?} on i32")
                        }
                    },
                    Type::Bool => match op {
                        BinOp::Eq => s.i32_eq(),
                        BinOp::Ne => s.i32_ne(),
                        _ => unreachable!("validator rejects {op:?} on bool"),
                    },
                    Type::JsVal => {
                        let (h, negate) = match op {
                            BinOp::Eq => (runtime::Helper::StrictEq, false),
                            BinOp::Ne => (runtime::Helper::StrictEq, true),
                            BinOp::LooseEq => (runtime::Helper::LooseEq, false),
                            BinOp::LooseNe => (runtime::Helper::LooseEq, true),
                            _ => unreachable!("validator rejects {op:?} on jsval"),
                        };
                        s.call(self.helper_index(Helper::Js(h)));
                        if negate {
                            s.i32_eqz();
                        }
                        s
                    }
                    other => unreachable!("no binary ops on {other}"),
                };
            }
            Expr::Convert {
                from: Type::F64,
                to: Type::JsVal,
                arg,
            } if matches!(**arg, Expr::Const(Const::F64(_))) => {
                let Expr::Const(Const::F64(v)) = **arg else {
                    unreachable!()
                };
                runtime::push_number(s, v);
            }
            Expr::Convert { from, to, arg } => {
                self.expr(s, frames, arg)?;
                let js = |h| Helper::Js(h);
                match (from, to) {
                    (Type::F64, Type::I32) => {
                        s.call(self.helper_index(Helper::ToInt32));
                    }
                    (Type::I32, Type::F64) => {
                        s.f64_convert_i32_s();
                    }
                    (Type::F64, Type::JsVal) => {
                        s.call(self.helper_index(js(runtime::Helper::Box)));
                    }
                    (Type::JsVal, Type::F64) => {
                        s.call(self.helper_index(js(runtime::Helper::ToNumber)));
                    }
                    (Type::JsVal, Type::Bool) => {
                        s.call(self.helper_index(js(runtime::Helper::ToBoolean)));
                    }
                    (Type::Bool, Type::JsVal) => {
                        s.if_(BlockType::Result(runtime::jsval()));
                        s.global_get(runtime::G_TRUE);
                        s.else_();
                        s.global_get(runtime::G_FALSE);
                        s.end();
                    }
                    _ => unreachable!("validator rejects {from} -> {to}"),
                }
            }
            Expr::TypeofIs { arg, tag } => {
                self.expr(s, frames, arg)?;
                s.call(self.helper_index(Helper::Js(runtime::Helper::Typeof)));
                s.i32_const(runtime::typeof_code(*tag)).i32_eq();
            }
            Expr::Seq { body, value } => {
                for x in body {
                    self.stmt(s, frames, x)?;
                }
                return self.expr(s, frames, value);
            }
            Expr::Str(units) => {
                let g = runtime::Cx::string(self, units);
                s.global_get(g);
            }
            Expr::Intrinsic { op, args } => {
                use runtime::Helper as H;
                if let Intrinsic::ConsoleWrite { .. } = op
                    && self.m.main.is_none()
                {
                    bail!("internal error: console output in a module that isn't a command");
                }
                for (i, a) in args.iter().enumerate() {
                    self.expr(s, frames, a)?;
                    if matches!(
                        op,
                        Intrinsic::Concat
                            | Intrinsic::ConsoleWrite { .. }
                            | Intrinsic::StringLength
                    ) {
                        s.ref_cast_non_null(runtime::string());
                    }
                    if matches!(op, Intrinsic::KeysLen | Intrinsic::KeysGet) && i == 0 {
                        s.ref_cast_non_null(wasm_encoder::HeapType::Concrete(runtime::T_KEYS));
                    }
                    if i == 0 {
                        match op {
                            Intrinsic::CellGet(t) | Intrinsic::CellSet(t) => {
                                s.ref_cast_non_null(wasm_encoder::HeapType::Concrete(cell_type(
                                    *t,
                                )));
                            }
                            Intrinsic::CellIsUninit => {
                                s.ref_cast_non_null(wasm_encoder::HeapType::Concrete(
                                    runtime::T_CELL,
                                ));
                            }
                            Intrinsic::EnvGet(_) => {
                                s.ref_cast_non_null(func::function());
                            }
                            _ => {}
                        }
                    }
                }
                // Inline.
                match op {
                    Intrinsic::StringLength => {
                        s.array_len().f64_convert_i32_u();
                        return Ok(true);
                    }
                    Intrinsic::KeysLen => {
                        s.array_len();
                        return Ok(true);
                    }
                    Intrinsic::KeysGet => {
                        s.array_get(runtime::T_KEYS);
                        return Ok(true);
                    }
                    Intrinsic::Prototype(b) => {
                        s.call(self.helper_index(Helper::Js(H::Obj(object::Helper::Proto(*b)))));
                        return Ok(true);
                    }
                    Intrinsic::NewCell(t) => {
                        s.struct_new(cell_type(*t));
                        return Ok(true);
                    }
                    Intrinsic::NewCellUninit => {
                        s.ref_null(wasm_encoder::HeapType::Abstract {
                            shared: false,
                            ty: wasm_encoder::AbstractHeapType::Eq,
                        })
                        .struct_new(runtime::T_CELL);
                        return Ok(true);
                    }
                    Intrinsic::CellGet(t) => {
                        s.struct_get(cell_type(*t), 0);
                        return Ok(true);
                    }
                    Intrinsic::CellSet(t) => {
                        s.struct_set(cell_type(*t), 0);
                        return Ok(false);
                    }
                    Intrinsic::CellIsUninit => {
                        s.struct_get(runtime::T_CELL, 0).ref_is_null();
                        return Ok(true);
                    }
                    Intrinsic::EnvGet(i) => {
                        s.struct_get(runtime::T_FUNCTION, func::F_ENV)
                            .i32_const(*i as i32)
                            .array_get(runtime::T_VALS);
                        return Ok(true);
                    }
                    Intrinsic::ArgGet(i) => {
                        s.i32_const(*i as i32);
                        s.call(self.helper_index(Helper::Js(H::Func(func::Helper::ArgGet))));
                        return Ok(true);
                    }
                    _ => {}
                }
                let h = match op {
                    Intrinsic::JsAdd => H::JsAdd,
                    Intrinsic::Concat => H::Concat,
                    Intrinsic::ToString => H::ToString,
                    Intrinsic::NumberToString => H::Num(numstr::Helper::NumberToString),
                    Intrinsic::StringLength
                    | Intrinsic::KeysLen
                    | Intrinsic::KeysGet
                    | Intrinsic::Prototype(_)
                    | Intrinsic::NewCell(_)
                    | Intrinsic::NewCellUninit
                    | Intrinsic::CellGet(_)
                    | Intrinsic::CellSet(_)
                    | Intrinsic::CellIsUninit
                    | Intrinsic::EnvGet(_)
                    | Intrinsic::ArgGet(_) => unreachable!("inline"),
                    Intrinsic::TypeofString => H::TypeofString,
                    Intrinsic::Inspect => H::Inspect,
                    Intrinsic::NoFormat => H::NoFormat,
                    Intrinsic::ConsoleWrite { stderr } => {
                        H::Host(host::Helper::Append(*stderr as u32))
                    }
                    Intrinsic::NewError(k) => H::Obj(object::Helper::NewError(*k)),
                    Intrinsic::NewObject => H::Obj(object::Helper::NewObject),
                    Intrinsic::GetProp => H::Obj(object::Helper::Get),
                    Intrinsic::SetProp => H::Obj(object::Helper::Set),
                    Intrinsic::DefineProp => H::Obj(object::Helper::DefineProp),
                    Intrinsic::DeleteProp => H::Obj(object::Helper::Delete),
                    Intrinsic::HasProp => H::Obj(object::Helper::Has),
                    Intrinsic::SetProto => H::Obj(object::Helper::SetProto),
                    Intrinsic::CopyProps => H::Obj(object::Helper::CopyProps),
                    Intrinsic::ForInKeys => H::Obj(object::Helper::ForInKeys),
                    Intrinsic::ObjectCreate => H::Obj(object::Helper::Create),
                    Intrinsic::GetPrototypeOf => H::Obj(object::Helper::GetPrototypeOf),
                    Intrinsic::HasOwn => H::Obj(object::Helper::HasOwn),
                    Intrinsic::InstanceOf(b) => H::Obj(object::Helper::InstanceOf(*b)),
                };
                s.call(self.helper_index(Helper::Js(h)));
                return Ok(op.signature().1.is_some());
            }
            Expr::Cond {
                cond,
                then,
                els,
                ty,
            } => {
                self.expr(s, frames, cond)?;
                s.if_(BlockType::Result(val_type(*ty)));
                frames.push(Frame::Anon);
                self.expr(s, frames, then)?;
                s.else_();
                self.expr(s, frames, els)?;
                frames.pop();
                s.end();
            }
            Expr::Call { func, args } => {
                for a in args {
                    self.expr(s, frames, a)?;
                }
                s.call(self.func_base + func.0);
                return Ok(self.m.function(*func).result.is_some());
            }
            Expr::Suspend { .. } => unreachable!("rejected by check_supported"),
            Expr::Closure(c) => {
                let shared = c
                    .shared
                    .then(|| self.shared.iter().position(|x| *x == c.code).unwrap() as u32)
                    .map(|k| self.global_base + self.m.globals.len() as u32 + k);
                if let Some(g) = shared {
                    s.global_get(g).ref_is_null();
                    s.if_(BlockType::Empty);
                    frames.push(Frame::Anon);
                }
                let code = self.func_base + c.code.0;
                runtime::Cx::declare_func(self, code);
                s.ref_func(code);
                if c.env.is_empty() {
                    s.ref_null(func::vals());
                } else {
                    for x in &c.env {
                        self.expr(s, frames, x)?;
                    }
                    s.array_new_fixed(runtime::T_VALS, c.env.len() as u32);
                }
                let name = runtime::Cx::str(self, &c.name);
                s.global_get(name);
                s.i32_const(c.length as i32).i32_const(c.constructor as i32);
                s.call(self.helper_index(Helper::Js(runtime::Helper::Func(func::Helper::New))));
                if let Some(g) = shared {
                    s.global_set(g);
                    frames.pop();
                    s.end();
                    s.global_get(g);
                }
            }
            Expr::CallRef {
                callee,
                this,
                args,
                what,
            } => {
                self.expr(s, frames, callee)?;
                self.expr(s, frames, this)?;
                for a in args {
                    self.expr(s, frames, a)?;
                }
                s.array_new_fixed(runtime::T_VALS, args.len() as u32);
                let what = runtime::Cx::str(self, what);
                s.global_get(what);
                s.call(
                    self.helper_index(Helper::Js(runtime::Helper::Func(func::Helper::CallValue))),
                );
            }
        }
        Ok(true)
    }

    fn helper(&mut self, h: Helper) -> Function {
        match h {
            Helper::JsRem => js_rem(),
            Helper::ToInt32 => {
                let rem = self.helper_index(Helper::JsRem);
                to_int32(rem)
            }
            Helper::Js(h) => h.body(self),
        }
    }
}

/// `$to_int32(x: f64) -> i32`: JS ToInt32.
fn to_int32(js_rem: u32) -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0)
        .f64_abs()
        .f64_const(9223372036854775808.0.into())
        .f64_lt();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(0).i64_trunc_sat_f64_s().i32_wrap_i64();
    s.else_();
    // |x| ≥ 2^63, NaN or ±∞: reduce modulo 2^32 exactly first (NaN/∞ → NaN → 0).
    s.local_get(0)
        .f64_const(4294967296.0.into())
        .call(js_rem)
        .i64_trunc_sat_f64_s()
        .i32_wrap_i64();
    s.end();
    s.end();
    f
}

/// `$js_rem(x: f64, y: f64) -> f64`: exact `fmod`, the semantics of JS `%` on numbers.
/// A port of musl's `fmod` (bit manipulation, no rounding error).
fn js_rem() -> Function {
    const X: u32 = 0;
    const Y: u32 = 1;
    const UX: u32 = 2;
    const UY: u32 = 3;
    const EX: u32 = 4;
    const EY: u32 = 5;
    const SX: u32 = 6;
    const I: u32 = 7;
    let mut f = Function::new([(6, ValType::I64)]);
    let mut s = f.instructions();

    // ux = bits(x); uy = bits(y); ex = ux>>52 & 0x7ff; ey = uy>>52 & 0x7ff; sx = ux>>63
    s.local_get(X).i64_reinterpret_f64().local_set(UX);
    s.local_get(Y).i64_reinterpret_f64().local_set(UY);
    s.local_get(UX)
        .i64_const(52)
        .i64_shr_u()
        .i64_const(0x7ff)
        .i64_and()
        .local_set(EX);
    s.local_get(UY)
        .i64_const(52)
        .i64_shr_u()
        .i64_const(0x7ff)
        .i64_and()
        .local_set(EY);
    s.local_get(UX).i64_const(63).i64_shr_u().local_set(SX);

    // if (uy<<1 == 0 || isnan(y) || ex == 0x7ff) return (x*y)/(x*y)
    s.local_get(UY).i64_const(1).i64_shl().i64_eqz();
    s.local_get(Y).local_get(Y).f64_ne().i32_or();
    s.local_get(EX).i64_const(0x7ff).i64_eq().i32_or();
    s.if_(BlockType::Empty);
    s.local_get(X)
        .local_get(Y)
        .f64_mul()
        .local_get(X)
        .local_get(Y)
        .f64_mul()
        .f64_div()
        .return_();
    s.end();

    // if (ux<<1 <= uy<<1) { if (ux<<1 == uy<<1) return 0*x; return x; }
    s.local_get(UX)
        .i64_const(1)
        .i64_shl()
        .local_get(UY)
        .i64_const(1)
        .i64_shl()
        .i64_le_u();
    s.if_(BlockType::Empty);
    s.local_get(UX)
        .i64_const(1)
        .i64_shl()
        .local_get(UY)
        .i64_const(1)
        .i64_shl()
        .i64_eq();
    s.if_(BlockType::Empty);
    s.f64_const(0.0.into()).local_get(X).f64_mul().return_();
    s.end();
    s.local_get(X).return_();
    s.end();

    // Normalize x and y (subnormals: shift the leading 1 into bit 52).
    for (u, e) in [(UX, EX), (UY, EY)] {
        s.local_get(e).i64_eqz();
        s.if_(BlockType::Empty);
        {
            // for (i = u<<12; i>>63 == 0; e--, i <<= 1);
            s.local_get(u).i64_const(12).i64_shl().local_set(I);
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(I)
                .i64_const(63)
                .i64_shr_u()
                .i32_wrap_i64()
                .br_if(1);
            s.local_get(e).i64_const(1).i64_sub().local_set(e);
            s.local_get(I).i64_const(1).i64_shl().local_set(I);
            s.br(0).end().end();
            // u <<= 1 - e
            s.local_get(u)
                .i64_const(1)
                .local_get(e)
                .i64_sub()
                .i64_shl()
                .local_set(u);
        }
        s.else_();
        {
            // u = (u & (2^52 - 1)) | 2^52
            s.local_get(u)
                .i64_const(0x000f_ffff_ffff_ffff)
                .i64_and()
                .i64_const(1 << 52)
                .i64_or()
                .local_set(u);
        }
        s.end();
    }

    // Shared step: i = ux - uy; if (i >= 0) { if (i == 0) return 0*x; ux = i; }
    let step = |s: &mut InstructionSink| {
        s.local_get(UX)
            .local_get(UY)
            .i64_sub()
            .local_tee(I)
            .i64_const(0)
            .i64_ge_s();
        s.if_(BlockType::Empty);
        s.local_get(I).i64_eqz();
        s.if_(BlockType::Empty);
        s.f64_const(0.0.into()).local_get(X).f64_mul().return_();
        s.end();
        s.local_get(I).local_set(UX);
        s.end();
    };

    // for (; ex > ey; ex--) { step; ux <<= 1; }
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(EX).local_get(EY).i64_le_s().br_if(1);
    step(&mut s);
    s.local_get(UX).i64_const(1).i64_shl().local_set(UX);
    s.local_get(EX).i64_const(1).i64_sub().local_set(EX);
    s.br(0).end().end();
    step(&mut s);

    // for (; ux>>52 == 0; ux <<= 1, ex--);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(UX)
        .i64_const(52)
        .i64_shr_u()
        .i64_eqz()
        .i32_eqz()
        .br_if(1);
    s.local_get(UX).i64_const(1).i64_shl().local_set(UX);
    s.local_get(EX).i64_const(1).i64_sub().local_set(EX);
    s.br(0).end().end();

    // if (ex > 0) { ux -= 2^52; ux |= ex << 52; } else { ux >>= 1 - ex; }
    s.local_get(EX).i64_const(0).i64_gt_s();
    s.if_(BlockType::Empty);
    s.local_get(UX)
        .i64_const(1 << 52)
        .i64_sub()
        .local_get(EX)
        .i64_const(52)
        .i64_shl()
        .i64_or()
        .local_set(UX);
    s.else_();
    s.local_get(UX)
        .i64_const(1)
        .local_get(EX)
        .i64_sub()
        .i64_shr_u()
        .local_set(UX);
    s.end();

    // return bits(ux | sx << 63)
    s.local_get(UX)
        .local_get(SX)
        .i64_const(63)
        .i64_shl()
        .i64_or()
        .f64_reinterpret_i64();
    s.end();
    f
}
