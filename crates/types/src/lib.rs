//! Whole-program type analysis (T131): for every property access, call and operator of the user's code, the set of
//! types its operands can have. The generated code will guard every guess, so these are guesses, not proofs.
//!
//! Flow-insensitive and inclusion-based, iterated to a fixpoint over the AST (0-CFA, as Hopc's type analyses), with
//! one refinement: a function that makes closures (a factory such as Prototype.js's `Class.create`) is analysed
//! once per call site, and the closures and objects it makes are distinct per call site.
//!
//! Abstract values: the primitive types, an allocation (an object or array literal, a `new`, a function, a
//! function's default prototype, a built-in object or function) or unknown. Every variable, `this`, return and
//! object property has a set of them; a read of a property missing everywhere adds nothing.

use std::collections::{BTreeSet, HashMap};

use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{Scoping, Semantic, SymbolId};
use oxc_span::GetSpan;
use oxc_syntax::scope::{ScopeFlags, ScopeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AV {
    Num,
    Str,
    Bool,
    Undef,
    Null,
    Sym,
    BigInt,
    Obj(u32),
    Unknown,
}

pub type Set = BTreeSet<AV>;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Kind {
    /// `{…}`
    Lit,
    /// `[…]`
    Arr,
    /// `new F(…)`; `Some(name)` for a built-in constructor.
    New(Option<String>),
    /// A function or class (`func`: the span start of its node).
    Fn(u32),
    /// The default `prototype` object of the function allocation `of`.
    Proto(u32),
    /// An `arguments` object.
    Args,
    /// A built-in object (`Math`, `Object.prototype`, …).
    Builtin(String),
    /// A built-in function (`Math.sqrt`, `Array`, …).
    BFn(String),
    /// An array made by a built-in (`split`, `slice`, …).
    BArr,
    /// `Object.create(p)`.
    Create,
    /// A regex literal.
    RegExp,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Alloc {
    span: u32,
    kind: Kind,
    inst: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Inst {
    func: u32,
    parent: u32,
    ctx: Option<u32>,
}

#[derive(Clone, Copy)]
enum FnNode<'a> {
    Func(&'a Function<'a>),
    Arrow(&'a ArrowFunctionExpression<'a>),
    Class(&'a Class<'a>),
}

const ROOT: u32 = u32::MAX;
/// Objects a stored set holds before it is widened to unknown ([`add_state`]).
const MAX_OBJECTS: usize = 32;
/// The work budget: expressions evaluated over all rounds. Past it the analysis gives up (no results), so a big
/// program (a whole `node_modules` graph) doesn't stall the build; deterministic, unlike a time limit.
const MAX_STEPS: u64 = 2_000_000;
/// At most this many per-call-site copies of one function; further call sites share one.
const MAX_COPIES: u32 = 64;

/// One site: offset, category, per operand the number of groups (what the compiler tells apart) and tags.
#[derive(Debug, Clone)]
pub struct Site {
    pub offset: u32,
    pub end: u32,
    pub cat: &'static str,
    pub groups: (usize, usize),
    pub tags: (Vec<String>, Vec<String>),
}

/// The analysis result: per site (span start, kind), the types seen, as tags comparable with run-time tags.
pub struct Result {
    /// (offset, category) → (operand tags, second operand tags for binary operators)
    pub sites: Vec<Site>,
    /// `o.m(…)` sites (the member's start, end) that call exactly one function (its node's start) on receivers of
    /// one group: where a guarded direct call pays.
    pub methods: HashMap<(u32, u32), u32>,
    pub rounds: u32,
    pub instances: usize,
    pub allocs: usize,
    /// Expressions evaluated (over [`MAX_STEPS`]: the analysis gave up, no results).
    pub steps: u64,
}

struct A<'a, 's> {
    scoping: &'s Scoping,
    user_end: u32,
    program: &'a Program<'a>,
    fns: HashMap<u32, FnNode<'a>>,
    scope_fn: HashMap<ScopeId, u32>,
    sensitive: HashMap<u32, bool>,
    allocs: Vec<Alloc>,
    alloc_ids: HashMap<Alloc, u32>,
    insts: Vec<Inst>,
    inst_ids: HashMap<Inst, u32>,
    vars: HashMap<(SymbolId, u32), Set>,
    globals: HashMap<String, Set>,
    props: HashMap<(u32, String), Set>,
    anyprop: HashMap<u32, Set>,
    elem: HashMap<u32, Set>,
    protos: HashMap<u32, Set>,
    this: HashMap<u32, Set>,
    ret: HashMap<u32, Set>,
    args_pos: HashMap<u32, Vec<Set>>,
    sites: HashMap<(u32, u32, &'static str), (Set, Set)>,
    /// method-call sites (member span): the functions they call, and whether anything else can be called
    targets: HashMap<(u32, u32), (BTreeSet<u32>, bool)>,
    /// per-call-site copies made of each function (at most [`MAX_COPIES`])
    copies: HashMap<u32, u32>,
    /// functions whose `prototype` property the program replaces (no default prototype object)
    replaced: std::collections::HashSet<u32>,
    /// constructors that always return an object (found by the first run)
    returns_obj: std::collections::HashSet<u32>,
    rounds: u32,
    /// expressions evaluated so far (the work budget, [`MAX_STEPS`])
    steps: u64,
    /// variables narrowed by an enclosing `typeof x === "…"` test (innermost last)
    narrow: Vec<((SymbolId, u32), String, bool)>,
    cur: u32,
    changed: bool,
}

fn one(v: AV) -> Set {
    let mut s = Set::new();
    s.insert(v);
    s
}

fn add(dst: &mut Set, src: &Set, changed: &mut bool) {
    for v in src {
        if dst.insert(*v) {
            *changed = true;
        }
    }
}

/// [`add`] into stored state (variables, properties, elements, `this`, returns), widened: past [`MAX_OBJECTS`]
/// objects a set holds unknown instead (megamorphic: no fast path would pay, and big sets make the analysis slow).
fn add_state(dst: &mut Set, src: &Set, changed: &mut bool) {
    if dst.contains(&AV::Unknown) && src.iter().all(|v| matches!(v, AV::Obj(_)) || dst.contains(v)) {
        return;
    }
    add(dst, src, changed);
    if dst.iter().filter(|v| matches!(v, AV::Obj(_))).count() > MAX_OBJECTS {
        dst.retain(|v| !matches!(v, AV::Obj(_)));
        dst.insert(AV::Unknown);
    }
}

pub fn analyze(program: &Program, semantic: &Semantic, user_end: u32) -> Result {
    // A first run finds the functions whose `prototype` the program replaces; the real run gives only the others
    // their default prototype object (else objects made before the replacement is seen keep it for ever).
    let first = run(program, semantic, user_end, (Default::default(), Default::default()));
    if first.steps > MAX_STEPS {
        return Result { sites: Vec::new(), methods: HashMap::new(), rounds: first.rounds, instances: first.insts.len(), allocs: first.allocs.len(), steps: first.steps };
    }
    let mut replaced = std::collections::HashSet::new();
    for ((o, k), v) in &first.props {
        if k == "prototype" && !v.is_empty() && let Kind::Fn(f) = first.allocs[*o as usize].kind {
            replaced.insert(f);
        }
    }
    // constructors whose every return gives an object: `new` gives that object, never `this`
    let mut returns_obj = std::collections::HashSet::new();
    let mut returns_other = std::collections::HashSet::new();
    for (i, r) in &first.ret {
        let f = first.insts[*i as usize].func;
        for v in r {
            if matches!(v, AV::Obj(_)) { returns_obj.insert(f); } else { returns_other.insert(f); }
        }
    }
    let returns_obj = returns_obj.difference(&returns_other).copied().collect();
    let a = run(program, semantic, user_end, (replaced, returns_obj));
    if a.steps > MAX_STEPS {
        return Result { sites: Vec::new(), methods: HashMap::new(), rounds: a.rounds, instances: a.insts.len(), allocs: a.allocs.len(), steps: a.steps };
    }
    let mut sites: Vec<_> = a
        .sites
        .iter()
        .map(|(&(offset, end, cat), (l, r))| Site {
            offset,
            end,
            cat,
            groups: (a.groups(l), a.groups(r)),
            tags: (a.tags(l), a.tags(r)),
        })
        .collect();
    sites.sort_by_key(|s| (s.offset, s.end, s.cat));
    let mut methods = HashMap::new();
    for (&(start, end), (fns, other)) in &a.targets {
        let groups = a.sites.get(&(start, end, "method")).map_or(99, |(l, _)| a.groups(l));
        if !*other && fns.len() == 1 && groups == 1 {
            methods.insert((start, end), *fns.iter().next().unwrap());
        }
    }
    Result { sites, methods, rounds: a.rounds, instances: a.insts.len(), allocs: a.allocs.len(), steps: a.steps }
}

fn run<'a, 's>(
    program: &'a Program<'a>,
    semantic: &'s Semantic<'a>,
    user_end: u32,
    (replaced, returns_obj): (std::collections::HashSet<u32>, std::collections::HashSet<u32>),
) -> A<'a, 's> {
    let mut a = A {
        replaced,
        returns_obj,
        rounds: 0,
        steps: 0,
        narrow: Vec::new(),
        scoping: semantic.scoping(),
        user_end,
        program,
        fns: HashMap::new(),
        scope_fn: HashMap::new(),
        sensitive: HashMap::new(),
        allocs: Vec::new(),
        alloc_ids: HashMap::new(),
        insts: Vec::new(),
        inst_ids: HashMap::new(),
        vars: HashMap::new(),
        globals: HashMap::new(),
        props: HashMap::new(),
        anyprop: HashMap::new(),
        elem: HashMap::new(),
        protos: HashMap::new(),
        this: HashMap::new(),
        ret: HashMap::new(),
        args_pos: HashMap::new(),
        sites: HashMap::new(),
        copies: HashMap::new(),
        targets: HashMap::new(),
        cur: 0,
        changed: false,
    };
    for node in semantic.nodes().iter() {
        match node.kind() {
            AstKind::Function(f) => {
                a.fns.insert(f.span.start, FnNode::Func(f));
                a.scope_fn.insert(f.scope_id(), f.span.start);
            }
            AstKind::ArrowFunctionExpression(f) => {
                a.fns.insert(f.span.start, FnNode::Arrow(f));
                a.scope_fn.insert(f.scope_id(), f.span.start);
            }
            AstKind::Class(c) => {
                a.fns.insert(c.span.start, FnNode::Class(c));
                for e in &c.body.body {
                    if let ClassElement::MethodDefinition(m) = e
                        && m.kind == MethodDefinitionKind::Constructor
                    {
                        a.scope_fn.insert(m.value.scope_id(), c.span.start);
                    }
                }
            }
            _ => {}
        }
    }
    // A function that makes closures is analysed per call site.
    for node in semantic.nodes().iter() {
        let s = match node.kind() {
            AstKind::Function(f) => f.scope_id(),
            AstKind::ArrowFunctionExpression(f) => f.scope_id(),
            _ => continue,
        };
        let inner = a.scoping.scope_parent_id(s).and_then(|p| a.enclosing_fn(p));
        if let Some(outer) = inner {
            a.sensitive.insert(outer, true);
        }
    }
    a.insts.push(Inst { func: ROOT, parent: ROOT, ctx: None });
    a.inst_ids.insert(a.insts[0], 0);
    let mut rounds = 0;
    loop {
        rounds += 1;
        a.changed = false;
        let mut i = 0;
        while i < a.insts.len() {
            a.cur = i as u32;
            a.run_inst(i as u32);
            i += 1;
        }
        if !a.changed || rounds > 200 || a.steps > MAX_STEPS {
            break;
        }
    }
    a.rounds = rounds;
    a
}

impl<'a, 's> A<'a, 's> {
    fn enclosing_fn(&self, mut s: ScopeId) -> Option<u32> {
        loop {
            if let Some(f) = self.scope_fn.get(&s) {
                return Some(*f);
            }
            s = self.scoping.scope_parent_id(s)?;
        }
    }

    fn alloc(&mut self, span: u32, kind: Kind, inst: u32) -> u32 {
        // only closures (and what hangs off them) are told apart per function copy
        let inst = if matches!(kind, Kind::Fn(_) | Kind::Proto(_) | Kind::Args) { inst } else { 0 };
        let a = Alloc { span, kind, inst };
        if let Some(id) = self.alloc_ids.get(&a) {
            return *id;
        }
        let id = self.allocs.len() as u32;
        self.allocs.push(a.clone());
        self.alloc_ids.insert(a, id);
        self.changed = true;
        id
    }

    fn builtin(&mut self, name: &str) -> AV {
        AV::Obj(self.alloc(0, Kind::Builtin(name.to_string()), 0))
    }
    fn bfn(&mut self, name: &str) -> AV {
        AV::Obj(self.alloc(0, Kind::BFn(name.to_string()), 0))
    }

    fn inst(&mut self, func: u32, parent: u32, ctx: Option<u32>) -> u32 {
        let mut i = Inst { func, parent, ctx };
        if ctx.is_some() && !self.inst_ids.contains_key(&i) {
            let n = self.copies.entry(func).or_default();
            if *n >= MAX_COPIES {
                i.ctx = None;
            } else {
                *n += 1;
            }
        }
        if let Some(id) = self.inst_ids.get(&i) {
            return *id;
        }
        let id = self.insts.len() as u32;
        self.insts.push(i);
        self.inst_ids.insert(i, id);
        self.changed = true;
        id
    }

    /// Run-time-comparable tags (what `typeprobe` can see): objects by literal/function site or by their
    /// prototype chain (`inst@<prototype's tag>`), arrays as `array`.
    fn tags(&self, s: &Set) -> Vec<String> {
        let mut t: BTreeSet<String> = BTreeSet::new();
        for v in s {
            match v {
                AV::Obj(o) => self.obj_tags(*o, 0, &mut t),
                _ => {
                    t.insert(prim_tag(*v).into());
                }
            }
        }
        t.into_iter().collect()
    }

    fn obj_tags(&self, o: u32, depth: u32, out: &mut BTreeSet<String>) {
        let a = &self.allocs[o as usize];
        match &a.kind {
            Kind::Arr | Kind::BArr => {
                out.insert("array".into());
            }
            Kind::New(Some(n)) if n == "Array" => {
                out.insert("array".into());
            }
            Kind::BFn(_) => {
                out.insert("other-fn".into());
            }
            Kind::New(None) | Kind::Create => {
                let mut ps = BTreeSet::new();
                for p in self.protos.get(&o).into_iter().flatten() {
                    if let AV::Obj(p) = p {
                        self.proto_tags(*p, depth + 1, &mut ps);
                    }
                }
                if ps.is_empty() {
                    out.insert("other-obj".into());
                }
                for p in ps {
                    out.insert(format!("inst@{p}"));
                }
            }
            Kind::Lit | Kind::Fn(_) | Kind::Proto(_) => {
                self.proto_tags(o, depth, out);
            }
            Kind::Args | Kind::Builtin(_) | Kind::RegExp | Kind::New(Some(_)) => {
                out.insert("other-obj".into());
            }
        }
    }

    /// The tags of an object used as a prototype (none for built-in prototypes).
    fn proto_tags(&self, o: u32, depth: u32, out: &mut BTreeSet<String>) {
        let a = &self.allocs[o as usize];
        match &a.kind {
            Kind::Lit | Kind::Fn(_) => {
                out.insert(format!("site@{}", a.span));
            }
            Kind::Proto(of) => {
                out.insert(format!("proto@{}", self.allocs[*of as usize].span));
            }
            Kind::New(None) | Kind::Create if depth < 4 => {
                let mut ps = BTreeSet::new();
                for p in self.protos.get(&o).into_iter().flatten() {
                    if let AV::Obj(p) = p {
                        self.proto_tags(*p, depth + 1, &mut ps);
                    }
                }
                for p in ps {
                    out.insert(format!("inst@{p}"));
                }
            }
            _ => {}
        }
    }

    /// What the compiler would tell apart (one struct type each): objects by where they are written (a literal, a
    /// function, a function's default prototype) or by their prototypes (objects from `new`/`Object.create`), arrays
    /// as one group; copies of a function made per call site are not told apart (the code exists once).
    fn groups(&self, s: &Set) -> usize {
        if s.contains(&AV::Unknown) {
            return 99;
        }
        self.tags(s).len()
    }

    fn record(&mut self, span: oxc_span::Span, cat: &'static str, l: &Set, r: Option<&Set>) {
        if span.start >= self.user_end {
            return;
        }
        let e = self.sites.entry((span.start, span.end, cat)).or_default();
        let mut c = false;
        // a receiver that is null/undefined throws: not a type a fast path needs
        let receiver = matches!(cat, "get" | "set" | "method" | "eget" | "eset");
        let l: Set = l.iter().copied().filter(|v| !receiver || !matches!(v, AV::Null | AV::Undef)).collect();
        add(&mut e.0, &l, &mut c);
        if let Some(r) = r {
            add(&mut e.1, r, &mut c);
        }
    }

    // ---- state helpers ----
    fn union_var(&mut self, k: (SymbolId, u32), v: &Set) {
        let e = self.vars.entry(k).or_default();
        add_state(e, v, &mut self.changed);
    }
    fn union_prop(&mut self, o: u32, k: &str, v: &Set) {
        let e = self.props.entry((o, k.to_string())).or_default();
        add_state(e, v, &mut self.changed);
    }
    fn union_map(map: &mut HashMap<u32, Set>, k: u32, v: &Set, changed: &mut bool) {
        let e = map.entry(k).or_default();
        add_state(e, v, changed);
    }

    fn sym_key(&self, sym: SymbolId) -> (SymbolId, u32) {
        let decl = self.enclosing_fn(self.scoping.symbol_scope_id(sym)).unwrap_or(ROOT);
        let mut i = self.cur;
        loop {
            let inst = self.insts[i as usize];
            if inst.func == decl || inst.parent == ROOT && inst.func == ROOT {
                return (sym, i);
            }
            if inst.parent == ROOT {
                return (sym, 0);
            }
            i = inst.parent;
        }
    }

    /// The instance whose `this` an arrow function sees.
    fn this_inst(&self) -> u32 {
        let mut i = self.cur;
        loop {
            let inst = self.insts[i as usize];
            match self.fns.get(&inst.func) {
                Some(FnNode::Arrow(_)) if inst.parent != ROOT => i = inst.parent,
                _ => return i,
            }
        }
    }

    // ---- instances ----
    fn run_inst(&mut self, i: u32) {
        let inst = self.insts[i as usize];
        if inst.func == ROOT {
            let program = self.program;
            let n = program.body.iter().take_while(|s| s.span().start < self.user_end).count();
            // the prelude's functions (after the user's code): bound, analysed only when called (a CommonJS entry
            // is run by one)
            for st in &program.body[n..] {
                if matches!(st, Statement::FunctionDeclaration(_)) {
                    self.stmt(st);
                }
            }
            self.stmts(&program.body[..n]);
            return;
        }
        match self.fns.get(&inst.func).copied() {
            Some(FnNode::Func(f)) => {
                if let Some(b) = &f.body {
                    self.stmts(&b.statements);
                }
            }
            Some(FnNode::Arrow(f)) => {
                match &f.body {
                    ArrowFunctionBody::FunctionBody(b) => self.stmts(&b.statements),
                    body => {
                        if let Some(e) = body.as_expression() {
                            let v = self.eval(e);
                            Self::union_map(&mut self.ret, i, &v, &mut self.changed);
                        }
                    }
                }
            }
            Some(FnNode::Class(c)) => {
                let this = self.this.get(&i).cloned().unwrap_or_default();
                for e in &c.body.body {
                    match e {
                        ClassElement::PropertyDefinition(p) if !p.r#static => {
                            let v = match &p.value {
                                Some(v) => self.eval(v),
                                None => one(AV::Undef),
                            };
                            if let Some(k) = p.key.static_name() {
                                self.write(&this, &k, &v);
                            }
                        }
                        ClassElement::MethodDefinition(m) if m.kind == MethodDefinitionKind::Constructor => {
                            if let Some(b) = &m.value.body {
                                for s in &b.statements {
                                    self.stmt(s);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            None => {}
        }
    }

    fn params(&self, func: u32) -> Option<&'a FormalParameters<'a>> {
        match self.fns.get(&func)? {
            FnNode::Func(f) => Some(&f.params),
            FnNode::Arrow(f) => Some(&f.params),
            FnNode::Class(c) => c.body.body.iter().find_map(|e| match e {
                ClassElement::MethodDefinition(m) if m.kind == MethodDefinitionKind::Constructor => {
                    Some(&*m.value.params)
                }
                _ => None,
            }),
        }
    }

    /// Calls the function values in `callee` with `this` and positional arguments; the union of their results.
    fn call(&mut self, site: u32, callee: &Set, this: &Set, args: &[Set], spread: Option<&Set>) -> Set {
        let mut out = Set::new();
        for v in callee.clone() {
            let AV::Obj(o) = v else {
                if v == AV::Unknown {
                    out.insert(AV::Unknown);
                }
                continue;
            };
            let al = self.allocs[o as usize].clone();
            match al.kind {
                Kind::Fn(func) => {
                    let ctx = if self.sensitive.get(&func).copied().unwrap_or(false) { Some(site) } else { None };
                    let i = self.inst(func, al.inst, ctx);
                    self.bind_call(i, func, this, args, spread);
                    let r = self.ret.get(&i).cloned().unwrap_or_default();
                    if matches!(self.fns.get(&func), Some(FnNode::Func(f)) if f.r#async)
                        || matches!(self.fns.get(&func), Some(FnNode::Arrow(f)) if f.r#async)
                    {
                        // an async function returns a promise of its result
                        let p = self.alloc(func, Kind::New(Some("Promise".into())), 0);
                        let r = self.awaited(&r);
                        Self::union_map(&mut self.elem, p, &r, &mut self.changed);
                        out.insert(AV::Obj(p));
                    } else {
                        add(&mut out, &r, &mut false);
                    }
                }
                Kind::BFn(name) => {
                    let r = self.call_builtin(site, &name, this, args);
                    add(&mut out, &r, &mut false);
                }
                _ => {}
            }
        }
        out
    }

    fn bind_call(&mut self, i: u32, func: u32, this: &Set, args: &[Set], spread: Option<&Set>) {
        self.steps += 1 + args.len() as u64;
        let saved = self.cur;
        self.cur = i;
        Self::union_map(&mut self.this, i, this, &mut self.changed);
        if let Some(ps) = self.params(func) {
            for (n, p) in ps.items.iter().enumerate() {
                let mut v = args.get(n).cloned().unwrap_or_else(|| if spread.is_some() { Set::new() } else { one(AV::Undef) });
                if let Some(s) = spread {
                    add(&mut v, s, &mut false);
                }
                self.bind(&p.pattern, &v);
            }
            if let Some(r) = &ps.rest {
                let a = self.alloc(r.span.start, Kind::BArr, i);
                let mut all = Set::new();
                for x in args.iter().skip(ps.items.len()) {
                    add(&mut all, x, &mut false);
                }
                if let Some(s) = spread {
                    add(&mut all, s, &mut false);
                }
                Self::union_map(&mut self.elem, a, &all, &mut self.changed);
                self.bind(&r.rest.argument, &one(AV::Obj(a)));
            }
        }
        // `arguments`: positional values
        let pos = self.args_pos.entry(i).or_default();
        while pos.len() < args.len() {
            pos.push(Set::new());
        }
        for (n, x) in args.iter().enumerate() {
            add_state(&mut pos[n], x, &mut self.changed);
        }
        if let Some(s) = spread {
            if pos.is_empty() {
                pos.push(Set::new());
            }
            for p in pos.iter_mut() {
                add_state(p, s, &mut self.changed);
            }
        }
        self.cur = saved;
    }

    fn bind(&mut self, p: &BindingPattern<'a>, v: &Set) {
        match p {
            BindingPattern::BindingIdentifier(id) => {
                let k = self.sym_key(id.symbol_id());
                self.union_var(k, v);
            }
            BindingPattern::AssignmentPattern(ap) => {
                let mut v = v.clone();
                let d = self.eval(&ap.right);
                add(&mut v, &d, &mut false);
                self.bind(&ap.left, &v);
            }
            BindingPattern::ObjectPattern(op) => {
                for prop in &op.properties {
                    let pv = match prop.key.static_name() {
                        Some(k) if !prop.computed => self.read(v, &k),
                        _ => one(AV::Unknown),
                    };
                    self.bind(&prop.value, &pv);
                }
                if let Some(r) = &op.rest {
                    self.bind(&r.argument, &one(AV::Unknown));
                }
            }
            BindingPattern::ArrayPattern(ap) => {
                let e = self.elems(v);
                for x in ap.elements.iter().flatten() {
                    self.bind(x, &e);
                }
                if let Some(r) = &ap.rest {
                    self.bind(&r.argument, &one(AV::Unknown));
                }
            }
        }
    }

    // ---- objects ----
    fn kind(&self, o: u32) -> &Kind {
        &self.allocs[o as usize].kind
    }

    fn is_arrayish(&self, o: u32) -> bool {
        matches!(self.kind(o), Kind::Arr | Kind::BArr | Kind::Args) || matches!(self.kind(o), Kind::New(Some(n)) if n == "Array")
    }
    fn is_typed(&self, o: u32) -> bool {
        matches!(self.kind(o), Kind::New(Some(n)) if n.ends_with("Array") && n != "Array")
    }

    /// The prototypes an object's lookups continue in.
    fn proto_of(&mut self, o: u32) -> Set {
        let mut s = self.protos.get(&o).cloned().unwrap_or_default();
        let extra = match self.kind(o).clone() {
            Kind::Lit | Kind::Proto(_) | Kind::Args => Some("Object.prototype"),
            Kind::Arr | Kind::BArr => Some("Array.prototype"),
            Kind::Fn(_) | Kind::BFn(_) => Some("Function.prototype"),
            Kind::New(Some(n)) if n == "Array" => Some("Array.prototype"),
            Kind::New(Some(_)) | Kind::RegExp => None,
            Kind::New(None) | Kind::Create => None,
            Kind::Builtin(n) if n == "Object.prototype" => None,
            Kind::Builtin(n) if n.ends_with(".prototype") => Some("Object.prototype"),
            Kind::Builtin(_) => Some("Object.prototype"),
        };
        if let Some(p) = extra {
            let b = self.builtin(p);
            s.insert(b);
        }
        s
    }

    fn lookup(&mut self, o: u32, k: &str, depth: u32, out: &mut Set) {
        let mut found = false;
        if let Some(v) = self.props.get(&(o, k.to_string()))
            && !v.is_empty() {
                found = true;
                let v = v.clone();
                add(out, &v, &mut false);
            }
        if let Some(v) = self.anyprop.get(&o) {
            let v = v.clone();
            add(out, &v, &mut false);
        }
        if found || depth > 8 {
            return;
        }
        // built-in own properties
        let kind = self.kind(o).clone();
        let b = match &kind {
            _ if k == "length" && (self.is_arrayish(o) || self.is_typed(o)) => Some(one(AV::Num)),
            Kind::Fn(f) if k == "prototype" && self.replaced.contains(f) => Some(Set::new()),
            Kind::Fn(_) if k == "prototype" => {
                let p = self.alloc(self.allocs[o as usize].span, Kind::Proto(o), self.allocs[o as usize].inst);
                Some(one(AV::Obj(p)))
            }
            Kind::Fn(_) | Kind::BFn(_) if k == "length" => Some(one(AV::Num)),
            Kind::Fn(_) | Kind::BFn(_) if k == "name" => Some(one(AV::Str)),
            Kind::BFn(n) if k == "prototype" => Some(one(self.builtin(&format!("{n}.prototype")))),
            Kind::Builtin(n) if n == "Math" && k.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') => {
                Some(one(AV::Num))
            }
            Kind::BFn(n) if n == "Number" && k.chars().all(|c| c.is_ascii_uppercase() || c == '_') => Some(one(AV::Num)),
            Kind::Builtin(n) if n.ends_with(".prototype") => {
                let known = match n.as_str() {
                    "Object.prototype" => OBJECT_PROTO.contains(&k),
                    "Function.prototype" => FUNCTION_PROTO.contains(&k),
                    "Array.prototype" => ARRAY_PROTO.contains(&k),
                    _ => true,
                };
                if known { Some(one(self.bfn(&format!("{n}.{k}")))) } else { None }
            }
            // statics of global constructors (`Array.isArray`) and members of built-in objects; a built-in
            // function's own members are Function.prototype's (below)
            Kind::BFn(n) if !n.contains('.') => Some(one(self.bfn(&format!("{n}.{k}")))),
            Kind::Builtin(n) if !n.ends_with(".prototype") || n.matches('.').count() == 1 => {
                Some(one(self.bfn(&format!("{n}.{k}"))))
            }
            Kind::RegExp if k == "lastIndex" => Some(one(AV::Num)),
            Kind::RegExp if k == "source" => Some(one(AV::Str)),
            Kind::RegExp => Some(one(self.bfn(&format!("RegExp.prototype.{k}")))),
            Kind::New(Some(n)) if self.is_typed(o) => Some(one(self.bfn(&format!("{n}.prototype.{k}")))),
            Kind::New(Some(n)) if n != "Array" && n != "Object" => Some(one(self.bfn(&format!("{n}.prototype.{k}")))),
            _ => None,
        };
        if let Some(b) = b {
            add(out, &b, &mut false);
            return;
        }
        if self.is_typed(o) {
            return;
        }
        for p in self.proto_of(o) {
            if let AV::Obj(p) = p {
                self.lookup(p, k, depth + 1, out);
            } else if p == AV::Unknown {
                out.insert(AV::Unknown);
            }
        }
    }

    fn read(&mut self, recv: &Set, k: &str) -> Set {
        let mut out = Set::new();
        for v in recv.clone() {
            match v {
                AV::Obj(o) => self.lookup(o, k, 0, &mut out),
                AV::Str if k == "length" => {
                    out.insert(AV::Num);
                }
                AV::Str => {
                    out.insert(self.bfn(&format!("String.prototype.{k}")));
                }
                AV::Num => {
                    out.insert(self.bfn(&format!("Number.prototype.{k}")));
                }
                AV::Unknown => {
                    out.insert(AV::Unknown);
                }
                _ => {}
            }
        }
        out
    }

    fn write(&mut self, recv: &Set, k: &str, val: &Set) {
        for v in recv.clone() {
            if let AV::Obj(o) = v {
                self.union_prop(o, k, val);
            }
        }
    }

    /// The element values of arrays (or the values of any key, for other objects).
    fn elems(&mut self, recv: &Set) -> Set {
        let mut out = Set::new();
        for v in recv.clone() {
            match v {
                AV::Obj(o) if self.is_typed(o) => {
                    out.insert(AV::Num);
                }
                AV::Obj(o) if matches!(self.kind(o), Kind::Args) => {
                    let inst = self.allocs[o as usize].inst;
                    if let Some(p) = self.args_pos.get(&inst) {
                        for x in p.clone() {
                            add(&mut out, &x, &mut false);
                        }
                    }
                }
                AV::Obj(o) => {
                    if let Some(e) = self.elem.get(&o) {
                        let e = e.clone();
                        add(&mut out, &e, &mut false);
                    }
                    if let Some(e) = self.anyprop.get(&o) {
                        let e = e.clone();
                        add(&mut out, &e, &mut false);
                    }
                }
                AV::Str => {
                    out.insert(AV::Str);
                }
                AV::Unknown => {
                    out.insert(AV::Unknown);
                }
                _ => {}
            }
        }
        out
    }

    /// `typeof x === "t"` (either order, `==`/`!=`/`!==` too): (x's variable, t, true for equality).
    fn typeof_test(&self, e: &Expression<'a>) -> Option<((SymbolId, u32), String, bool)> {
        let Expression::BinaryExpression(b) = e.without_parentheses() else { return None };
        let eq = match b.operator {
            BinaryOperator::Equality | BinaryOperator::StrictEquality => true,
            BinaryOperator::Inequality | BinaryOperator::StrictInequality => false,
            _ => return None,
        };
        let (t, s) = match (&b.left, &b.right) {
            (Expression::UnaryExpression(u), Expression::StringLiteral(s))
            | (Expression::StringLiteral(s), Expression::UnaryExpression(u))
                if u.operator == UnaryOperator::Typeof =>
            {
                (&u.argument, s)
            }
            _ => return None,
        };
        let Expression::Identifier(id) = t else { return None };
        let sym = self.scoping.get_reference(id.reference_id()).symbol_id()?;
        Some((self.sym_key(sym), s.value.to_string(), eq))
    }

    /// Narrows the tested variable inside a branch (`then`: the test holds); the number of entries pushed.
    fn push_narrow(&mut self, t: &Option<((SymbolId, u32), String, bool)>, then: bool) -> usize {
        let Some((k, tag, eq)) = t else { return 0 };
        self.narrow.push((*k, tag.clone(), *eq == then));
        1
    }

    fn typeof_tag(&self, v: AV) -> Option<&'static str> {
        Some(match v {
            AV::Num => "number",
            AV::Str => "string",
            AV::Bool => "boolean",
            AV::Undef => "undefined",
            AV::Null => "object",
            AV::Sym => "symbol",
            AV::BigInt => "bigint",
            AV::Obj(o) => match self.kind(o) {
                Kind::Fn(_) | Kind::BFn(_) => "function",
                _ => "object",
            },
            AV::Unknown => return None,
        })
    }

    /// What `await v` gives: a promise's value, anything else as is.
    fn awaited(&mut self, v: &Set) -> Set {
        let mut out = Set::new();
        for x in v.clone() {
            match x {
                AV::Obj(o) if matches!(self.kind(o), Kind::New(Some(n)) if n == "Promise") => {
                    let e = self.elem.get(&o).cloned().unwrap_or_default();
                    add(&mut out, &e, &mut false);
                }
                _ => {
                    out.insert(x);
                }
            }
        }
        out
    }

    fn read_computed(&mut self, recv: &Set, key: &Set, key_name: Option<&str>) -> Set {
        if let Some(k) = key_name {
            return self.read(recv, k);
        }
        let mut out = self.elems(recv);
        if key.iter().any(|k| *k != AV::Num) {
            // a string key: any named property of these objects
            for v in recv.clone() {
                if let AV::Obj(o) = v {
                    let named: Vec<Set> =
                        self.props.iter().filter(|((p, _), _)| *p == o).map(|(_, s)| s.clone()).collect();
                    for s in named {
                        add(&mut out, &s, &mut false);
                    }
                }
            }
        }
        out
    }

    fn write_computed(&mut self, recv: &Set, key: &Set, key_name: Option<&str>, val: &Set) {
        if let Some(k) = key_name {
            return self.write(recv, k, val);
        }
        let numeric = key.iter().all(|k| *k == AV::Num);
        for v in recv.clone() {
            if let AV::Obj(o) = v {
                if numeric && (self.is_arrayish(o) || !matches!(self.kind(o), Kind::Lit | Kind::New(None))) {
                    Self::union_map(&mut self.elem, o, val, &mut self.changed);
                } else {
                    Self::union_map(&mut self.anyprop, o, val, &mut self.changed);
                }
            }
        }
    }

    // ---- built-ins ----
    fn global(&mut self, name: &str) -> Set {
        if let Some(s) = self.globals.get(name) {
            return s.clone();
        }
        match name {
            "undefined" => one(AV::Undef),
            "NaN" | "Infinity" => one(AV::Num),
            "Math" | "JSON" | "console" | "process" | "Reflect" | "globalThis" | "performance" => {
                one(self.builtin(name))
            }
            "arguments" => {
                let i = self.this_inst();
                let a = self.alloc(0, Kind::Args, i);
                one(AV::Obj(a))
            }
            _ if GLOBAL_FNS.contains(&name) => one(self.bfn(name)),
            // a global nothing assigns: reading it throws (a ReferenceError), so it gives no value
            _ => Set::new(),
        }
    }

    fn call_builtin(&mut self, site: u32, name: &str, this: &Set, args: &[Set]) -> Set {
        // callbacks: functions passed to built-ins are called with what the built-in would pass
        let elem = self.elems(this);
        let method = name.rsplit('.').next().unwrap_or(name);
        let cb_args = vec![elem.clone(), one(AV::Num), this.clone()];
        let mut cb_ret = Set::new();
        for a in args {
            if a.iter().any(|v| matches!(v, AV::Obj(o) if matches!(self.kind(*o), Kind::Fn(_)))) {
                let undef = one(AV::Undef);
                let r = match method {
                    "sort" => self.call(site, a, &undef, &[elem.clone(), elem.clone()], None),
                    "reduce" | "reduceRight" => {
                        let acc = args.get(1).cloned().unwrap_or_default();
                        let mut acc2 = acc.clone();
                        add(&mut acc2, &elem, &mut false);
                        self.call(site, a, &undef, &[acc2, elem.clone(), one(AV::Num), this.clone()], None)
                    }
                    "replace" => self.call(site, a, &undef, &[one(AV::Str), one(AV::Str), one(AV::Num)], None),
                    _ if name.starts_with("Array.prototype.") => self.call(site, a, &undef, &cb_args, None),
                    _ if name.starts_with("Promise.prototype.") => self.call(site, a, &undef, std::slice::from_ref(&elem), None),
                    _ => self.call(site, a, &one(AV::Unknown), &[one(AV::Unknown), one(AV::Unknown)], None),
                };
                add(&mut cb_ret, &r, &mut false);
            }
        }
        let num = one(AV::Num);
        let s = one(AV::Str);
        let b = one(AV::Bool);
        if name.starts_with("Math.") || name.starts_with("Number.prototype.") && !matches!(method, "toString" | "toFixed" | "toPrecision") {
            return num;
        }
        match name {
            "Number" | "parseInt" | "parseFloat" | "Date.now" | "performance.now" | "Number.parseInt"
            | "Number.parseFloat" => return num,
            "String" | "String.fromCharCode" | "JSON.stringify" => return s,
            "isNaN" | "isFinite" | "Array.isArray" | "Boolean" | "Number.isInteger" => return b,
            "Object.create" => {
                let o = self.alloc(site, Kind::Create, self.cur);
                let p = args.first().cloned().unwrap_or_default();
                Self::union_map(&mut self.protos, o, &p, &mut self.changed);
                return one(AV::Obj(o));
            }
            "Object.keys" | "Object.getOwnPropertyNames" => {
                let o = self.alloc(site, Kind::BArr, self.cur);
                Self::union_map(&mut self.elem, o, &s, &mut self.changed);
                return one(AV::Obj(o));
            }
            "Array" => {
                let o = self.alloc(site, Kind::New(Some("Array".into())), self.cur);
                return one(AV::Obj(o));
            }
            _ => {}
        }
        if let Some(m) = name.strip_prefix("String.prototype.") {
            return match m {
                "charCodeAt" | "indexOf" | "lastIndexOf" | "search" | "localeCompare" | "codePointAt" => num,
                "split" | "match" => {
                    let o = self.alloc(site, Kind::BArr, self.cur);
                    Self::union_map(&mut self.elem, o, &s, &mut self.changed);
                    one(AV::Obj(o))
                }
                "startsWith" | "endsWith" | "includes" => b,
                _ => s,
            };
        }
        if let Some(m) = name.strip_prefix("Array.prototype.") {
            return match m {
                "push" | "unshift" => {
                    for v in this.clone() {
                        if let AV::Obj(o) = v {
                            for a in args {
                                Self::union_map(&mut self.elem, o, a, &mut self.changed);
                            }
                        }
                    }
                    num
                }
                "indexOf" | "lastIndexOf" | "findIndex" => num,
                "pop" | "shift" | "find" => elem,
                "join" | "toString" => s,
                "some" | "every" | "includes" => b,
                "forEach" => one(AV::Undef),
                "reduce" | "reduceRight" => cb_ret,
                "slice" | "concat" | "filter" | "splice" | "reverse" | "sort" | "map" => {
                    if m == "reverse" || m == "sort" {
                        return this.clone();
                    }
                    let o = self.alloc(site, Kind::BArr, self.cur);
                    let mut e = if m == "map" { cb_ret } else { elem };
                    if m == "concat" {
                        for a in args {
                            let ae = self.elems(a);
                            add(&mut e, &ae, &mut false);
                            add(&mut e, &a.iter().filter(|v| !matches!(v, AV::Obj(_))).copied().collect(), &mut false);
                        }
                    }
                    Self::union_map(&mut self.elem, o, &e, &mut self.changed);
                    one(AV::Obj(o))
                }
                _ => one(AV::Unknown),
            };
        }
        if let Some(m) = name.strip_prefix("Map.prototype.").or_else(|| name.strip_prefix("Set.prototype.")) {
            return match m {
                "set" | "add" => {
                    let v = if name.starts_with("Map") { args.get(1) } else { args.first() };
                    for x in this.clone() {
                        if let AV::Obj(o) = x
                            && let Some(v) = v {
                                Self::union_map(&mut self.elem, o, v, &mut self.changed);
                            }
                    }
                    this.clone()
                }
                "get" => elem,
                "has" | "delete" => b,
                "size" => num,
                _ => one(AV::Unknown),
            };
        }
        if name == "fs.readFileSync" {
            if args.len() >= 2 {
                return s;
            }
            return one(AV::Obj(self.alloc(site, Kind::New(Some("Uint8Array".into())), 0)));
        }
        if matches!(name, "Buffer.alloc" | "Buffer.allocUnsafe" | "Buffer.from" | "Buffer.concat")
            || name.starts_with("Uint8Array.prototype.") && matches!(method, "subarray" | "slice")
        {
            return one(AV::Obj(self.alloc(site, Kind::New(Some("Uint8Array".into())), 0)));
        }
        if name.starts_with("RegExp.prototype.test") || name == "Object.prototype.hasOwnProperty" {
            return b;
        }
        if name.starts_with("console.") {
            return one(AV::Undef);
        }
        one(AV::Unknown)
    }

    fn construct(&mut self, site: u32, callee: &Set, args: &[Set], spread: Option<&Set>) -> Set {
        let mut out = Set::new();
        for v in callee.clone() {
            let AV::Obj(o) = v else {
                out.insert(AV::Unknown);
                continue;
            };
            match self.kind(o).clone() {
                Kind::Fn(func) => {
                    // one object per constructor (its struct type), not per `new` site
                    let n = self.alloc(o, Kind::New(None), ROOT);
                    let proto = self.read(&one(AV::Obj(o)), "prototype");
                    Self::union_map(&mut self.protos, n, &proto, &mut self.changed);
                    let this = one(AV::Obj(n));
                    let ctx = if self.sensitive.get(&func).copied().unwrap_or(false) { Some(site) } else { None };
                    let i = self.inst(func, self.allocs[o as usize].inst, ctx);
                    self.bind_call(i, func, &this, args, spread);
                    // a constructor that returns an object gives that object instead of `this`
                    let r = self.ret.get(&i).cloned().unwrap_or_default();
                    let objs: Set = r.iter().copied().filter(|x| matches!(x, AV::Obj(_))).collect();
                    if !self.returns_obj.contains(&func) && (objs.is_empty() || objs.len() < r.len()) {
                        out.insert(AV::Obj(n));
                    }
                    add(&mut out, &objs, &mut false);
                }
                Kind::BFn(name) => {
                    let n = self.alloc(site, Kind::New(Some(name.clone())), self.cur);
                    if name == "Array" && args.len() > 1 {
                        for a in args {
                            Self::union_map(&mut self.elem, n, a, &mut self.changed);
                        }
                    }
                    out.insert(AV::Obj(n));
                }
                _ => {
                    out.insert(AV::Unknown);
                }
            }
        }
        out
    }

    // ---- statements ----
    fn stmt(&mut self, s: &Statement<'a>) {
        match s {
            Statement::ExpressionStatement(e) => {
                self.eval(&e.expression);
            }
            Statement::VariableDeclaration(d) => self.var_decl(d),
            Statement::FunctionDeclaration(f) => {
                let a = self.alloc(f.span.start, Kind::Fn(f.span.start), self.cur);
                if let Some(id) = &f.id {
                    let k = self.sym_key(id.symbol_id());
                    self.union_var(k, &one(AV::Obj(a)));
                }
            }
            Statement::ClassDeclaration(c) => {
                let v = self.class(c);
                if let Some(id) = &c.id {
                    let k = self.sym_key(id.symbol_id());
                    self.union_var(k, &v);
                }
            }
            Statement::ReturnStatement(r) => {
                let v = match &r.argument {
                    Some(e) => self.eval(e),
                    None => one(AV::Undef),
                };
                let cur = self.cur;
                Self::union_map(&mut self.ret, cur, &v, &mut self.changed);
            }
            Statement::BlockStatement(b) => self.stmts(&b.body),
            Statement::IfStatement(i) => {
                self.eval(&i.test);
                let t = self.typeof_test(&i.test);
                let n = self.push_narrow(&t, true);
                self.stmt(&i.consequent);
                self.narrow.truncate(self.narrow.len() - n);
                if let Some(a) = &i.alternate {
                    let n = self.push_narrow(&t, false);
                    self.stmt(a);
                    self.narrow.truncate(self.narrow.len() - n);
                }
            }
            Statement::ForStatement(f) => {
                match &f.init {
                    Some(ForStatementInit::VariableDeclaration(d)) => self.var_decl(d),
                    Some(e) => {
                        if let Some(e) = e.as_expression() {
                            self.eval(e);
                        }
                    }
                    None => {}
                }
                if let Some(t) = &f.test {
                    self.eval(t);
                }
                if let Some(u) = &f.update {
                    self.eval(u);
                }
                self.stmt(&f.body);
            }
            Statement::ForInStatement(f) => {
                self.eval(&f.right);
                self.for_left(&f.left, &one(AV::Str));
                self.stmt(&f.body);
            }
            Statement::ForOfStatement(f) => {
                let o = self.eval(&f.right);
                let e = self.elems(&o);
                self.for_left(&f.left, &e);
                self.stmt(&f.body);
            }
            Statement::WhileStatement(w) => {
                self.eval(&w.test);
                self.stmt(&w.body);
            }
            Statement::DoWhileStatement(w) => {
                self.stmt(&w.body);
                self.eval(&w.test);
            }
            Statement::LabeledStatement(l) => self.stmt(&l.body),
            Statement::SwitchStatement(sw) => {
                self.eval(&sw.discriminant);
                for c in &sw.cases {
                    if let Some(t) = &c.test {
                        self.eval(t);
                    }
                    c.consequent.iter().for_each(|s| self.stmt(s));
                }
            }
            Statement::ThrowStatement(t) => {
                self.eval(&t.argument);
            }
            Statement::TryStatement(t) => {
                t.block.body.iter().for_each(|s| self.stmt(s));
                if let Some(h) = &t.handler {
                    if let Some(p) = &h.param {
                        self.bind(&p.pattern, &one(AV::Unknown));
                    }
                    h.body.body.iter().for_each(|s| self.stmt(s));
                }
                if let Some(f) = &t.finalizer {
                    f.body.iter().for_each(|s| self.stmt(s));
                }
            }
            Statement::WithStatement(w) => {
                self.eval(&w.object);
                self.stmt(&w.body);
            }
            _ => walk::walk_statement(self, s),
        }
    }

    /// A statement list; after `if (test) <always leaves>` the rest of the list sees the test as false.
    fn stmts(&mut self, list: &[Statement<'a>]) {
        let mut pushed = 0;
        for s in list {
            self.stmt(s);
            if let Statement::IfStatement(i) = s
                && i.alternate.is_none()
                && exits(&i.consequent)
            {
                let t = self.typeof_test(&i.test);
                pushed += self.push_narrow(&t, false);
            }
        }
        self.narrow.truncate(self.narrow.len() - pushed);
    }

    fn var_decl(&mut self, d: &VariableDeclaration<'a>) {
        for v in &d.declarations {
            let val = match &v.init {
                Some(e) => self.eval(e),
                None => continue,
            };
            self.bind(&v.id, &val);
        }
    }

    fn for_left(&mut self, l: &ForStatementLeft<'a>, v: &Set) {
        match l {
            ForStatementLeft::VariableDeclaration(d) => {
                for x in &d.declarations {
                    self.bind(&x.id, v);
                }
            }
            _ => {
                if let Some(t) = l.as_assignment_target() {
                    self.assign(t, v);
                }
            }
        }
    }

    fn class(&mut self, c: &Class<'a>) -> Set {
        let a = self.alloc(c.span.start, Kind::Fn(c.span.start), self.cur);
        let proto = self.alloc(c.span.start, Kind::Proto(a), self.cur);
        if let Some(h) = &c.heritage {
            let base = self.eval(&h.expression);
            let bp = self.read(&base, "prototype");
            Self::union_map(&mut self.protos, proto, &bp, &mut self.changed);
        }
        for e in &c.body.body {
            if let ClassElement::MethodDefinition(m) = e {
                if m.kind == MethodDefinitionKind::Constructor {
                    continue;
                }
                let f = self.alloc(m.value.span.start, Kind::Fn(m.value.span.start), self.cur);
                if let Some(k) = m.key.static_name() {
                    let target = if m.r#static { a } else { proto };
                    let v = if m.kind == MethodDefinitionKind::Method { one(AV::Obj(f)) } else { one(AV::Unknown) };
                    self.union_prop(target, &k, &v);
                }
            }
        }
        one(AV::Obj(a))
    }

    fn assign(&mut self, t: &AssignmentTarget<'a>, v: &Set) {
        match t {
            AssignmentTarget::AssignmentTargetIdentifier(id) => self.write_ident(id, v),
            AssignmentTarget::StaticMemberExpression(m) => {
                let o = self.eval(&m.object);
                self.record(m.span, "set", &o, None);
                self.write(&o, &m.property.name, v);
            }
            AssignmentTarget::ComputedMemberExpression(m) => {
                let o = self.eval(&m.object);
                let k = self.eval(&m.expression);
                self.record(m.span, "eset", &o, None);
                self.record(m.span, "esetkey", &k, None);
                let name = static_key(&m.expression);
                self.write_computed(&o, &k, name.as_deref(), v);
            }
            _ => {
                walk::walk_assignment_target(self, t);
            }
        }
    }

    fn write_ident(&mut self, id: &IdentifierReference<'a>, v: &Set) {
        match self.scoping.get_reference(id.reference_id()).symbol_id() {
            Some(sym) => {
                let k = self.sym_key(sym);
                self.union_var(k, v);
            }
            None => {
                let e = self.globals.entry(id.name.to_string()).or_default();
                add(e, v, &mut self.changed);
            }
        }
    }

    fn read_ident(&mut self, id: &IdentifierReference<'a>) -> Set {
        match self.scoping.get_reference(id.reference_id()).symbol_id() {
            Some(sym) => {
                let k = self.sym_key(sym);
                let v = self.vars.get(&k).cloned().unwrap_or_default();
                let mut v = v;
                for (n, tag, want) in &self.narrow {
                    if *n == k {
                        v.retain(|x| *x == AV::Unknown || (self.typeof_tag(*x) == Some(tag.as_str())) == *want);
                    }
                }
                v
            }
            None => self.global(&id.name),
        }
    }

    fn args(&mut self, args: &[Argument<'a>]) -> (Vec<Set>, Option<Set>) {
        let mut out = Vec::new();
        let mut spread: Option<Set> = None;
        for a in args {
            match a {
                Argument::SpreadElement(s) => {
                    let v = self.eval(&s.argument);
                    let e = self.elems(&v);
                    add(spread.get_or_insert_default(), &e, &mut false);
                }
                _ => {
                    let v = self.eval(a.to_expression());
                    if let Some(sp) = &mut spread {
                        add(sp, &v, &mut false);
                    } else {
                        out.push(v);
                    }
                }
            }
        }
        (out, spread)
    }

    // ---- expressions ----
    fn eval(&mut self, e: &Expression<'a>) -> Set {
        self.steps += 1;
        match e {
            Expression::NumericLiteral(_) => one(AV::Num),
            Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => {
                if let Expression::TemplateLiteral(t) = e {
                    for x in &t.expressions {
                        self.eval(x);
                    }
                }
                one(AV::Str)
            }
            Expression::BooleanLiteral(_) => one(AV::Bool),
            Expression::NullLiteral(_) => one(AV::Null),
            Expression::BigIntLiteral(_) => one(AV::BigInt),
            Expression::RegExpLiteral(r) => one(AV::Obj(self.alloc(r.span.start, Kind::RegExp, self.cur))),
            Expression::Identifier(id) => self.read_ident(id),
            Expression::ThisExpression(_) => {
                let i = self.this_inst();
                if i == 0 { one(AV::Unknown) } else { self.this.get(&i).cloned().unwrap_or_default() }
            }
            Expression::ParenthesizedExpression(p) => self.eval(&p.expression),
            Expression::SequenceExpression(s) => {
                let mut last = Set::new();
                for x in &s.expressions {
                    last = self.eval(x);
                }
                last
            }
            Expression::ConditionalExpression(c) => {
                self.eval(&c.test);
                let t = self.typeof_test(&c.test);
                let n = self.push_narrow(&t, true);
                let mut a = self.eval(&c.consequent);
                self.narrow.truncate(self.narrow.len() - n);
                let n = self.push_narrow(&t, false);
                let b = self.eval(&c.alternate);
                self.narrow.truncate(self.narrow.len() - n);
                add(&mut a, &b, &mut false);
                a
            }
            Expression::LogicalExpression(l) => {
                let mut a = self.eval(&l.left);
                let t = self.typeof_test(&l.left);
                let n = self.push_narrow(&t, l.operator == LogicalOperator::And);
                let b = self.eval(&l.right);
                self.narrow.truncate(self.narrow.len() - n);
                add(&mut a, &b, &mut false);
                a
            }
            Expression::UnaryExpression(u) => {
                self.eval(&u.argument);
                match u.operator {
                    UnaryOperator::LogicalNot | UnaryOperator::Delete => one(AV::Bool),
                    UnaryOperator::Typeof => one(AV::Str),
                    UnaryOperator::Void => one(AV::Undef),
                    _ => one(AV::Num),
                }
            }
            Expression::UpdateExpression(u) => {
                let n = one(AV::Num);
                match &u.argument {
                    SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                        self.read_ident(id);
                        self.write_ident(id, &n);
                    }
                    SimpleAssignmentTarget::StaticMemberExpression(m) => {
                        let o = self.eval(&m.object);
                        self.record(m.span, "set", &o, None);
                        self.write(&o, &m.property.name, &n);
                    }
                    SimpleAssignmentTarget::ComputedMemberExpression(m) => {
                        let o = self.eval(&m.object);
                        let k = self.eval(&m.expression);
                        self.record(m.span, "eset", &o, None);
                        self.record(m.span, "esetkey", &k, None);
                        let name = static_key(&m.expression);
                        self.write_computed(&o, &k, name.as_deref(), &n);
                    }
                    _ => {}
                }
                n
            }
            Expression::BinaryExpression(b) => {
                let l = self.eval(&b.left);
                let r = self.eval(&b.right);
                use BinaryOperator as B;
                let cat = match b.operator {
                    B::Addition | B::Subtraction | B::Multiplication | B::Division | B::Remainder | B::Exponential => {
                        Some("arith")
                    }
                    B::BitwiseAnd | B::BitwiseOR | B::BitwiseXOR | B::ShiftLeft | B::ShiftRight | B::ShiftRightZeroFill => {
                        Some("bit")
                    }
                    B::LessThan | B::GreaterThan | B::LessEqualThan | B::GreaterEqualThan => Some("compare"),
                    B::Equality | B::Inequality | B::StrictEquality | B::StrictInequality => Some("equal"),
                    _ => None,
                };
                if let Some(cat) = cat {
                    self.record(b.span, cat, &l, Some(&r));
                }
                match b.operator {
                    B::Addition => plus(&l, &r),
                    _ if cat == Some("compare") || cat == Some("equal") => one(AV::Bool),
                    B::In | B::Instanceof => one(AV::Bool),
                    _ => one(AV::Num),
                }
            }
            Expression::AssignmentExpression(a) => {
                let mut v = self.eval(&a.right);
                if a.operator != AssignmentOperator::Assign {
                    if a.operator.is_logical() {
                        // `x ||= v`: the old value or v
                        if let Some(old) = self.target_value(&a.left) {
                            add(&mut v, &old, &mut false);
                        }
                    } else if a.operator == AssignmentOperator::Addition {
                        let old = self.target_value(&a.left).unwrap_or_default();
                        v = plus(&old, &v);
                    } else {
                        v = one(AV::Num);
                    }
                }
                self.assign(&a.left, &v);
                v
            }
            Expression::ArrayExpression(arr) => {
                let o = self.alloc(arr.span.start, Kind::Arr, self.cur);
                let mut all = Set::new();
                for el in &arr.elements {
                    match el {
                        ArrayExpressionElement::SpreadElement(s) => {
                            let v = self.eval(&s.argument);
                            let ev = self.elems(&v);
                            add(&mut all, &ev, &mut false);
                        }
                        ArrayExpressionElement::Elision(_) => {
                            all.insert(AV::Undef);
                        }
                        _ => {
                            let v = self.eval(el.to_expression());
                            add(&mut all, &v, &mut false);
                        }
                    }
                }
                Self::union_map(&mut self.elem, o, &all, &mut self.changed);
                one(AV::Obj(o))
            }
            Expression::ObjectExpression(obj) => {
                let o = self.alloc(obj.span.start, Kind::Lit, self.cur);
                for p in &obj.properties {
                    match p {
                        ObjectPropertyKind::ObjectProperty(p) => {
                            let v = self.eval(&p.value);
                            if p.kind != PropertyKind::Init {
                                continue;
                            }
                            match p.key.static_name() {
                                Some(k) if !p.computed => self.union_prop(o, &k, &v),
                                _ => {
                                    if let Some(k) = p.key.as_expression() {
                                        self.eval(k);
                                    }
                                    Self::union_map(&mut self.anyprop, o, &v, &mut self.changed)
                                }
                            }
                        }
                        ObjectPropertyKind::SpreadProperty(s) => {
                            self.eval(&s.argument);
                            Self::union_map(&mut self.anyprop, o, &one(AV::Unknown), &mut self.changed);
                        }
                    }
                }
                one(AV::Obj(o))
            }
            Expression::FunctionExpression(f) => one(AV::Obj(self.alloc(f.span.start, Kind::Fn(f.span.start), self.cur))),
            Expression::ArrowFunctionExpression(f) => {
                one(AV::Obj(self.alloc(f.span.start, Kind::Fn(f.span.start), self.cur)))
            }
            Expression::ClassExpression(c) => self.class(c),
            Expression::StaticMemberExpression(m) => {
                let o = self.eval(&m.object);
                self.record(m.span, "get", &o, None);
                self.read(&o, &m.property.name)
            }
            Expression::ComputedMemberExpression(m) => {
                let o = self.eval(&m.object);
                let k = self.eval(&m.expression);
                self.record(m.span, "eget", &o, None);
                self.record(m.span, "egetkey", &k, None);
                let name = static_key(&m.expression);
                self.read_computed(&o, &k, name.as_deref())
            }
            Expression::CallExpression(c) => self.call_expr(c),
            Expression::NewExpression(n) => {
                let callee = self.eval(&n.callee);
                let (args, spread) = self.args(&n.arguments);
                self.construct(n.span.start, &callee, &args, spread.as_ref())
            }
            Expression::AwaitExpression(a) => {
                let v = self.eval(&a.argument);
                self.awaited(&v)
            }
            Expression::ChainExpression(_) => {
                walk::walk_expression(self, e);
                one(AV::Unknown)
            }
            _ => {
                walk::walk_expression(self, e);
                one(AV::Unknown)
            }
        }
    }

    fn target_value(&mut self, t: &AssignmentTarget<'a>) -> Option<Set> {
        match t {
            AssignmentTarget::AssignmentTargetIdentifier(id) => Some(self.read_ident(id)),
            AssignmentTarget::StaticMemberExpression(m) => {
                let o = self.eval(&m.object);
                Some(self.read(&o, &m.property.name))
            }
            AssignmentTarget::ComputedMemberExpression(m) => {
                let o = self.eval(&m.object);
                let k = self.eval(&m.expression);
                let name = static_key(&m.expression);
                Some(self.read_computed(&o, &k, name.as_deref()))
            }
            _ => None,
        }
    }

    fn call_expr(&mut self, c: &CallExpression<'a>) -> Set {
        let site = c.span.start;
        match &c.callee {
            Expression::StaticMemberExpression(m) => {
                let name = m.property.name.as_str();
                let recv = self.eval(&m.object);
                self.record(m.span, "method", &recv, None);
                let f = self.read(&recv, name);
                let (args, spread) = self.args(&c.arguments);
                // Object.defineProperty(o, "k", { value: v }): o.k = v
                if name == "defineProperty"
                    && let Some(Argument::StringLiteral(k)) = c.arguments.get(1)
                    && f.iter().any(|v| matches!(v, AV::Obj(o) if matches!(self.kind(*o), Kind::BFn(n) if n == "Object.defineProperty")))
                {
                    let target = args.first().cloned().unwrap_or_default();
                    let desc = args.get(2).cloned().unwrap_or_default();
                    let v = self.read(&desc, "value");
                    self.write(&target, &k.value, &v);
                    return target;
                }
                // f.call(this, …) / f.apply(this, args)
                let real: Set = recv.iter().copied().filter(|v| matches!(v, AV::Obj(o) if matches!(self.kind(*o), Kind::Fn(_)))).collect();
                let via = f.iter().any(|v| matches!(v, AV::Obj(o) if matches!(self.kind(*o), Kind::BFn(n) if n == "Function.prototype.call" || n == "Function.prototype.apply")));
                if via && !real.is_empty() {
                    let this = args.first().cloned().unwrap_or_else(|| one(AV::Undef));
                    if name == "call" {
                        return self.call(site, &real, &this, args.get(1..).unwrap_or(&[]), spread.as_ref());
                    }
                    // apply: the array's positions (an `arguments` object keeps them)
                    let arr = args.get(1).cloned().unwrap_or_default();
                    let mut pos: Vec<Set> = Vec::new();
                    let mut other = Set::new();
                    for v in &arr {
                        if let AV::Obj(o) = v
                            && matches!(self.kind(*o), Kind::Args)
                        {
                            let inst = self.allocs[*o as usize].inst;
                            for (n, p) in self.args_pos.get(&inst).cloned().unwrap_or_default().into_iter().enumerate() {
                                if pos.len() <= n {
                                    pos.push(Set::new());
                                }
                                add(&mut pos[n], &p, &mut false);
                            }
                        } else {
                            let e = self.elems(&one(*v));
                            add(&mut other, &e, &mut false);
                        }
                    }
                    let spread = if other.is_empty() { None } else { Some(other) };
                    return self.call(site, &real, &this, &pos, spread.as_ref());
                }
                // each receiver calls the method it finds, with itself as `this`
                let mut out = Set::new();
                for r in recv.clone() {
                    let r1 = one(r);
                    let fr = self.read(&r1, name);
                    if !matches!(r, AV::Null | AV::Undef) {
                        let t = self.targets.entry((m.span.start, m.span.end)).or_default();
                        for v in &fr {
                            match v {
                                AV::Obj(o) if matches!(self.allocs[*o as usize].kind, Kind::Fn(_)) => {
                                    let Kind::Fn(f) = self.allocs[*o as usize].kind else { unreachable!() };
                                    t.0.insert(f);
                                }
                                _ => t.1 = true,
                            }
                        }
                        if fr.is_empty() {
                            t.1 = true;
                        }
                    }
                    let x = self.call(site, &fr, &r1, &args, spread.as_ref());
                    add(&mut out, &x, &mut false);
                }
                out
            }
            Expression::Identifier(id) if id.name == "require"
                && let Some(Argument::StringLiteral(m)) = c.arguments.first() =>
            {
                let f = self.read_ident(id);
                self.record(c.span, "call", &f, None);
                one(self.builtin(m.value.trim_start_matches("node:")))
            }
            Expression::Identifier(id) => {
                let f = self.read_ident(id);
                self.record(c.span, "call", &f, None);
                let (args, spread) = self.args(&c.arguments);
                self.call(site, &f, &one(AV::Undef), &args, spread.as_ref())
            }
            callee => {
                let f = self.eval(callee);
                let (args, spread) = self.args(&c.arguments);
                self.call(site, &f, &one(AV::Undef), &args, spread.as_ref())
            }
        }
    }
}

/// The types of `l + r`: a string if a side can be a string (or an object, through ToPrimitive), a number if both
/// sides can be number-like; unknown stays unknown (it doesn't make every sum a possible string).
fn plus(l: &Set, r: &Set) -> Set {
    let numeric = |s: &Set| s.iter().any(|v| matches!(v, AV::Num | AV::Bool | AV::Undef | AV::Null));
    let stringy = |s: &Set| s.iter().any(|v| matches!(v, AV::Str | AV::Obj(_)));
    let mut out = Set::new();
    if stringy(l) || stringy(r) {
        out.insert(AV::Str);
    }
    if numeric(l) && numeric(r) {
        out.insert(AV::Num);
    }
    if l.contains(&AV::Unknown) || r.contains(&AV::Unknown) {
        out.insert(AV::Unknown);
    }
    if l.contains(&AV::BigInt) && r.contains(&AV::BigInt) {
        out.insert(AV::BigInt);
    }
    out
}

fn prim_tag(v: AV) -> &'static str {
    match v {
        AV::Num => "num",
        AV::Str => "string",
        AV::Bool => "boolean",
        AV::Undef => "undefined",
        AV::Null => "null",
        AV::Sym => "symbol",
        AV::BigInt => "bigint",
        _ => "?",
    }
}

const GLOBAL_FNS: &[&str] = &[
    "Object", "Function", "Array", "String", "Number", "Boolean", "Symbol", "BigInt", "Date", "RegExp", "Error",
    "TypeError", "RangeError", "SyntaxError", "ReferenceError", "EvalError", "URIError", "Map", "Set", "WeakMap",
    "WeakSet", "Promise", "Proxy", "ArrayBuffer", "DataView", "Int8Array", "Uint8Array", "Uint8ClampedArray",
    "Int16Array", "Uint16Array", "Int32Array", "Uint32Array", "Float32Array", "Float64Array", "BigInt64Array",
    "BigUint64Array", "parseInt", "parseFloat", "isNaN", "isFinite", "encodeURIComponent", "decodeURIComponent",
    "encodeURI", "decodeURI", "escape", "unescape", "eval", "require", "setTimeout", "setInterval", "clearTimeout",
    "clearInterval", "setImmediate", "queueMicrotask", "Buffer", "print", "TextEncoder", "TextDecoder",
];

const FUNCTION_PROTO: &[&str] = &["call", "apply", "bind", "toString"];
const ARRAY_PROTO: &[&str] = &[
    "push", "pop", "shift", "unshift", "slice", "splice", "concat", "join", "reverse", "sort", "indexOf", "lastIndexOf",
    "forEach", "map", "filter", "reduce", "reduceRight", "some", "every", "find", "findIndex", "includes", "fill",
    "keys", "values", "entries", "toString", "flat", "flatMap", "at", "copyWithin",
];
const OBJECT_PROTO: &[&str] =
    &["hasOwnProperty", "toString", "valueOf", "isPrototypeOf", "propertyIsEnumerable", "toLocaleString", "constructor"];

/// Does the statement always leave its list (return, throw, break, continue)?
fn exits(s: &Statement) -> bool {
    match s {
        Statement::ReturnStatement(_) | Statement::ThrowStatement(_) | Statement::BreakStatement(_)
        | Statement::ContinueStatement(_) => true,
        Statement::BlockStatement(b) => b.body.last().is_some_and(exits),
        _ => false,
    }
}

fn static_key(e: &Expression) -> Option<String> {
    match e {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        _ => None,
    }
}

impl<'a, 's> Visit<'a> for A<'a, 's> {
    fn visit_expression(&mut self, e: &Expression<'a>) {
        self.eval(e);
    }
    fn visit_statement(&mut self, s: &Statement<'a>) {
        self.stmt(s);
    }
    fn visit_function(&mut self, _f: &Function<'a>, _flags: ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _f: &ArrowFunctionExpression<'a>) {}
    fn visit_class(&mut self, _c: &Class<'a>) {}
}

/// Parses `source` as a script (sloppy, CommonJS-style, as `typeprobe` does) and analyses all of it: offsets
/// are the file's own.
pub fn analyze_source(source: &str) -> std::result::Result<Result, String> {
    let alloc = oxc_allocator::Allocator::default();
    let ret = oxc_parser::Parser::new(&alloc, source, oxc_span::SourceType::cjs()).parse();
    if ret.fatal_error || !ret.diagnostics.is_empty() {
        return Err(format!("{} parse errors", ret.diagnostics.len()));
    }
    let sem = oxc_semantic::SemanticBuilder::new().with_build_nodes(true).build(&ret.program);
    Ok(analyze(&ret.program, &sem.semantic, source.len() as u32))
}

#[cfg(test)]
mod tests;
