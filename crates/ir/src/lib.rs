//! IR: the single mid-level representation between the frontend and the backend.
//! JS-aware (closures, exceptions, suspension points) but independent of Wasm encoding.
//!
//! # Shape
//! A **structured tree**: statements nest like Wasm `block`/`loop`/`if`, so the backend
//! emits them directly, with no CFG and no relooper. Every function owns a flat, numbered
//! local list (params first, like Wasm), which keeps later liveness passes (the T18
//! suspension transform) cheap. The IR holds no oxc types. Source positions are plain [`Pos`].
//!
//! # Node kinds
//! Statements ([`Stmt`]):
//! - `Expr(e)`: evaluate `e` and discard its value.
//! - `Set(l, e)` / `SetGlobal(g, e)`: assign to a local / global.
//! - `Block { label, body }`: `Break(label)` jumps to just after the block.
//! - `Loop { label, body }`: `Continue(label)` jumps to the start of the body, `Break(label)`
//!   jumps to just after the loop. The body does not repeat implicitly: it ends with an
//!   explicit `Continue`, or the loop exits after one pass (matching Wasm's `loop`).
//! - `If { cond, then, els }`: `cond` has type `Bool`.
//! - `Break(label)` / `Continue(label)`: labels must name an enclosing `Block`/`Loop`
//!   (`Continue` only a `Loop`).
//! - `Return(e?)`.
//! - `Try { body, catch_local, catch_body }` *(placeholder, T12)*: JS `try/catch`.
//!   `finally` is lowered away before reaching the IR.
//! - `Throw(e)` *(placeholder, T12)*: throw a JS value.
//!
//! Expressions ([`Expr`]):
//! - `Const(c)`: `f64`, `i32` or `bool` literal.
//! - `Local(l)` / `Global(g)`: read.
//! - `Tee(l, e)`: assign to a local and yield the value (JS assignment expressions).
//! - `Unary { op, ty, arg }` / `Binary { op, ty, lhs, rhs }`: machine ops on operands of type
//!   `ty` (`F64` or `I32`, `Bool` for logical `Not`/`Eq`/`Ne`). Comparisons yield `Bool`.
//!   These are *not* JS operators. JS `+` on dynamic values arrives in T08 as its own node.
//! - `Convert { from, to, arg }`: numeric conversion. `F64 → I32` is JS `ToInt32` (modulo 2^32, NaN/±inf → 0),
//!   not a saturating truncation. `I32 → F64` is signed.
//! - `Cond { cond, then, els, ty }`: conditional expression (`?:`, `&&`, `||` after lowering).
//! - `Call { func, args }`: direct call.
//! - `Suspend { kind, arg }` *(placeholder, T18/T19)*: `await` / `yield` point.

use std::fmt;

mod print;
mod validate;

pub use validate::validate;

/// Value types. Wasm types get chosen in the backend: `Bool` is an `i32` 0/1, `JsVal` is `(ref null eq)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Type {
    F64,
    I32,
    Bool,
    /// Dynamic JS value (T08).
    JsVal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlobalId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FuncId(pub u32);
/// Unique within one function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LabelId(pub u32);

/// 1-based source position, for diagnostics about unsupported IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Module {
    pub functions: Vec<Function>,
    pub globals: Vec<Global>,
    pub exports: Vec<Export>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
    pub init: Const,
}

/// A function exported from the component under `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub func: FuncId,
    /// `name` was written as a string literal in the source (`export { f as "my-f" }`).
    pub string_name: bool,
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FnFlags {
    pub is_async: bool,
    pub is_generator: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    /// Source name, or a synthetic name such as `tmp` (printing adds the index, so names needn't be unique).
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    /// The first `params` entries of `locals` are the parameters.
    pub params: u32,
    pub locals: Vec<Local>,
    pub result: Option<Type>,
    pub flags: FnFlags,
    pub body: Vec<Stmt>,
    pub pos: Option<Pos>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Const {
    F64(f64),
    I32(i32),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    /// `F64`: negate.
    Neg,
    /// `F64`: absolute value (used for number truthiness: `abs(x) > 0` is false for ±0 and NaN).
    Abs,
    /// `Bool`: logical not.
    Not,
    /// `I32`: bitwise not.
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    /// `F64`: JS `%` (truncated remainder, sign of dividend). `I32`: signed remainder.
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// `I32` only from here on.
    And,
    Or,
    Xor,
    Shl,
    /// Arithmetic (signed) shift right: JS `>>`.
    Shr,
    /// Logical shift right: JS `>>>`.
    ShrU,
}

impl BinOp {
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SuspendKind {
    Await,
    Yield,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expr(Expr),
    Set(LocalId, Expr),
    SetGlobal(GlobalId, Expr),
    Block {
        label: LabelId,
        body: Vec<Stmt>,
    },
    Loop {
        label: LabelId,
        body: Vec<Stmt>,
    },
    If {
        cond: Expr,
        then: Vec<Stmt>,
        els: Vec<Stmt>,
    },
    Break(LabelId),
    Continue(LabelId),
    Return(Option<Expr>),
    Try {
        body: Vec<Stmt>,
        catch_local: Option<LocalId>,
        catch_body: Vec<Stmt>,
    },
    Throw(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Const(Const),
    Local(LocalId),
    Global(GlobalId),
    Tee(LocalId, Box<Expr>),
    Unary {
        op: UnOp,
        ty: Type,
        arg: Box<Expr>,
    },
    Binary {
        op: BinOp,
        ty: Type,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Convert {
        from: Type,
        to: Type,
        arg: Box<Expr>,
    },
    Cond {
        cond: Box<Expr>,
        then: Box<Expr>,
        els: Box<Expr>,
        ty: Type,
    },
    Call {
        func: FuncId,
        args: Vec<Expr>,
    },
    Suspend {
        kind: SuspendKind,
        arg: Box<Expr>,
    },
}

impl Expr {
    pub fn f64(v: f64) -> Expr {
        Expr::Const(Const::F64(v))
    }
    pub fn i32(v: i32) -> Expr {
        Expr::Const(Const::I32(v))
    }
    pub fn bool(v: bool) -> Expr {
        Expr::Const(Const::Bool(v))
    }
    pub fn local(l: LocalId) -> Expr {
        Expr::Local(l)
    }
    pub fn unary(op: UnOp, ty: Type, arg: Expr) -> Expr {
        Expr::Unary {
            op,
            ty,
            arg: Box::new(arg),
        }
    }
    pub fn binary(op: BinOp, ty: Type, lhs: Expr, rhs: Expr) -> Expr {
        Expr::Binary {
            op,
            ty,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    /// Result type, or `None` for a call to a function without a result.
    /// Assumes well-formed IR (ids in range).
    pub fn ty(&self, func: &Function, module: &Module) -> Option<Type> {
        Some(match self {
            Expr::Const(Const::F64(_)) => Type::F64,
            Expr::Const(Const::I32(_)) => Type::I32,
            Expr::Const(Const::Bool(_)) => Type::Bool,
            Expr::Local(l) | Expr::Tee(l, _) => func.local(*l).ty,
            Expr::Global(g) => module.globals[g.0 as usize].ty,
            Expr::Unary { ty, .. } => *ty,
            Expr::Binary { op, ty, .. } => {
                if op.is_comparison() {
                    Type::Bool
                } else {
                    *ty
                }
            }
            Expr::Convert { to, .. } => *to,
            Expr::Cond { ty, .. } => *ty,
            Expr::Call { func: f, .. } => return module.functions[f.0 as usize].result,
            Expr::Suspend { .. } => Type::JsVal,
        })
    }
}

impl Function {
    /// A function with the given params and no extra locals or body yet.
    pub fn new(name: impl Into<String>, params: &[(&str, Type)], result: Option<Type>) -> Function {
        Function {
            name: name.into(),
            params: params.len() as u32,
            locals: params
                .iter()
                .map(|(n, ty)| Local {
                    name: n.to_string(),
                    ty: *ty,
                })
                .collect(),
            result,
            flags: FnFlags::default(),
            body: Vec::new(),
            pos: None,
        }
    }

    pub fn param(&self, i: u32) -> LocalId {
        assert!(i < self.params, "param {i} out of range");
        LocalId(i)
    }

    pub fn add_local(&mut self, name: impl Into<String>, ty: Type) -> LocalId {
        self.locals.push(Local {
            name: name.into(),
            ty,
        });
        LocalId(self.locals.len() as u32 - 1)
    }

    pub fn local(&self, l: LocalId) -> &Local {
        &self.locals[l.0 as usize]
    }
}

impl Module {
    pub fn add_function(&mut self, f: Function) -> FuncId {
        self.functions.push(f);
        FuncId(self.functions.len() as u32 - 1)
    }

    pub fn function(&self, f: FuncId) -> &Function {
        &self.functions[f.0 as usize]
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Type::F64 => "f64",
            Type::I32 => "i32",
            Type::Bool => "bool",
            Type::JsVal => "jsval",
        })
    }
}

#[cfg(test)]
mod tests;
