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
//! - `Const(c)`: `f64`, `i32` or `bool` literal, or `undefined` / `null` (type `jsval`).
//! - `Local(l)` / `Global(g)`: read.
//! - `Tee(l, e)`: assign to a local and yield the value (JS assignment expressions).
//! - `Unary { op, ty, arg }` / `Binary { op, ty, lhs, rhs }`: machine ops on operands of type
//!   `ty` (`F64` or `I32`, `Bool` for logical `Not`/`Eq`/`Ne`). Comparisons yield `Bool`.
//!   These are machine ops, except on `JsVal`: `Eq`/`Ne` are JS `===`/`!==` and `LooseEq`/`LooseNe`
//!   are JS `==`/`!=` (both only on `JsVal`). Arithmetic on JS values goes through `JsVal → F64` first.
//! - `TypeofIs { arg, tag }`: `typeof arg === "<tag>"`, as `Bool` (`arg` is `JsVal`).
//! - `Str(units)`: a string literal (UTF-16 code units), type `JsVal`.
//! - `Intrinsic { op, args }`: a runtime operation with a fixed signature ([`Intrinsic::signature`]),
//!   e.g. JS `+` on JS values or ToString.
//! - `Seq { body, value }`: run `body`, then evaluate `value`. `body` must not `Break`/`Continue` out.
//! - `Convert { from, to, arg }`: `F64 → I32` is JS `ToInt32` (modulo 2^32, NaN/±inf → 0),
//!   not a saturating truncation. `I32 → F64` is signed. `F64 → JsVal` boxes a number, `JsVal → F64` is
//!   JS `ToNumber`, `Bool → JsVal` picks the `true`/`false` value, `JsVal → Bool` is JS `ToBoolean`.
//!   (T08: primitives only; strings and objects extend the runtime helpers.)
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
    /// Types at the component boundary. `func` takes and returns `f64` for [`AbiType::F64`]
    /// and a JS string (`JsVal`) for [`AbiType::String`].
    pub sig: AbiSig,
}

/// A value type at the component boundary (a WIT type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbiType {
    F64,
    String,
}

impl AbiType {
    /// The IR type the exported function uses for it.
    pub fn ir_type(self) -> Type {
        match self {
            AbiType::F64 => Type::F64,
            AbiType::String => Type::JsVal,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AbiSig {
    pub params: Vec<AbiType>,
    pub result: Option<AbiType>,
}

impl AbiSig {
    /// All `f64` (the T07/T08 boundary), for a function with `params` parameters.
    pub fn numbers(params: u32, result: bool) -> AbiSig {
        AbiSig {
            params: vec![AbiType::F64; params as usize],
            result: result.then_some(AbiType::F64),
        }
    }
}

/// A runtime operation on JS values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Intrinsic {
    /// JS `a + b` on two JS values (concatenation if either is a string).
    JsAdd,
    /// Concatenation of two JS strings.
    Concat,
    /// JS ToString of a JS value (a string).
    ToString,
    /// JS Number::toString of an `f64` (a string).
    NumberToString,
    /// `v.length`.
    Length,
    /// `typeof v` (a string).
    TypeofString,
}

impl Intrinsic {
    pub fn signature(self) -> (&'static [Type], Type) {
        use Type::*;
        match self {
            Intrinsic::JsAdd | Intrinsic::Concat => (&[JsVal, JsVal], JsVal),
            Intrinsic::ToString | Intrinsic::Length | Intrinsic::TypeofString => (&[JsVal], JsVal),
            Intrinsic::NumberToString => (&[F64], JsVal),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Intrinsic::JsAdd => "js_add",
            Intrinsic::Concat => "concat",
            Intrinsic::ToString => "to_string",
            Intrinsic::NumberToString => "number_to_string",
            Intrinsic::Length => "length",
            Intrinsic::TypeofString => "typeof",
        }
    }
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
    /// `jsval`.
    Undefined,
    /// `jsval`.
    Null,
}

impl Const {
    pub fn ty(&self) -> Type {
        match self {
            Const::F64(_) => Type::F64,
            Const::I32(_) => Type::I32,
            Const::Bool(_) => Type::Bool,
            Const::Undefined | Const::Null => Type::JsVal,
        }
    }
}

/// A result of JS `typeof`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeofTag {
    Undefined,
    Object,
    Boolean,
    Number,
    String,
    Symbol,
    Function,
    BigInt,
}

impl TypeofTag {
    pub const ALL: [TypeofTag; 8] = [
        TypeofTag::Undefined,
        TypeofTag::Object,
        TypeofTag::Boolean,
        TypeofTag::Number,
        TypeofTag::String,
        TypeofTag::Symbol,
        TypeofTag::Function,
        TypeofTag::BigInt,
    ];

    /// The string `typeof` returns.
    pub fn as_str(self) -> &'static str {
        match self {
            TypeofTag::Undefined => "undefined",
            TypeofTag::Object => "object",
            TypeofTag::Boolean => "boolean",
            TypeofTag::Number => "number",
            TypeofTag::String => "string",
            TypeofTag::Symbol => "symbol",
            TypeofTag::Function => "function",
            TypeofTag::BigInt => "bigint",
        }
    }

    pub fn parse(s: &str) -> Option<TypeofTag> {
        TypeofTag::ALL.into_iter().find(|t| t.as_str() == s)
    }
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
    /// `JsVal` only: JS `==`.
    LooseEq,
    /// `JsVal` only: JS `!=`.
    LooseNe,
}

impl BinOp {
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::LooseEq
                | BinOp::LooseNe
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
    TypeofIs {
        arg: Box<Expr>,
        tag: TypeofTag,
    },
    Str(Vec<u16>),
    Intrinsic {
        op: Intrinsic,
        args: Vec<Expr>,
    },
    Seq {
        body: Vec<Stmt>,
        value: Box<Expr>,
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
    pub fn undefined() -> Expr {
        Expr::Const(Const::Undefined)
    }
    pub fn str(s: &str) -> Expr {
        Expr::Str(s.encode_utf16().collect())
    }
    pub fn intrinsic(op: Intrinsic, args: Vec<Expr>) -> Expr {
        Expr::Intrinsic { op, args }
    }
    pub fn convert(from: Type, to: Type, arg: Expr) -> Expr {
        Expr::Convert {
            from,
            to,
            arg: Box::new(arg),
        }
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
            Expr::Const(c) => c.ty(),
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
            Expr::TypeofIs { .. } => Type::Bool,
            Expr::Str(_) => Type::JsVal,
            Expr::Intrinsic { op, .. } => op.signature().1,
            Expr::Seq { value, .. } => return value.ty(func, module),
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
