// typeprobe (T000, T131): rewrites a JS file so that, run under node, every property access, call and binary
// operator records the types it sees per site. Not part of the compiler.
// Usage: typeprobe [--sites] in.js out.js sites.tsv
//   default: objects told apart by V8 map (`node --allow-natives-syntax`), numbers as int/double;
//   --sites: objects by where they were made (literal/function site, or `inst@` their prototype's tag), as
//            `js2wasi3 types` names them; numbers as `num`.
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};
use std::collections::HashSet;

struct Edit {
    pos: u32,
    end_first: u8, // 0 = closing text (emitted before openings at the same position)
    order: i64,
    text: String,
}

#[derive(Default)]
struct P {
    edits: Vec<Edit>,
    dels: Vec<(u32, u32)>,
    sites: Vec<(String, Span)>,
    seq: i64,
    writes: HashSet<(u32, u32)>,
    callees: HashSet<(u32, u32)>,
    chain: u32,
    sites_mode: bool,
    /// where a function declaration's tag goes: the start of the enclosing body (after directives)
    decl_at: Vec<u32>,
    /// functions written as methods (`m() {}`, getters, class methods): not wrapped
    methods: HashSet<u32>,
}

fn key(s: Span) -> (u32, u32) {
    (s.start, s.end)
}

impl P {
    fn site(&mut self, cat: &str, at: Span) -> usize {
        self.sites.push((cat.to_string(), at));
        self.sites.len() - 1
    }
    fn wrap(&mut self, s: Span, open: String, close: &str) {
        self.seq += 1;
        self.edits.push(Edit { pos: s.start, end_first: 1, order: self.seq, text: open });
        self.edits.push(Edit { pos: s.end, end_first: 0, order: -self.seq, text: close.to_string() });
    }
    fn member(&mut self, span: Span, obj: &Expression, computed: Option<&Expression>) {
        if self.chain > 0 || matches!(obj, Expression::Super(_)) {
            return;
        }
        let w = self.writes.contains(&key(span));
        let cat = match (computed.is_some(), w, self.callees.contains(&key(span))) {
            (false, true, _) => "set",
            (false, false, true) => "method",
            (false, false, false) => "get",
            (true, true, _) => "eset",
            (true, false, _) => "eget",
        };
        let id = self.site(cat, span);
        self.wrap(obj.span(), format!("__r({id},"), ")");
        if let Some(k) = computed {
            let kid = self.site(if w { "esetkey" } else { "egetkey" }, span);
            self.wrap(k.span(), format!("__r({kid},"), ")");
        }
    }
}

const OPS: &[(&str, &str)] = &[
    ("+", "arith"), ("-", "arith"), ("*", "arith"), ("/", "arith"), ("%", "arith"), ("**", "arith"),
    ("&", "bit"), ("|", "bit"), ("^", "bit"), ("<<", "bit"), (">>", "bit"), (">>>", "bit"),
    ("<", "compare"), (">", "compare"), ("<=", "compare"), (">=", "compare"),
    ("==", "equal"), ("!=", "equal"), ("===", "equal"), ("!==", "equal"),
];

fn body_at(b: &FunctionBody) -> u32 {
    b.directives.last().map_or(b.span.start + 1, |d| d.span.end)
}

impl<'a> Visit<'a> for P {
    fn visit_program(&mut self, it: &Program<'a>) {
        self.decl_at.push(it.directives.last().map_or(it.span.start, |d| d.span.end));
        walk::walk_program(self, it);
    }
    fn visit_object_property(&mut self, it: &ObjectProperty<'a>) {
        if it.method || it.kind != PropertyKind::Init {
            self.methods.insert(it.value.span().start);
        }
        walk::walk_object_property(self, it);
    }
    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        self.methods.insert(it.value.span.start);
        walk::walk_method_definition(self, it);
    }
    fn visit_object_expression(&mut self, it: &ObjectExpression<'a>) {
        if self.sites_mode {
            self.wrap(it.span, format!("__a({},", it.span.start), ")");
        }
        walk::walk_object_expression(self, it);
    }
    fn visit_function(&mut self, it: &Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
        if self.sites_mode {
            if it.is_expression() {
                if !self.methods.contains(&it.span.start) {
                    self.wrap(it.span, format!("__a({},", it.span.start), ")");
                }
            } else if let (Some(id), Some(&at)) = (&it.id, self.decl_at.last()) {
                self.seq += 1;
                self.edits.push(Edit { pos: at, end_first: 1, order: self.seq, text: format!("__a({},{});", it.span.start, id.name) });
            }
        }
        if let Some(b) = &it.body {
            self.decl_at.push(body_at(b));
        }
        walk::walk_function(self, it, flags);
        if it.body.is_some() {
            self.decl_at.pop();
        }
    }
    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        if self.sites_mode {
            self.wrap(it.span, format!("__a({},", it.span.start), ")");
        }
        let at = match &it.body {
            ArrowFunctionBody::FunctionBody(b) => body_at(b),
            _ => it.span.start,
        };
        self.decl_at.push(at);
        walk::walk_arrow_function_expression(self, it);
        self.decl_at.pop();
    }
    fn visit_class(&mut self, it: &Class<'a>) {
        if self.sites_mode {
            if it.is_expression() {
                self.wrap(it.span, format!("__a({},", it.span.start), ")");
            } else if let Some(id) = &it.id {
                self.seq += 1;
                self.edits.push(Edit { pos: it.span.end, end_first: 0, order: i64::MAX, text: format!(";__a({},{});", it.span.start, id.name) });
            }
        }
        walk::walk_class(self, it);
    }
    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if let Some(m) = it.left.as_member_expression() {
            self.writes.insert(key(m.span()));
        }
        walk::walk_assignment_expression(self, it);
    }
    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        if let Some(m) = it.argument.as_member_expression() {
            self.writes.insert(key(m.span()));
        }
        walk::walk_update_expression(self, it);
    }
    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        if it.callee.as_member_expression().is_some() {
            self.wrap(it.callee.span(), "(".into(), ")");
        }
        walk::walk_new_expression(self, it);
    }
    fn visit_chain_expression(&mut self, it: &ChainExpression<'a>) {
        self.chain += 1;
        walk::walk_chain_expression(self, it);
        self.chain -= 1;
    }
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some(m) = it.callee.as_member_expression() {
            self.callees.insert(key(m.span()));
        }
        if self.chain == 0
            && let Expression::Identifier(id) = &it.callee
                && id.name != "eval" {
                    let s = self.site("call", it.span);
                    self.wrap(it.callee.span(), format!("__r({s},"), ")");
                }
        walk::walk_call_expression(self, it);
    }
    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        self.member(it.span, &it.object, None);
        walk::walk_static_member_expression(self, it);
    }
    fn visit_computed_member_expression(&mut self, it: &ComputedMemberExpression<'a>) {
        self.member(it.span, &it.object, Some(&it.expression));
        walk::walk_computed_member_expression(self, it);
    }
    fn visit_binary_expression(&mut self, it: &BinaryExpression<'a>) {
        let op = it.operator.as_str();
        if let Some(n) = OPS.iter().position(|(o, _)| *o == op) {
            let id = self.site(OPS[n].1, it.span);
            let (l, r) = (it.left.span(), it.right.span());
            self.wrap(Span::new(l.start, r.end), format!("__o{n}({id},"), ")");
            // the operator between the operands becomes a comma (after the left operand's closings)
            self.edits.push(Edit { pos: l.end, end_first: 0, order: -self.seq, text: ",".into() });
            self.dels.push((l.end, r.start));
        }
        walk::walk_binary_expression(self, it);
    }
}

fn main() {
    let mut a: Vec<String> = std::env::args().collect();
    let sites_mode = a.get(1).is_some_and(|x| x == "--sites");
    if sites_mode {
        a.remove(1);
    }
    let src = std::fs::read_to_string(&a[1]).unwrap();
    let alloc = Allocator::default();
    let ret = Parser::new(&alloc, &src, SourceType::cjs()).parse();
    assert!(ret.diagnostics.is_empty(), "parse errors");
    let mut p = P { sites_mode, ..P::default() };
    p.visit_program(&ret.program);
    p.edits.sort_by_key(|e| (e.pos, e.end_first, e.order));
    let mut out = String::from(if sites_mode { include_str!("header_sites.js") } else { include_str!("header.js") });
    for (n, (o, _)) in OPS.iter().enumerate() {
        out.push_str(&format!("function __o{n}(i,a,b){{__b(i,a,b);return a{o}b}}\n"));
    }
    out.push_str(&format!("__init({});\n", p.sites.len()));
    let b = src.as_bytes();
    let mut ei = 0;
    let mut i = 0u32;
    let deleted = |i: u32| p.dels.iter().any(|&(s, e)| i >= s && i < e);
    let mut bytes: Vec<u8> = out.into_bytes();
    while (i as usize) <= b.len() {
        while ei < p.edits.len() && p.edits[ei].pos == i {
            bytes.extend_from_slice(p.edits[ei].text.as_bytes());
            ei += 1;
        }
        if (i as usize) < b.len() && !deleted(i) {
            bytes.push(b[i as usize]);
        }
        i += 1;
    }
    std::fs::write(&a[2], bytes).unwrap();
    let mut t = String::new();
    for (id, (cat, at)) in p.sites.iter().enumerate() {
        let line = src[..at.start as usize].matches('\n').count() + 1;
        t.push_str(&format!("{id}\t{cat}\t{line}\t{}:{}\n", at.start, at.end));
    }
    std::fs::write(&a[3], t).unwrap();
}
