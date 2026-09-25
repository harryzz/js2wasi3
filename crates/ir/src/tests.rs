use super::*;

fn add_module() -> Module {
    let mut m = Module::default();
    let mut f = Function::new(
        "add",
        &[("a", Type::F64), ("b", Type::F64)],
        Some(Type::F64),
    );
    let (a, b) = (f.param(0), f.param(1));
    f.body.push(Stmt::Return(Some(Expr::binary(
        BinOp::Add,
        Type::F64,
        Expr::local(a),
        Expr::local(b),
    ))));
    let id = m.add_function(f);
    m.exports.push(Export {
        name: "add".into(),
        func: id,
        string_name: false,
        pos: None,
        sig: AbiSig::numbers(2, true),
    });
    m
}

#[test]
fn add_prints() {
    let expected = "\
func add#0(a.0: f64, b.1: f64) -> f64 {
  return f64.add(a.0, b.1)
}
export \"add\" = add#0
";
    assert_eq!(add_module().to_string(), expected);
}

/// Every node kind once, including the placeholders (`Try`, `Throw`, `Suspend`).
/// Mirrors the lowering of:
/// ```js
/// let count = 0;
/// function sum(n) { let s = 0; for (let i = 0; i < n; i++) { if (i % 2) continue; s += i; } count++; return s; }
/// ```
#[test]
fn all_kinds_print() {
    let mut m = Module::default();
    m.globals.push(Global {
        name: "count".into(),
        ty: Type::I32,
        mutable: true,
        init: Const::I32(0),
    });
    let count = GlobalId(0);

    let mut f = Function::new("sum", &[("n", Type::F64)], Some(Type::F64));
    let n = f.param(0);
    let s = f.add_local("s", Type::F64);
    let i = f.add_local("i", Type::F64);
    let (brk, cont) = (LabelId(0), LabelId(1));
    f.body = vec![
        Stmt::Set(s, Expr::f64(0.0)),
        Stmt::Set(i, Expr::f64(0.0)),
        Stmt::Loop {
            label: brk,
            body: vec![
                Stmt::If {
                    cond: Expr::unary(
                        UnOp::Not,
                        Type::Bool,
                        Expr::binary(BinOp::Lt, Type::F64, Expr::local(i), Expr::local(n)),
                    ),
                    then: vec![Stmt::Break(brk)],
                    els: vec![],
                },
                Stmt::Block {
                    label: cont,
                    body: vec![Stmt::If {
                        cond: Expr::binary(
                            BinOp::Ne,
                            Type::F64,
                            Expr::binary(BinOp::Rem, Type::F64, Expr::local(i), Expr::f64(2.0)),
                            Expr::f64(0.0),
                        ),
                        then: vec![Stmt::Break(cont)],
                        els: vec![Stmt::Set(
                            s,
                            Expr::binary(BinOp::Add, Type::F64, Expr::local(s), Expr::local(i)),
                        )],
                    }],
                },
                Stmt::Set(
                    i,
                    Expr::binary(BinOp::Add, Type::F64, Expr::local(i), Expr::f64(1.0)),
                ),
                Stmt::Continue(brk),
            ],
        },
        Stmt::SetGlobal(
            count,
            Expr::binary(BinOp::Add, Type::I32, Expr::Global(count), Expr::i32(1)),
        ),
        Stmt::Return(Some(Expr::local(s))),
    ];
    let sum = m.add_function(f);

    let mut g = Function::new("misc", &[("x", Type::F64)], None);
    g.flags = FnFlags {
        is_async: true,
        is_generator: true,
    };
    let x = g.param(0);
    let t = g.add_local("t", Type::I32);
    let e = g.add_local("e", Type::JsVal);
    g.body = vec![
        Stmt::Expr(Expr::Call {
            func: sum,
            args: vec![Expr::local(x)],
        }),
        Stmt::Set(
            t,
            Expr::unary(
                UnOp::BitNot,
                Type::I32,
                Expr::Tee(
                    t,
                    Box::new(Expr::Convert {
                        from: Type::F64,
                        to: Type::I32,
                        arg: Box::new(Expr::local(x)),
                    }),
                ),
            ),
        ),
        Stmt::Expr(Expr::Cond {
            cond: Box::new(Expr::bool(true)),
            then: Box::new(Expr::unary(UnOp::Neg, Type::F64, Expr::f64(f64::NAN))),
            els: Box::new(Expr::f64(-0.0)),
            ty: Type::F64,
        }),
        Stmt::Try {
            body: vec![Stmt::Throw(Expr::local(e))],
            catch_local: Some(e),
            catch_body: vec![Stmt::Expr(Expr::Suspend {
                kind: SuspendKind::Await,
                arg: Box::new(Expr::local(e)),
            })],
        },
        Stmt::Try {
            body: vec![Stmt::Expr(Expr::Suspend {
                kind: SuspendKind::Yield,
                arg: Box::new(Expr::local(e)),
            })],
            catch_local: None,
            catch_body: vec![],
        },
        Stmt::Return(None),
    ];
    m.add_function(g);
    m.exports.push(Export {
        name: "sum".into(),
        func: sum,
        string_name: false,
        pos: None,
        sig: AbiSig::numbers(1, true),
    });

    let expected = "\
global @count: i32 mut = 0_i32
func sum#0(n.0: f64) -> f64 {
  local s.1: f64
  local i.2: f64
  s.1 = 0.0
  i.2 = 0.0
  loop L0 {
    if bool.not(f64.lt(i.2, n.0)) {
      break L0
    }
    block L1 {
      if f64.ne(f64.rem(i.2, 2.0), 0.0) {
        break L1
      } else {
        s.1 = f64.add(s.1, i.2)
      }
    }
    i.2 = f64.add(i.2, 1.0)
    continue L0
  }
  @count = i32.add(@count, 1_i32)
  return s.1
}
async func* misc#1(x.0: f64) {
  local t.1: i32
  local e.2: jsval
  call sum#0(x.0)
  t.1 = i32.bitnot((t.1 := f64->i32(x.0)))
  cond.f64(true, f64.neg(NaN), -0.0)
  try {
    throw e.2
  } catch (e.2) {
    await(e.2)
  }
  try {
    yield(e.2)
  } catch {
  }
  return
}
export \"sum\" = sum#0
";
    assert_eq!(m.to_string(), expected);
}

#[test]
fn expr_types() {
    let mut m = add_module();
    m.functions.push(Function::new("noret", &[], None));
    let f = m.function(FuncId(0));
    let a = Expr::local(f.param(0));
    assert_eq!(a.ty(f, &m), Some(Type::F64));
    assert_eq!(
        Expr::binary(BinOp::Lt, Type::F64, a.clone(), a.clone()).ty(f, &m),
        Some(Type::Bool)
    );
    assert_eq!(
        Expr::binary(BinOp::Mul, Type::F64, a.clone(), a.clone()).ty(f, &m),
        Some(Type::F64)
    );
    assert_eq!(
        Expr::Convert {
            from: Type::F64,
            to: Type::I32,
            arg: Box::new(a.clone())
        }
        .ty(f, &m),
        Some(Type::I32)
    );
    assert_eq!(
        Expr::Call {
            func: FuncId(0),
            args: vec![a.clone(), a]
        }
        .ty(f, &m),
        Some(Type::F64)
    );
    assert_eq!(
        Expr::Call {
            func: FuncId(1),
            args: vec![]
        }
        .ty(f, &m),
        None
    );
}
