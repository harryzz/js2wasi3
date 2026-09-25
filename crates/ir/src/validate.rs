//! IR validator: catches lowering bugs before they turn into invalid Wasm.
//!
//! Checks id ranges, label scoping (`Break` to an enclosing `Block`/`Loop`, `Continue`
//! only to an enclosing `Loop`, no label defined twice on one path), and types: assignments,
//! returns, conditions, operand types allowed for each op, call arity/argument types, and
//! no value-less call used as a value. Placeholder kinds (`Try`, `Throw`, `Suspend`) are
//! checked structurally only.

use crate::{BinOp, Const, Expr, Function, Module, Stmt, Type, UnOp};

/// Validate a whole module. Each error names the function it's in.
pub fn validate(m: &Module) -> Result<(), Vec<String>> {
    let mut errs = Vec::new();
    for g in &m.globals {
        if const_ty(&g.init) != g.ty {
            errs.push(format!(
                "global @{}: init {:?} is not {}",
                g.name, g.init, g.ty
            ));
        }
    }
    for (i, f) in m.functions.iter().enumerate() {
        let mut v = V {
            m,
            f,
            labels: Vec::new(),
            errs: Vec::new(),
        };
        if f.params as usize > f.locals.len() {
            v.err(format!(
                "{} params but only {} locals",
                f.params,
                f.locals.len()
            ));
        }
        v.stmts(&f.body);
        errs.extend(
            v.errs
                .into_iter()
                .map(|e| format!("func {}#{i}: {e}", f.name)),
        );
    }
    for e in &m.exports {
        if e.func.0 as usize >= m.functions.len() {
            errs.push(format!("export {:?}: no function #{}", e.name, e.func.0));
        }
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

fn const_ty(c: &Const) -> Type {
    match c {
        Const::F64(_) => Type::F64,
        Const::I32(_) => Type::I32,
        Const::Bool(_) => Type::Bool,
    }
}

struct V<'m> {
    m: &'m Module,
    f: &'m Function,
    /// Enclosing labels: (label, is_loop).
    labels: Vec<(crate::LabelId, bool)>,
    errs: Vec<String>,
}

impl V<'_> {
    fn err(&mut self, e: String) {
        self.errs.push(e);
    }

    fn local_ty(&mut self, l: crate::LocalId) -> Option<Type> {
        match self.f.locals.get(l.0 as usize) {
            Some(x) => Some(x.ty),
            None => {
                self.err(format!("no local .{}", l.0));
                None
            }
        }
    }

    fn expect(&mut self, what: &str, got: Option<Type>, want: Type) {
        match got {
            Some(t) if t == want => {}
            Some(t) => self.err(format!("{what}: expected {want}, got {t}")),
            None => self.err(format!("{what}: expected {want}, got no value")),
        }
    }

    fn stmts(&mut self, list: &[Stmt]) {
        for s in list {
            self.stmt(s);
        }
    }

    fn nested(&mut self, label: crate::LabelId, is_loop: bool, body: &[Stmt]) {
        if self.labels.iter().any(|(l, _)| *l == label) {
            self.err(format!("label L{} defined inside itself", label.0));
        }
        self.labels.push((label, is_loop));
        self.stmts(body);
        self.labels.pop();
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Expr(e) => {
                self.expr(e);
            }
            Stmt::Set(l, e) => {
                let t = self.expr(e);
                if let Some(lt) = self.local_ty(*l) {
                    self.expect(&format!("set .{}", l.0), t, lt);
                }
            }
            Stmt::SetGlobal(g, e) => {
                let t = self.expr(e);
                match self.m.globals.get(g.0 as usize) {
                    None => self.err(format!("no global #{}", g.0)),
                    Some(gl) => {
                        if !gl.mutable {
                            self.err(format!("set of immutable global @{}", gl.name));
                        }
                        self.expect(&format!("set @{}", gl.name), t, gl.ty);
                    }
                }
            }
            Stmt::Block { label, body } => self.nested(*label, false, body),
            Stmt::Loop { label, body } => self.nested(*label, true, body),
            Stmt::If { cond, then, els } => {
                let t = self.expr(cond);
                self.expect("if condition", t, Type::Bool);
                self.stmts(then);
                self.stmts(els);
            }
            Stmt::Break(l) => {
                if !self.labels.iter().any(|(x, _)| x == l) {
                    self.err(format!(
                        "break L{}: no enclosing block or loop with that label",
                        l.0
                    ));
                }
            }
            Stmt::Continue(l) => match self.labels.iter().find(|(x, _)| x == l) {
                Some((_, true)) => {}
                Some((_, false)) => {
                    self.err(format!("continue L{}: label is a block, not a loop", l.0))
                }
                None => self.err(format!(
                    "continue L{}: no enclosing loop with that label",
                    l.0
                )),
            },
            Stmt::Return(e) => {
                let t = e.as_ref().and_then(|e| self.expr(e));
                match (self.f.result, e) {
                    (Some(r), Some(_)) => self.expect("return", t, r),
                    (None, None) => {}
                    (Some(r), None) => self.err(format!(
                        "return without a value in a function returning {r}"
                    )),
                    (None, Some(_)) => {
                        self.err("return with a value in a function without a result".into())
                    }
                }
            }
            Stmt::Try {
                body,
                catch_local,
                catch_body,
            } => {
                self.stmts(body);
                if let Some(l) = catch_local {
                    self.local_ty(*l);
                }
                self.stmts(catch_body);
            }
            Stmt::Throw(e) => {
                self.expr(e);
            }
        }
    }

    /// Type of `e`, or `None` for a value-less call (which is only valid as `Stmt::Expr`).
    fn value(&mut self, what: &str, e: &Expr) -> Option<Type> {
        let t = self.expr(e);
        if t.is_none() {
            self.err(format!("{what}: call without a result used as a value"));
        }
        t
    }

    fn expr(&mut self, e: &Expr) -> Option<Type> {
        match e {
            Expr::Const(c) => Some(const_ty(c)),
            Expr::Local(l) => self.local_ty(*l),
            Expr::Global(g) => match self.m.globals.get(g.0 as usize) {
                Some(gl) => Some(gl.ty),
                None => {
                    self.err(format!("no global #{}", g.0));
                    None
                }
            },
            Expr::Tee(l, v) => {
                let t = self.value("tee", v);
                let lt = self.local_ty(*l)?;
                self.expect(&format!("tee .{}", l.0), t, lt);
                Some(lt)
            }
            Expr::Unary { op, ty, arg } => {
                let t = self.value("unary operand", arg);
                self.expect(&format!("{op:?} operand"), t, *ty);
                let ok = matches!(
                    (op, ty),
                    (UnOp::Neg | UnOp::Abs, Type::F64)
                        | (UnOp::Not, Type::Bool)
                        | (UnOp::BitNot, Type::I32)
                );
                if !ok {
                    self.err(format!("{op:?} is not defined on {ty}"));
                }
                Some(*ty)
            }
            Expr::Binary { op, ty, lhs, rhs } => {
                let (l, r) = (self.value("binary lhs", lhs), self.value("binary rhs", rhs));
                self.expect(&format!("{op:?} lhs"), l, *ty);
                self.expect(&format!("{op:?} rhs"), r, *ty);
                let ok = match ty {
                    Type::I32 => true,
                    Type::F64 => !matches!(
                        op,
                        BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Shl | BinOp::Shr | BinOp::ShrU
                    ),
                    Type::Bool => matches!(op, BinOp::Eq | BinOp::Ne),
                    Type::JsVal => false,
                };
                if !ok {
                    self.err(format!("{op:?} is not defined on {ty}"));
                }
                Some(if op.is_comparison() { Type::Bool } else { *ty })
            }
            Expr::Convert { from, to, arg } => {
                let t = self.value("convert operand", arg);
                self.expect("convert operand", t, *from);
                if !matches!((from, to), (Type::F64, Type::I32) | (Type::I32, Type::F64)) {
                    self.err(format!("conversion {from} -> {to} is not defined"));
                }
                Some(*to)
            }
            Expr::Cond {
                cond,
                then,
                els,
                ty,
            } => {
                let c = self.value("cond condition", cond);
                self.expect("cond condition", c, Type::Bool);
                let (a, b) = (self.value("cond then", then), self.value("cond else", els));
                self.expect("cond then", a, *ty);
                self.expect("cond else", b, *ty);
                Some(*ty)
            }
            Expr::Call { func, args } => {
                let Some(callee) = self.m.functions.get(func.0 as usize) else {
                    self.err(format!("call of missing function #{}", func.0));
                    return None;
                };
                if args.len() != callee.params as usize {
                    self.err(format!(
                        "call {}: {} args for {} params",
                        callee.name,
                        args.len(),
                        callee.params
                    ));
                }
                for (i, a) in args.iter().enumerate() {
                    let t = self.value("call argument", a);
                    if let Some(p) = callee.locals.get(i).filter(|_| i < callee.params as usize) {
                        self.expect(&format!("call {} arg {i}", callee.name), t, p.ty);
                    }
                }
                callee.result
            }
            Expr::Suspend { arg, .. } => {
                self.value("suspend operand", arg);
                Some(Type::JsVal)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Expr, FuncId, Function, LabelId, Stmt, Type};

    fn module(f: Function) -> Module {
        let mut m = Module::default();
        m.add_function(f);
        m
    }

    #[test]
    fn well_formed_is_ok() {
        let mut f = Function::new("f", &[("a", Type::F64)], Some(Type::F64));
        f.body = vec![Stmt::Loop {
            label: LabelId(0),
            body: vec![
                Stmt::If {
                    cond: Expr::bool(true),
                    then: vec![Stmt::Break(LabelId(0))],
                    els: vec![],
                },
                Stmt::Continue(LabelId(0)),
            ],
        }];
        f.body.push(Stmt::Return(Some(Expr::local(f.param(0)))));
        assert_eq!(validate(&module(f)), Ok(()));
    }

    #[test]
    fn catches_bad_labels_types_and_calls() {
        let mut f = Function::new("f", &[("a", Type::F64)], Some(Type::F64));
        f.body = vec![
            Stmt::Block {
                label: LabelId(0),
                body: vec![Stmt::Continue(LabelId(0))],
            },
            Stmt::Break(LabelId(7)),
            Stmt::If {
                cond: Expr::f64(1.0),
                then: vec![],
                els: vec![],
            },
            Stmt::Set(f.param(0), Expr::i32(1)),
            Stmt::Expr(Expr::binary(
                crate::BinOp::Shl,
                Type::F64,
                Expr::f64(1.0),
                Expr::f64(2.0),
            )),
            Stmt::Expr(Expr::Call {
                func: FuncId(0),
                args: vec![],
            }),
            Stmt::Return(None),
        ];
        let errs = validate(&module(f)).unwrap_err();
        let want = [
            "func f#0: continue L0: label is a block, not a loop",
            "func f#0: break L7: no enclosing block or loop with that label",
            "func f#0: if condition: expected bool, got f64",
            "func f#0: set .0: expected f64, got i32",
            "func f#0: Shl is not defined on f64",
            "func f#0: call f: 0 args for 1 params",
            "func f#0: return without a value in a function returning f64",
        ];
        assert_eq!(errs, want);
    }
}
