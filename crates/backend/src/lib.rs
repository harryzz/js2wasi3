//! Backend: IR → core Wasm bytes (WasmGC types, exception tag, linear memory for the ABI only).
//!
//! v0 (T06) handles the numbers-only IR from T05: `f64`/`i32`/`bool` locals and globals,
//! structured control flow, direct calls and exports. T08 adds `jsval` (see [`runtime`]).
//!
//! # Mapping
//! - `Bool` is an `i32` holding 0 or 1.
//! - `Block L { .. }` → `block`. `Loop L { .. }` → `block $brk (loop $cont ..)`: `Break L` branches
//!   to `$brk`, `Continue L` to `$cont`. `if` also opens a label level, which depth computation accounts for.
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

mod runtime;

use std::collections::HashMap;

use anyhow::{Context, bail};
use js2wasi3_ir::{self as ir, BinOp, Const, Expr, LabelId, Stmt, Type, UnOp};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, ExportKind, ExportSection, Function, FunctionSection,
    GlobalSection, GlobalType, IndirectNameMap, InstructionSink, NameMap, NameSection, TypeSection,
    ValType,
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

    let js = uses_jsval(module);
    let mut types = TypeSection::new();
    if js {
        runtime::types(&mut types);
    }
    let mut e = Emitter {
        m: module,
        types,
        type_base: if js { runtime::TYPES } else { 0 },
        global_base: if js { runtime::GLOBALS } else { 0 },
        type_ids: HashMap::new(),
        helpers: Vec::new(),
    };
    let n = module.functions.len() as u32;

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

    let mut exports = ExportSection::new();
    for x in &module.exports {
        exports.export(&x.name, ExportKind::Func, x.func.0);
    }

    let mut names = NameSection::new();
    let mut fnames = NameMap::new();
    let mut lnames = IndirectNameMap::new();
    for (i, f) in module.functions.iter().enumerate() {
        fnames.append(i as u32, &f.name);
        let mut locals = NameMap::new();
        for (j, l) in f.locals.iter().enumerate() {
            locals.append(j as u32, &l.name);
        }
        lnames.append(i as u32, &locals);
    }
    for (k, h) in e.helpers.iter().enumerate() {
        fnames.append(n + k as u32, h.name());
    }
    names.functions(&fnames);
    names.locals(&lnames);

    let mut out = wasm_encoder::Module::new();
    out.section(&e.types).section(&funcs);
    if !globals.is_empty() {
        out.section(&globals);
    }
    out.section(&exports).section(&code).section(&names);
    Ok(out.finish())
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
            Expr::TypeofIs { .. } | Expr::Suspend { .. } => true,
            Expr::Tee(_, a) | Expr::Unary { arg: a, .. } => expr(a),
            Expr::Call { args, .. } => args.iter().any(expr),
            Expr::Seq { body, value } => stmts(body) || expr(value),
        }
    }
    fn stmts(list: &[Stmt]) -> bool {
        list.iter().any(|s| match s {
            Stmt::Expr(e) | Stmt::Set(_, e) | Stmt::SetGlobal(_, e) | Stmt::Throw(e) => expr(e),
            Stmt::Return(e) => e.as_ref().is_some_and(expr),
            Stmt::Block { body, .. } | Stmt::Loop { body, .. } => stmts(body),
            Stmt::If { cond, then, els } => expr(cond) || stmts(then) || stmts(els),
            Stmt::Try {
                body, catch_body, ..
            } => stmts(body) || stmts(catch_body),
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
                out.push(("Try", "T12"));
                unsupported_stmts(body, out);
                unsupported_stmts(catch_body, out);
            }
            Stmt::Throw(e) => {
                out.push(("Throw", "T12"));
                unsupported_expr(e, out);
            }
            Stmt::Expr(e) | Stmt::Set(_, e) | Stmt::SetGlobal(_, e) | Stmt::Return(Some(e)) => {
                unsupported_expr(e, out)
            }
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
    /// Runtime types / globals that come before the module's own.
    type_base: u32,
    global_base: u32,
    type_ids: HashMap<(Vec<ValType>, Vec<ValType>), u32>,
    /// Helpers in index order. Helper `k` has function index `m.functions.len() + k`.
    helpers: Vec<Helper>,
}

impl Emitter<'_> {
    fn type_id(&mut self, params: Vec<ValType>, results: Vec<ValType>) -> u32 {
        let next = self.type_base + self.type_ids.len() as u32;
        *self
            .type_ids
            .entry((params.clone(), results.clone()))
            .or_insert_with(|| {
                self.types.ty().function(params, results);
                next
            })
    }

    fn helper_index(&mut self, h: Helper) -> u32 {
        let k = match self.helpers.iter().position(|x| *x == h) {
            Some(k) => k,
            None => {
                self.helpers.push(h);
                self.helpers.len() - 1
            }
        };
        self.m.functions.len() as u32 + k as u32
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
            Stmt::Try { .. } | Stmt::Throw(_) => unreachable!("rejected by check_supported"),
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
                s.call(func.0);
                return Ok(self.m.function(*func).result.is_some());
            }
            Expr::Suspend { .. } => unreachable!("rejected by check_supported"),
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
            Helper::Js(h) => h.body(&mut |x| self.helper_index(Helper::Js(x))),
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
