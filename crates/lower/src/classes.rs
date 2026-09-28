//! Classes (T16): class definitions, constructors, `super`, `new.target`.
//!
//! A class is a function object ([`ir::FnKind::Class`]): calling it runs a trap
//! (`X$call`: "Class constructor X cannot be invoked without 'new'"), `new` runs its construct
//! entry (`X$construct(self, new_target, args)`), which calls the constructor's IR function (see
//! [`Lowerer::declare_class`]). A base constructor gets a new `this` (`CreateThis`); a derived one
//! starts with `this` uninitialized (a `Hole`, checked on every read) until `super(…)` constructs it
//! with the parent class (the class's prototype) and the same `new.target`.
//!
//! Evaluating a class ([`Lowerer::class_value`]): the heritage (checked by `ClassHeritage`), the
//! function object and its `prototype` (`ClassSetup`), the methods and accessors (non-enumerable,
//! on the prototype or, static, on the class), the binding of a class declaration, then the static
//! fields with `this` = the class. Instance fields are defined by the constructor: at its start
//! (base) or after each `super(…)` (derived). `super.x` reads the home object's prototype
//! (`SuperGet`) with the current `this` as the receiver.

use std::collections::HashMap;

use js2wasi3_ir::{
    self as ir, Builtin, ErrorKind, Expr, FnKind, FuncId, Intrinsic, LocalId, PrivateKind, Stmt,
    Type, UnOp,
};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeId;
use oxc_syntax::symbol::SymbolId;

use super::{FnCx, Kind, Lowerer, R, Slot, Val, always_returns, js_length, jsval_of, utf16};

/// node's ReferenceError for `this` before `super()` in a derived constructor.
const UNINIT_THIS: &str = "Must call super constructor in derived class before accessing 'this' or returning from derived constructor";

impl<'p, 'a> Lowerer<'p, 'a> {
    /// Evaluate a class definition: the class object. `name`: the name it gets from where it's
    /// defined (`const A = class {}`); `binding`: a class declaration's binding, initialized before
    /// the static fields run.
    pub(crate) fn class_value(
        &mut self,
        cx: &mut FnCx,
        c: &'a Class<'a>,
        name: Option<&str>,
        binding: Option<SymbolId>,
    ) -> R<Expr> {
        let scope = c.scope_id.get().expect("classes have scopes");
        let mut body = Vec::new();
        let heritage = cx.func.add_local("heritage", Type::JsVal);
        let proto_parent = cx.func.add_local("proto_parent", Type::JsVal);
        match &c.heritage {
            Some(h) => {
                let v = self.expr(cx, &h.expression)?;
                body.push(Stmt::Set(heritage, jsval_of(v)));
                body.push(Stmt::Set(
                    proto_parent,
                    Expr::intrinsic(Intrinsic::ClassHeritage, vec![Expr::Local(heritage)]),
                ));
            }
            None => {
                body.push(Stmt::Set(heritage, Expr::undefined()));
                body.push(Stmt::Set(
                    proto_parent,
                    Expr::intrinsic(Intrinsic::Prototype(Builtin::Object), vec![]),
                ));
            }
        }
        let class_name =
            c.id.as_ref()
                .map(|i| i.name.to_string())
                .or(name.map(str::to_string))
                .unwrap_or_default();
        let class_name = self.p.original_name(&class_name).to_string();
        // A new symbol per private name, a cell per instance private method or accessor (filled
        // below, read by the constructor), and a getter and a setter local per private accessor.
        let mut accessors = HashMap::new();
        for (i, p) in self.an.privates.iter().enumerate() {
            if p.class != scope {
                continue;
            }
            let l = cx.func.add_local(p.name.clone(), Type::JsVal);
            body.push(Stmt::Set(
                l,
                Expr::intrinsic(Intrinsic::NewSymbol, vec![Expr::str(&p.name)]),
            ));
            cx.privates.insert(i as u32, l);
            if p.method && !p.is_static {
                let cell = cx.func.add_local(format!("{}.cell", p.name), Type::JsVal);
                body.push(Stmt::Set(
                    cell,
                    Expr::intrinsic(Intrinsic::NewCell(Type::JsVal), vec![Expr::undefined()]),
                ));
                cx.priv_cells.insert(i as u32, cell);
            }
            if p.accessor {
                let get = cx.func.add_local(format!("{}.get", p.name), Type::JsVal);
                let set = cx.func.add_local(format!("{}.set", p.name), Type::JsVal);
                body.push(Stmt::Set(get, Expr::undefined()));
                body.push(Stmt::Set(set, Expr::undefined()));
                accessors.insert(i as u32, (get, set));
            }
        }
        // A cell per computed field key: the class object (made before its elements run) captures
        // the instance ones.
        for el in &c.body.body {
            if let ClassElement::PropertyDefinition(p) = el
                && p.computed
            {
                let cell = cx.func.add_local("key.cell", Type::JsVal);
                body.push(Stmt::Set(
                    cell,
                    Expr::intrinsic(Intrinsic::NewCell(Type::JsVal), vec![Expr::undefined()]),
                ));
                cx.field_keys.insert(p.span.start, cell);
            }
        }
        // The constructor is only called through its construct entry: its parameters can be anything.
        let id = self.classes[&scope].id;
        for p in self.func_params[&id].clone().into_iter().flatten() {
            self.observe(Slot::Var(p), Kind::Any);
        }
        let lowered = self.lower_class(scope);
        let (call, construct) = self.class_adapters(scope, &class_name);
        lowered?;
        let env = self.env_for(cx, scope, c.span)?;
        let length = self.classes[&scope]
            .ctor
            .map_or(0, |f| js_length(&f.params));
        let k = cx.func.add_local(
            if class_name.is_empty() {
                "class"
            } else {
                &class_name
            },
            Type::JsVal,
        );
        body.push(Stmt::Set(
            k,
            Expr::Closure(Box::new(ir::Closure {
                code: call,
                name: class_name,
                length,
                kind: FnKind::Class { construct },
                env,
                shared: false,
                source: self.source_text(c.span),
            })),
        ));
        let proto = cx.func.add_local("prototype", Type::JsVal);
        body.push(Stmt::Set(
            proto,
            Expr::intrinsic(
                Intrinsic::ClassSetup,
                vec![
                    Expr::Local(k),
                    Expr::Local(heritage),
                    Expr::Local(proto_parent),
                ],
            ),
        ));
        if c.is_expression()
            && let Some(sym) = c.id.as_ref().and_then(|i| i.symbol_id.get())
        {
            cx.values.insert(sym, k);
        }
        let saved_home = cx.home;
        let mut statics = Vec::new();
        let mut result = Ok(());
        for el in &c.body.body {
            let r = match el {
                ClassElement::MethodDefinition(m)
                    if m.kind == MethodDefinitionKind::Constructor =>
                {
                    Ok(())
                }
                ClassElement::MethodDefinition(m) => {
                    self.class_method(cx, scope, m, k, proto, &accessors, &mut body)
                }
                ClassElement::PropertyDefinition(p) => {
                    if p.r#static {
                        statics.push(Static::Field(p));
                    }
                    if p.computed {
                        // The key is evaluated now, in order with the other keys (T31).
                        self.field_key(cx, p, &mut body)
                    } else {
                        Ok(())
                    }
                }
                ClassElement::StaticBlock(b) => {
                    statics.push(Static::Block(b));
                    Ok(())
                }
                ClassElement::AccessorProperty(a) => {
                    Err(self.unsupported(a.span, "`accessor` class field", "not scheduled"))
                }
                ClassElement::TSIndexSignature(t) => {
                    Err(self.unsupported(t.span, "TypeScript index signature", "not scheduled"))
                }
            };
            if let Err(e) = r {
                result = Err(e);
            }
        }
        cx.home = saved_home;
        std::mem::replace(&mut result, Ok(()))?;
        // Private accessors: a getter/setter pair, on the class (static) or in the cell the
        // constructor installs from.
        let mut accessors: Vec<_> = accessors.into_iter().collect();
        accessors.sort_by_key(|(i, _)| *i);
        for (i, (get, set)) in accessors {
            let pair = Expr::intrinsic(
                Intrinsic::NewAccessor,
                vec![Expr::Local(get), Expr::Local(set)],
            );
            body.push(Stmt::Expr(if self.an.privates[i as usize].is_static {
                Expr::intrinsic(
                    Intrinsic::PrivateAdd {
                        kind: PrivateKind::Accessor,
                    },
                    vec![
                        Expr::Local(k),
                        Expr::Local(cx.privates[&i]),
                        pair,
                        Expr::str(""),
                    ],
                )
            } else {
                Expr::intrinsic(
                    Intrinsic::CellSet(Type::JsVal),
                    vec![Expr::Local(cx.priv_cells[&i]), pair],
                )
            }));
        }
        if let Some(sym) = binding {
            let slot = Slot::Var(sym);
            self.observe(slot, Kind::Any);
            let ty = self.slot_ty(slot);
            let v = self.store(slot, ty, Val::js(Expr::Local(k), Kind::Any));
            body.extend(self.declare_store(cx, sym, ty, v, true));
            if let Some(flag) = self.tdz_flags.get(&sym) {
                body.push(Stmt::SetGlobal(*flag, Expr::bool(true)));
            }
        }
        // Static fields and blocks, in order: `this` and the home object are the class.
        let saved = (cx.this, cx.this_checked, cx.home);
        cx.this = Some(k);
        cx.this_checked = false;
        cx.home = Some(k);
        for st in statics {
            let r = match st {
                Static::Field(p) => self.field(cx, scope, p, Expr::Local(k), &mut body),
                // Its `var`s are its own: the symbols are the block's, lowered as locals here.
                Static::Block(b) => {
                    let scope = b.scope_id.get().expect("static blocks have scopes");
                    self.var_cells(cx, scope, |_| false, &mut body);
                    body.extend(self.stmts(cx, &b.body));
                    Ok(())
                }
            };
            if let Err(e) = r {
                result = Err(e);
            }
        }
        (cx.this, cx.this_checked, cx.home) = saved;
        result?;
        Ok(Expr::Seq {
            body,
            value: Box::new(Expr::Local(k)),
        })
    }

    /// A method, getter or setter: defined on the prototype (or the class, `static`).
    #[allow(clippy::too_many_arguments)]
    fn class_method(
        &mut self,
        cx: &mut FnCx,
        scope: ScopeId,
        m: &'a MethodDefinition<'a>,
        class: LocalId,
        proto: LocalId,
        accessors: &HashMap<u32, (LocalId, LocalId)>,
        body: &mut Vec<Stmt>,
    ) -> R<()> {
        let target = if m.r#static { class } else { proto };
        if let PropertyKey::PrivateIdentifier(id) = &m.key {
            // A private method: installed on the class now (static), or on each instance by the
            // constructor (from its cell). A getter or setter goes into its local, paired after
            // the class body.
            let name = format!("#{}", id.name);
            let i = self.private_id(scope, &name);
            let fscope = m.value.scope_id.get().expect("functions have scopes");
            cx.home = Some(target);
            let prefix = match m.kind {
                MethodDefinitionKind::Get => "get ",
                MethodDefinitionKind::Set => "set ",
                _ => "",
            };
            let f = self.closure(cx, fscope, Some(&format!("{prefix}{name}")), m.span)?;
            if !prefix.is_empty() {
                let (get, set) = accessors[&i];
                let l = if m.kind == MethodDefinitionKind::Get {
                    get
                } else {
                    set
                };
                body.push(Stmt::Set(l, f));
                return Ok(());
            }
            let sym = Expr::Local(cx.privates[&i]);
            body.push(Stmt::Expr(if m.r#static {
                Expr::intrinsic(
                    Intrinsic::PrivateAdd {
                        kind: PrivateKind::Method,
                    },
                    vec![Expr::Local(class), sym, f, Expr::str("")],
                )
            } else {
                Expr::intrinsic(
                    Intrinsic::CellSet(Type::JsVal),
                    vec![Expr::Local(cx.priv_cells[&i]), f],
                )
            }));
            return Ok(());
        }
        let (key, name) = self.member_key(cx, &m.key, m.computed, body)?;
        let (prefix, op) = match m.kind {
            MethodDefinitionKind::Get => (
                "get ",
                Intrinsic::DefineAccessor {
                    setter: false,
                    enumerable: false,
                },
            ),
            MethodDefinitionKind::Set => (
                "set ",
                Intrinsic::DefineAccessor {
                    setter: true,
                    enumerable: false,
                },
            ),
            _ => ("", Intrinsic::DefineMethod),
        };
        let fname = name.as_ref().map(|name| format!("{prefix}{name}"));
        let scope = m.value.scope_id.get().expect("functions have scopes");
        cx.home = Some(target);
        let mut f = self.closure(cx, scope, fname.as_deref(), m.span)?;
        if name.is_none() {
            // A computed key: the name comes from it at run time (F59).
            f = Expr::intrinsic(
                Intrinsic::SetFunctionName,
                vec![f, key.clone(), Expr::str(prefix)],
            );
        }
        body.push(Stmt::Expr(Expr::intrinsic(
            op,
            vec![Expr::Local(target), key, f],
        )));
        Ok(())
    }

    /// A class element's key (evaluated into a local if computed, so it runs in order) and, when
    /// it's known statically, the name a function defined there gets.
    fn member_key(
        &mut self,
        cx: &mut FnCx,
        key: &'a PropertyKey<'a>,
        computed: bool,
        body: &mut Vec<Stmt>,
    ) -> R<(Expr, Option<String>)> {
        match key {
            PropertyKey::StaticIdentifier(id) => {
                Ok((Expr::str(id.name.as_str()), Some(id.name.to_string())))
            }
            PropertyKey::PrivateIdentifier(id) => {
                Err(self.unsupported(id.span, "private name `#x`", "later"))
            }
            PropertyKey::StringLiteral(sl) if !computed => Ok((
                Expr::Str(utf16(sl.value.as_str(), sl.lone_surrogates)),
                Some(sl.value.to_string()),
            )),
            PropertyKey::StaticMemberExpression(m)
                if computed
                    && m.property.name == "iterator"
                    && self.builtin_ident(&m.object) == Some(Builtin::Symbol) =>
            {
                Ok((
                    Expr::intrinsic(Intrinsic::SymbolIterator, vec![]),
                    Some("[Symbol.iterator]".to_string()),
                ))
            }
            k => {
                let e = k.as_expression().expect("the other property keys");
                let v = self.expr(cx, e)?;
                let t = cx.func.add_local("key", Type::JsVal);
                body.push(Stmt::Set(t, jsval_of(v)));
                Ok((Expr::Local(t), None))
            }
        }
    }

    /// A computed field key: evaluated and converted (ToPropertyKey) when the class is, into its
    /// cell for [`Lowerer::field`] (an instance field's cell is captured by the constructor).
    fn field_key(
        &mut self,
        cx: &mut FnCx,
        p: &'a PropertyDefinition<'a>,
        body: &mut Vec<Stmt>,
    ) -> R<()> {
        let (key, _) = self.member_key(cx, &p.key, true, body)?;
        body.push(Stmt::Expr(Expr::intrinsic(
            Intrinsic::CellSet(Type::JsVal),
            vec![
                Expr::Local(cx.field_keys[&p.span.start]),
                Expr::intrinsic(Intrinsic::ToPropertyKey, vec![key]),
            ],
        )));
        Ok(())
    }

    /// A field of class `scope`: define it on `target` (the class, or `this` in a constructor).
    fn field(
        &mut self,
        cx: &mut FnCx,
        scope: ScopeId,
        p: &'a PropertyDefinition<'a>,
        target: Expr,
        body: &mut Vec<Stmt>,
    ) -> R<()> {
        if let PropertyKey::PrivateIdentifier(id) = &p.key {
            let name = format!("#{}", id.name);
            let i = self.private_id(scope, &name);
            let value = match &p.value {
                Some(v) => jsval_of(self.named(cx, v, Some(&name))?),
                None => Expr::undefined(),
            };
            body.push(Stmt::Expr(Expr::intrinsic(
                Intrinsic::PrivateAdd {
                    kind: PrivateKind::Field,
                },
                vec![
                    target,
                    Expr::Local(cx.privates[&i]),
                    value,
                    Expr::str(&format!(
                        "Cannot initialize {name} twice on the same object"
                    )),
                ],
            )));
            return Ok(());
        }
        let (key, name) = match cx.field_keys.get(&p.span.start) {
            Some(cell) if p.computed => {
                let l = cx.func.add_local("key", Type::JsVal);
                body.push(Stmt::Set(
                    l,
                    Expr::intrinsic(Intrinsic::CellGet(Type::JsVal), vec![Expr::Local(*cell)]),
                ));
                (Expr::Local(l), None)
            }
            _ => self.member_key(cx, &p.key, p.computed, body)?,
        };
        let value = match &p.value {
            Some(v) => {
                let f = jsval_of(self.named(cx, v, name.as_deref())?);
                if name.is_none() && crate::anonymous_function(v) {
                    // A computed key: the name comes from it at run time (F59).
                    Expr::intrinsic(
                        Intrinsic::SetFunctionName,
                        vec![f, key.clone(), Expr::str("")],
                    )
                } else {
                    f
                }
            }
            None => Expr::undefined(),
        };
        body.push(Stmt::Expr(Expr::intrinsic(
            Intrinsic::DefineField,
            vec![target, key, value],
        )));
        Ok(())
    }

    /// The instance fields of class `scope`, defined on `this` (in its constructor).
    fn field_inits(&mut self, cx: &mut FnCx, scope: ScopeId) -> R<Vec<Stmt>> {
        let c = self.classes[&scope].class;
        let this = cx.this.expect("a constructor has `this`");
        let mut out = Vec::new();
        // Instance private methods first.
        let class_name = c.id.as_ref().map_or("anonymous", |i| i.name.as_str());
        for (i, p) in self.an.privates.iter().enumerate() {
            let i = i as u32;
            if p.class == scope && p.method && !p.is_static {
                let kind = if p.accessor {
                    PrivateKind::Accessor
                } else {
                    PrivateKind::Method
                };
                out.push(Stmt::Expr(Expr::intrinsic(
                    Intrinsic::PrivateAdd { kind },
                    vec![
                        Expr::Local(this),
                        Expr::Local(cx.privates[&i]),
                        Expr::intrinsic(
                            Intrinsic::CellGet(Type::JsVal),
                            vec![Expr::Local(cx.priv_cells[&i])],
                        ),
                        Expr::str(&format!(
                            "Cannot initialize private methods of class {class_name} twice on the same object"
                        )),
                    ],
                )));
            }
        }
        let mut result = Ok(());
        for el in &c.body.body {
            if let ClassElement::PropertyDefinition(p) = el
                && !p.r#static
                && let Err(e) = self.field(cx, scope, p, Expr::Local(this), &mut out)
            {
                result = Err(e);
            }
        }
        result.map(|_| out)
    }

    /// Lower a class's constructor (once per round).
    fn lower_class(&mut self, scope: ScopeId) -> R<()> {
        let (id, c, ctor) = {
            let cf = &self.classes[&scope];
            (cf.id, cf.class, cf.ctor)
        };
        if self.lowered.contains(&id) {
            return Ok(());
        }
        self.lowered.insert(id);
        let derived = c.heritage.is_some();
        let func = self.module.functions[id.0 as usize].clone();
        let mut cx = FnCx::new(id, func);
        cx.scope = Some(scope);
        cx.class = Some(scope);
        cx.ctor = Some(derived);
        let before = self.diags.len();
        let mut prologue = Vec::new();
        let vars = ctor.and_then(|f| f.scope_id.get()).unwrap_or(scope);
        self.prologue(
            &mut cx,
            scope,
            vars,
            ctor.map(|f| &*f.params),
            c.span,
            &mut prologue,
        )?;
        if derived {
            let t = cx.func.add_local("this", Type::JsVal);
            prologue.push(Stmt::Set(t, Expr::intrinsic(Intrinsic::Hole, vec![])));
            cx.this = Some(t);
            cx.this_checked = true;
        }
        let this = cx.this.expect("set above");
        if self.an.uses_home.contains(&scope) {
            let self_local = cx.self_local.expect("classes take `self`");
            let h = cx.func.add_local("home", Type::JsVal);
            prologue.push(Stmt::Set(
                h,
                Expr::intrinsic(
                    Intrinsic::GetProp,
                    vec![Expr::Local(self_local), Expr::str("prototype")],
                ),
            ));
            cx.home = Some(h);
        }
        let mut stmts = Vec::new();
        if !derived && let Ok(inits) = self.field_inits(&mut cx, scope) {
            stmts.extend(inits);
        }
        match ctor {
            Some(f) => {
                let list = f.body.as_ref().map_or(&[][..], |b| &b.statements);
                stmts.extend(self.stmts(&mut cx, list));
                if !always_returns(list) {
                    stmts.push(Stmt::Return(Some(self.ctor_result(&cx, Expr::undefined()))));
                }
            }
            None => {
                if derived {
                    // `constructor(...args) { super(...args); }`
                    let args = cx.func.param(cx.func.params - 1);
                    let construct = self.parent_construct(&cx, |parent, nt, what| {
                        Expr::intrinsic(
                            Intrinsic::ConstructList,
                            vec![parent, Expr::Local(args), nt, what],
                        )
                    });
                    if let Ok(s) = self.super_construct(&mut cx, construct) {
                        stmts.extend(s);
                    }
                }
                stmts.push(Stmt::Return(Some(Expr::Local(this))));
            }
        }
        if self.diags.len() > before {
            return Err(Unsupported);
        }
        self.finish(cx, prologue, stmts);
        Ok(())
    }

    /// What a constructor returns for `return v` (and falling off the end: `undefined`).
    pub(crate) fn ctor_result(&self, cx: &FnCx, v: Expr) -> Expr {
        let derived = cx.ctor.expect("a constructor");
        let this = cx.this.expect("a constructor has `this`");
        Expr::intrinsic(
            Intrinsic::CtorReturn { derived },
            vec![v, Expr::Local(this)],
        )
    }

    /// The construction of the parent class for `super(…)`: `make(parent, new_target, what)`.
    fn parent_construct(&self, cx: &FnCx, make: impl FnOnce(Expr, Expr, Expr) -> Expr) -> Expr {
        let self_local = cx.self_local.expect("classes take `self`");
        let nt = cx
            .new_target
            .expect("derived constructors take `new.target`");
        let parent = Expr::intrinsic(Intrinsic::GetPrototypeOf, vec![Expr::Local(self_local)]);
        make(parent, Expr::Local(nt), Expr::str(&self.super_what(cx)))
    }

    /// How node names the parent in "… is not a constructor".
    fn super_what(&self, cx: &FnCx) -> String {
        let c = cx.class.map(|s| self.classes[&s].class);
        match c.and_then(|c| c.id.as_ref()) {
            Some(id) => format!("Super constructor null of {}", id.name),
            None => "Super constructor null of anonymous class".to_string(),
        }
    }

    /// Bind `this` to a new parent object (`construct`), then define the instance fields.
    fn super_construct(&mut self, cx: &mut FnCx, construct: Expr) -> R<Vec<Stmt>> {
        let this = cx.this.expect("a constructor has `this`");
        let r = cx.func.add_local("super", Type::JsVal);
        let mut out = vec![
            Stmt::Set(r, construct),
            Stmt::If {
                cond: Expr::unary(
                    UnOp::Not,
                    Type::Bool,
                    Expr::intrinsic(Intrinsic::IsHole, vec![Expr::Local(this)]),
                ),
                then: vec![Stmt::Throw(Expr::intrinsic(
                    Intrinsic::NewError(ErrorKind::ReferenceError),
                    vec![
                        Expr::str("Super constructor may only be called once"),
                        Expr::undefined(),
                    ],
                ))],
                els: vec![],
            },
            Stmt::Set(this, Expr::Local(r)),
        ];
        let scope = cx.class.expect("a class constructor");
        out.extend(self.field_inits(cx, scope)?);
        Ok(out)
    }

    /// `super(…)` in a derived constructor: the new `this`.
    pub(crate) fn super_call(&mut self, cx: &mut FnCx, c: &'a CallExpression<'a>) -> R<Val> {
        if cx.ctor != Some(true) {
            if self
                .an
                .unsupported
                .iter()
                .any(|(sp, _)| *sp == c.callee.span())
            {
                return Err(Unsupported); // reported by the analysis
            }
            return Err(self.unsupported(
                c.span,
                "`super()` outside a derived class constructor",
                "never",
            ));
        }
        let construct = if c.arguments.iter().any(|a| a.is_spread()) {
            let args = self.spread_args(cx, &c.arguments, c.span)?;
            self.parent_construct(cx, |parent, nt, what| {
                Expr::intrinsic(Intrinsic::ConstructList, vec![parent, args, nt, what])
            })
        } else {
            let args = self.value_args(cx, &c.arguments)?;
            let what = self.super_what(cx);
            self.parent_construct(cx, |parent, nt, _| Expr::New {
                callee: Box::new(parent),
                new_target: Some(Box::new(nt)),
                args,
                what,
            })
        };
        let body = self.super_construct(cx, construct)?;
        let this = cx.this.expect("a constructor has `this`");
        Ok(Val::js(
            Expr::Seq {
                body,
                value: Box::new(Expr::Local(this)),
            },
            Kind::Any,
        ))
    }

    /// The index of private name `name` of class `scope`.
    fn private_id(&self, scope: ScopeId, name: &str) -> u32 {
        self.an
            .privates
            .iter()
            .position(|p| p.class == scope && p.name == name)
            .expect("declared private names are collected by the analysis") as u32
    }

    /// The symbol of a private name reference, and how node reports reading it from / writing it
    /// to an object without it.
    pub(crate) fn private_sym(
        &mut self,
        cx: &FnCx,
        id: &PrivateIdentifier<'a>,
    ) -> R<(Expr, String, String)> {
        let found = self
            .an
            .private_refs
            .get(&id.span.start)
            .and_then(|i| cx.privates.get(i).map(|l| (*i, *l)));
        let Some((i, l)) = found else {
            return Err(self.unsupported(
                id.span,
                &format!("internal: private name `#{}` not in scope", id.name),
                "T16",
            ));
        };
        let p = &self.an.privates[i as usize];
        if p.method {
            // Methods and accessors: a brand check.
            let c = self.classes[&p.class].class;
            let class = c.id.as_ref().map_or("anonymous", |i| i.name.as_str());
            let msg = if p.is_static {
                format!("Receiver must be class {class}")
            } else {
                format!("Receiver must be an instance of class {class}")
            };
            return Ok((Expr::Local(l), msg.clone(), msg));
        }
        let read = format!(
            "Cannot read private member {} from an object whose class did not declare it",
            p.name
        );
        let write = format!(
            "Cannot write private member {} to an object whose class did not declare it",
            p.name
        );
        Ok((Expr::Local(l), read, write))
    }

    /// `o.#x`.
    pub(crate) fn private_get(
        &mut self,
        cx: &mut FnCx,
        pf: &'a PrivateFieldExpression<'a>,
    ) -> R<Val> {
        if pf.optional {
            return Err(self.unsupported(pf.span, "optional chain with a private field", "later"));
        }
        let obj = jsval_of(self.expr(cx, &pf.object)?);
        let (sym, msg, _) = self.private_sym(cx, &pf.field)?;
        Ok(Val::js(
            Expr::intrinsic(Intrinsic::PrivateGet, vec![obj, sym, Expr::str(&msg)]),
            Kind::Any,
        ))
    }

    /// `new f(…)` for any value `f` (the native errors are handled before).
    pub(crate) fn new_expr(&mut self, cx: &mut FnCx, n: &'a NewExpression<'a>) -> R<Val> {
        let what = self.callee_text(&n.callee);
        if ["Array", "Object", "RegExp", "Date"]
            .iter()
            .chain(super::builtins::NEW_ONLY)
            .any(|b| self.is_global(&n.callee, b))
            && !n.arguments.iter().any(|a| a.is_spread())
            && let Some(func) = self.builtin_global(&n.callee)
        {
            // `new Array(…)` is `Array(…)` (T21); `new Map(…)` calls `$$Map`, `new Object(…)` `$$Object` (T25),
            // `new RegExp(…)` `$$RegExp` (T27), `new Date(…)` `$$Date` (T26).
            let func = if self.is_global(&n.callee, "Date") {
                let sym = self.prelude.names[super::builtins::DATE_NEW];
                self.funcs[&sym]
            } else {
                func
            };
            self.check_builtin(n.callee.span(), func)?;
            let (e, kind) = self.direct_call(cx, func, None, &n.arguments)?;
            return self.call_result(e, kind);
        }
        let f = jsval_of(self.expr(cx, &n.callee)?);
        if n.arguments.iter().any(|a| a.is_spread()) {
            let t = cx.func.add_local("callee", Type::JsVal);
            let args = self.spread_args(cx, &n.arguments, n.span)?;
            return Ok(Val::js(
                Expr::Seq {
                    body: vec![Stmt::Set(t, f)],
                    value: Box::new(Expr::intrinsic(
                        Intrinsic::ConstructList,
                        vec![Expr::Local(t), args, Expr::Local(t), Expr::str(&what)],
                    )),
                },
                Kind::Any,
            ));
        }
        let args = self.value_args(cx, &n.arguments)?;
        Ok(Val::js(
            Expr::New {
                callee: Box::new(f),
                new_target: None,
                args,
                what,
            },
            Kind::Any,
        ))
    }

    /// `this`: checked in a derived constructor (and arrow functions in it).
    pub(crate) fn this_value(&self, cx: &FnCx) -> Expr {
        let Some(l) = cx.this else {
            return Expr::undefined();
        };
        if !cx.this_checked {
            return Expr::Local(l);
        }
        Expr::Seq {
            body: vec![Stmt::If {
                cond: Expr::intrinsic(Intrinsic::IsHole, vec![Expr::Local(l)]),
                then: vec![Stmt::Throw(Expr::intrinsic(
                    Intrinsic::NewError(ErrorKind::ReferenceError),
                    vec![Expr::str(UNINIT_THIS), Expr::undefined()],
                ))],
                els: vec![],
            }],
            value: Box::new(Expr::Local(l)),
        }
    }

    /// `super.x` / `super[k]`: the home object's prototype's property, `this` as the receiver.
    /// Returns (the value, `this`).
    pub(crate) fn super_member(
        &mut self,
        cx: &mut FnCx,
        m: &'a MemberExpression<'a>,
    ) -> R<(Expr, Expr)> {
        let Some(home) = cx.home else {
            return Err(self.unsupported(m.span(), "`super` outside a method", "never"));
        };
        let key = match m {
            MemberExpression::StaticMemberExpression(sm) => Expr::str(sm.property.name.as_str()),
            MemberExpression::ComputedMemberExpression(cm) => {
                jsval_of(self.expr(cx, &cm.expression)?)
            }
            MemberExpression::PrivateFieldExpression(p) => {
                return Err(self.unsupported(p.span, "private field", "later"));
            }
        };
        let t = cx.func.add_local("this", Type::JsVal);
        let this = self.this_value(cx);
        let get = Expr::Seq {
            body: vec![Stmt::Set(t, this)],
            value: Box::new(Expr::intrinsic(
                Intrinsic::SuperGet,
                vec![Expr::Local(home), key, Expr::Local(t)],
            )),
        };
        Ok((get, Expr::Local(t)))
    }

    /// A class's `$code` functions, made on first use: the call trap and the construct entry.
    fn class_adapters(&mut self, scope: ScopeId, name: &str) -> (FuncId, FuncId) {
        if let Some(a) = self.class_adapters.get(&scope) {
            return *a;
        }
        let id = self.classes[&scope].id;
        let f = self.module.function(id).clone();
        let hidden = self.hidden[&id];
        let code_sig = [
            ("self", Type::JsVal),
            ("this", Type::JsVal),
            ("args", Type::JsVal),
        ];
        let mut call = ir::Function::new(format!("{}$call", f.name), &code_sig, Some(Type::JsVal));
        call.pos = f.pos;
        call.body = vec![Stmt::Throw(Expr::intrinsic(
            Intrinsic::NewError(ErrorKind::TypeError),
            vec![
                Expr::str(&format!(
                    "Class constructor {name} cannot be invoked without 'new'"
                )),
                Expr::undefined(),
            ],
        ))];
        let call = self.module.add_function(call);
        let mut construct = ir::Function::new(
            format!("{}$construct", f.name),
            &[
                ("self", Type::JsVal),
                ("new.target", Type::JsVal),
                ("args", Type::JsVal),
            ],
            Some(Type::JsVal),
        );
        construct.pos = f.pos;
        let mut args = vec![Expr::Local(LocalId(0))];
        if hidden.this {
            args.push(Expr::intrinsic(
                Intrinsic::CreateThis,
                vec![Expr::Local(LocalId(1))],
            ));
        }
        if hidden.new_target {
            args.push(Expr::Local(LocalId(1)));
        }
        let rest = self.rest_param.contains(&id);
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
        construct.body = vec![Stmt::Return(Some(Expr::Call { func: id, args }))];
        let construct = self.module.add_function(construct);
        self.class_adapters.insert(scope, (call, construct));
        (call, construct)
    }
}

use super::Unsupported;

/// A static element run after the class is defined, in source order.
enum Static<'a> {
    Field(&'a PropertyDefinition<'a>),
    Block(&'a StaticBlock<'a>),
}
