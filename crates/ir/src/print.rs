//! Stable text form of the IR, used for snapshot tests and `js2wasi3 ir`.
//!
//! ```text
//! global @count: i32 mut = 0_i32
//! func add#0(a.0: f64, b.1: f64) -> f64 {
//!   return f64.add(a.0, b.1)
//! }
//! export "add" = add#0
//! ```
//! A command ends with `main = <func>`.
//! Locals print as `name.index`, functions as `name#index`, globals as `@name`, labels as `L<n>`.
//! `f64` constants always carry a decimal point (`1.0`, `NaN`, `inf`), `i32` constants a `_i32` suffix.
//! `Seq` prints on one line: `seq { stmt; stmt; value }`. Intrinsics print as `$name(args)`.

use std::fmt::{self, Write};

use crate::{BinOp, Const, Expr, Function, Module, Stmt, SuspendKind, UnOp};

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut p = Printer {
            m: self,
            out: String::new(),
            depth: 0,
        };
        p.module();
        f.write_str(&p.out)
    }
}

struct Printer<'m> {
    m: &'m Module,
    out: String,
    depth: usize,
}

impl Printer<'_> {
    fn line(&mut self, s: &str) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn module(&mut self) {
        for g in &self.m.globals {
            let m = if g.mutable { " mut" } else { "" };
            let s = format!("global @{}: {}{m} = {}", g.name, g.ty, konst(&g.init));
            self.line(&s);
        }
        for (i, func) in self.m.functions.iter().enumerate() {
            self.function(i, func);
        }
        for e in &self.m.exports {
            let s = format!("export {:?} = {}", e.name, self.func_name(e.func.0));
            self.line(&s);
        }
        if let Some(main) = self.m.main {
            let s = format!("main = {}", self.func_name(main.0));
            self.line(&s);
        }
    }

    fn func_name(&self, i: u32) -> String {
        format!("{}#{i}", self.m.functions[i as usize].name)
    }

    fn function(&mut self, i: usize, func: &Function) {
        let params: Vec<String> = (0..func.params)
            .map(|p| format!("{}: {}", local(func, p), func.locals[p as usize].ty))
            .collect();
        let mut head = String::new();
        if func.flags.is_async {
            head.push_str("async ");
        }
        head.push_str("func");
        if func.flags.is_generator {
            head.push('*');
        }
        write!(head, " {}({})", self.func_name(i as u32), params.join(", ")).unwrap();
        if let Some(r) = func.result {
            write!(head, " -> {r}").unwrap();
        }
        head.push_str(" {");
        self.line(&head);
        self.depth += 1;
        for l in func.params..func.locals.len() as u32 {
            let s = format!("local {}: {}", local(func, l), func.locals[l as usize].ty);
            self.line(&s);
        }
        self.stmts(func, &func.body);
        self.depth -= 1;
        self.line("}");
    }

    fn stmts(&mut self, func: &Function, body: &[Stmt]) {
        for s in body {
            self.stmt(func, s);
        }
    }

    fn nested(&mut self, func: &Function, head: &str, body: &[Stmt]) {
        self.line(&format!("{head} {{"));
        self.depth += 1;
        self.stmts(func, body);
        self.depth -= 1;
    }

    fn stmt(&mut self, func: &Function, s: &Stmt) {
        match s {
            Stmt::Expr(e) => {
                let e = self.expr(func, e);
                self.line(&e);
            }
            Stmt::Set(l, e) => {
                let s = format!("{} = {}", local(func, l.0), self.expr(func, e));
                self.line(&s);
            }
            Stmt::SetGlobal(g, e) => {
                let s = format!(
                    "@{} = {}",
                    self.m.globals[g.0 as usize].name,
                    self.expr(func, e)
                );
                self.line(&s);
            }
            Stmt::Block { label, body } => {
                self.nested(func, &format!("block L{}", label.0), body);
                self.line("}");
            }
            Stmt::Loop { label, body } => {
                self.nested(func, &format!("loop L{}", label.0), body);
                self.line("}");
            }
            Stmt::If { cond, then, els } => {
                let c = self.expr(func, cond);
                self.nested(func, &format!("if {c}"), then);
                if els.is_empty() {
                    self.line("}");
                } else {
                    self.line("} else {");
                    self.depth += 1;
                    self.stmts(func, els);
                    self.depth -= 1;
                    self.line("}");
                }
            }
            Stmt::Break(l) => self.line(&format!("break L{}", l.0)),
            Stmt::Continue(l) => self.line(&format!("continue L{}", l.0)),
            Stmt::Return(None) => self.line("return"),
            Stmt::Return(Some(e)) => {
                let s = format!("return {}", self.expr(func, e));
                self.line(&s);
            }
            Stmt::Try {
                body,
                catch_local,
                catch_body,
            } => {
                self.nested(func, "try", body);
                let head = match catch_local {
                    Some(l) => format!("}} catch ({}) {{", local(func, l.0)),
                    None => "} catch {".to_string(),
                };
                self.line(&head);
                self.depth += 1;
                self.stmts(func, catch_body);
                self.depth -= 1;
                self.line("}");
            }
            Stmt::Throw(e) => {
                let s = format!("throw {}", self.expr(func, e));
                self.line(&s);
            }
        }
    }

    fn expr(&self, func: &Function, e: &Expr) -> String {
        match e {
            Expr::Const(c) => konst(c),
            Expr::Local(l) => local(func, l.0),
            Expr::Global(g) => format!("@{}", self.m.globals[g.0 as usize].name),
            Expr::Tee(l, v) => format!("({} := {})", local(func, l.0), self.expr(func, v)),
            Expr::Unary { op, ty, arg } => format!("{ty}.{}({})", unop(*op), self.expr(func, arg)),
            Expr::Binary { op, ty, lhs, rhs } => {
                format!(
                    "{ty}.{}({}, {})",
                    binop(*op),
                    self.expr(func, lhs),
                    self.expr(func, rhs)
                )
            }
            Expr::Convert { from, to, arg } => format!("{from}->{to}({})", self.expr(func, arg)),
            Expr::Cond {
                cond,
                then,
                els,
                ty,
            } => format!(
                "cond.{ty}({}, {}, {})",
                self.expr(func, cond),
                self.expr(func, then),
                self.expr(func, els)
            ),
            Expr::Call { func: callee, args } => {
                let args: Vec<String> = args.iter().map(|a| self.expr(func, a)).collect();
                format!("call {}({})", self.func_name(callee.0), args.join(", "))
            }
            Expr::Str(units) => quote(units),
            Expr::Intrinsic { op, args } => {
                let args: Vec<String> = args.iter().map(|a| self.expr(func, a)).collect();
                format!("${}({})", op.name(), args.join(", "))
            }
            Expr::TypeofIs { arg, tag } => {
                format!("typeof({}) == {:?}", self.expr(func, arg), tag.as_str())
            }
            Expr::Seq { body, value } => {
                let mut p = Printer {
                    m: self.m,
                    out: String::new(),
                    depth: 0,
                };
                p.stmts(func, body);
                let mut parts: Vec<String> = p.out.lines().map(|l| l.trim().to_string()).collect();
                parts.push(self.expr(func, value));
                format!("seq {{ {} }}", parts.join("; "))
            }
            Expr::Suspend { kind, arg } => {
                let k = match kind {
                    SuspendKind::Await => "await",
                    SuspendKind::Yield => "yield",
                };
                format!("{k}({})", self.expr(func, arg))
            }
        }
    }
}

/// A string literal: `"..."` with JSON-style escapes; lone surrogates as `\u{d800}`.
fn quote(units: &[u16]) -> String {
    let mut out = String::from("\"");
    for c in char::decode_utf16(units.iter().copied()) {
        match c {
            Ok(c) => out.extend(c.escape_debug()),
            Err(e) => out.push_str(&format!("\\u{{{:x}}}", e.unpaired_surrogate())),
        }
    }
    out.push('"');
    out
}

fn local(func: &Function, i: u32) -> String {
    format!("{}.{i}", func.locals[i as usize].name)
}

fn konst(c: &Const) -> String {
    match c {
        // `{:?}` keeps the decimal point (`1.0`) and prints `NaN` / `inf` / `-0.0`.
        Const::F64(v) => format!("{v:?}"),
        Const::I32(v) => format!("{v}_i32"),
        Const::Bool(v) => v.to_string(),
        Const::Undefined => "undefined".to_string(),
        Const::Null => "null".to_string(),
    }
}

fn unop(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "neg",
        UnOp::Abs => "abs",
        UnOp::Not => "not",
        UnOp::BitNot => "bitnot",
    }
}

fn binop(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "add",
        BinOp::Sub => "sub",
        BinOp::Mul => "mul",
        BinOp::Div => "div",
        BinOp::Rem => "rem",
        BinOp::Eq => "eq",
        BinOp::Ne => "ne",
        BinOp::Lt => "lt",
        BinOp::Le => "le",
        BinOp::Gt => "gt",
        BinOp::Ge => "ge",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::Xor => "xor",
        BinOp::Shl => "shl",
        BinOp::Shr => "shr",
        BinOp::ShrU => "shr_u",
        BinOp::LooseEq => "loose_eq",
        BinOp::LooseNe => "loose_ne",
    }
}
