//! Built-ins written in JS (T21, D5): the prelude.
//!
//! The prelude (`crates/lower/builtins/*.js`) is appended to the user's source and parsed with it
//! as one module ([`parse`]), so it goes through the same lowering, inference and backend as user
//! code. Its conventions:
//! - Top-level names start with `$$`. `$$Math_floor` implements `Math.floor`: the lowering maps
//!   a static member `A.b` of an undeclared global `A` to the prelude function `$$A_b`, if there is
//!   one. `$$parseInt` implements the global function `parseInt`. `$$String_prototype_slice` is
//!   `String.prototype.slice`: installed on the prototype (calls go through it) when the user's
//!   code names a property `slice` (`x.slice`, `x["slice"]`). Other `$$` names are helpers.
//! - Only function declarations at the top level, and variables with a literal initializer
//!   (state, e.g. `Math.random`'s): they become globals, in a command too.
//! - A call of an undeclared `__intrinsic_<name>(…)` is an IR operation (see
//!   `Lowerer::intrinsic_call`); only the prelude may use them.
//!
//! Only the prelude functions the user's code can reach are declared and lowered ([`Prelude`]), so a
//! program without built-ins compiles to the same IR as without the prelude.

use std::collections::{HashMap, HashSet};

use js2wasi3_frontend::{self as frontend, Allocator, Diagnostics, Program};
use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, Expression, Statement};
use oxc_span::{GetSpan, Span};
use oxc_syntax::symbol::SymbolId;

use crate::unicode_props::{NAMES, TABLES};

/// The JS source of the built-ins.
/// `Array.prototype` methods that are native for arrays; the prelude's are their generic
/// fallbacks (T35).
pub const ARRAY_NATIVE: &[&str] = &["push", "pop", "join", "toString"];

pub const PRELUDE: &str = concat!(
    include_str!("../builtins/math.js"),
    include_str!("../builtins/string.js"),
    include_str!("../builtins/array.js"),
    include_str!("../builtins/object.js"),
    include_str!("../builtins/reflect.js"),
    include_str!("../builtins/regexp.js"),
    include_str!("../builtins/date.js"),
    include_str!("../builtins/unicode-case.js"),
    include_str!("../builtins/json.js"),
    include_str!("../builtins/map.js"),
    include_str!("../builtins/symbol.js"),
    include_str!("../builtins/console.js"),
    include_str!("../builtins/typedarray.js"),
    include_str!("../builtins/stream.js"),
    include_str!("../builtins/asyncgen.js"),
    include_str!("../builtins/bigint.js"),
    include_str!("../builtins/module.js"),
);

/// Prelude dependencies the AST doesn't show (T25): a `Map`/`Set` iterates through
/// `[Symbol.iterator]` (installed with `entries`/`values`) and its iterator's `next`, which the
/// user's code never names; a RegExp converts to a string through its `toString` (T27).
const IMPLIED: &[(&str, &[&str])] = &[
    (
        "$$Map",
        &["$$Map_prototype_entries", "$$MapIterator_prototype_next"],
    ),
    (
        "$$Map_groupBy",
        &["$$Map_prototype_entries", "$$MapIterator_prototype_next"],
    ),
    (
        "$$Set",
        &["$$Set_prototype_values", "$$SetIterator_prototype_next"],
    ),
    // ToPrimitive of a RegExp (T27) reaches its `toString` without the code naming it.
    ("$$RegExpCreate", &["$$RegExp_prototype_toString"]),
    ("$$RegExp", &["$$RegExp_prototype_toString"]),
    // `matchAll`/`replaceAll` read `flags` through a variable key.
    ("$$reCreate", &["$$RegExp_prototype_flags"]),
    // `Date()` and `Date` as a value (T26); ToPrimitive of a Date reaches `toString`/`valueOf`,
    // `JSON.stringify` its `toJSON`, `console.log` `$$dateInspect`.
    (
        "$$Date",
        &[
            "$$DateCall",
            "$$dateInspect",
            "$$Date_prototype_toString",
            "$$Date_prototype_valueOf",
            "$$Date_prototype_toJSON",
            "$$Date_prototype_toISOString",
        ],
    ),
    ("$$Map_prototype_keys", &["$$MapIterator_prototype_next"]),
    ("$$Map_prototype_values", &["$$MapIterator_prototype_next"]),
    ("$$Map_prototype_entries", &["$$MapIterator_prototype_next"]),
    ("$$Set_prototype_values", &["$$SetIterator_prototype_next"]),
    ("$$Set_prototype_entries", &["$$SetIterator_prototype_next"]),
];

/// Prototype methods installed under another name too (T25): `Set.prototype.keys` is `values`;
/// (T26) `Date.prototype.toGMTString` is `toUTCString`.
const ALIASES: &[(&str, &str)] = &[
    ("keys", "$$Set_prototype_values"),
    ("toGMTString", "$$Date_prototype_toUTCString"),
];

/// The prelude functions `yield*` in an async generator calls (T29, `builtins/asyncgen.js`).
pub(crate) const ASYNC_DELEGATE: [&str; 7] = [
    "$$asyncDelegateOpen",
    "$$asyncDelegateNext",
    "$$asyncDelegateMethod",
    "$$asyncDelegateCall",
    "$$asyncDelegateClose",
    "$$asyncDelegateResult",
    "$$asyncDelegateNoThrow",
];

/// The prelude function a regex literal calls (T27): `$$RegExpCreate(source, flags)`.
pub(crate) const REGEXP_CREATE: &str = "$$RegExpCreate";

/// The prelude function every RegExp is made by (T27).
const REGEXP_CREATE_ANY: &str = "$$reCreate";

/// `RegExp.prototype[Symbol.x]` (`$$RegExp_prototype_$x`, T27) and the property names that can
/// reach it.
const REGEXP_SYMBOL_METHODS: &[(&str, &[&str])] = &[
    ("match", &["match"]),
    ("matchAll", &["matchAll"]),
    ("replace", &["replace", "replaceAll"]),
    ("search", &["search"]),
    ("split", &["split"]),
];

/// `new Date(…)` (T26); [`DATE_CALL`]: `Date(…)` (a string); [`DATE_INSPECT`]: `console.log`'s text.
pub(crate) const DATE_NEW: &str = "$$Date";
pub(crate) const DATE_CALL: &str = "$$DateCall";
pub(crate) const DATE_INSPECT: &str = "$$dateInspect";

/// The prelude's StringToNumber: the runtime's ToNumber of a string calls it.
pub(crate) const STRING_TO_NUMBER: &str = "$$StringToNumber";

/// The prelude's StringToBigInt (T40): the runtime's comparisons of strings with BigInts call it.
pub(crate) const STRING_TO_BIGINT: &str = "$$StringToBigInt";

/// The prelude's stream reader and pump (T20 slice 6b): the backend lifts and lowers a user
/// world's streams with them.
pub(crate) const STREAMS: [&str; 2] = ["$$StreamReader", "$$StreamPump"];

/// Parse a user module followed by the built-ins prelude (what [`crate::lower`] expects).
pub fn parse<'a>(
    alloc: &'a Allocator,
    path: &str,
    source: &'a str,
) -> Result<Program<'a>, Diagnostics> {
    let mut prelude = String::from(PRELUDE);
    prelude.push_str(&unicode_sets(source));
    frontend::parse_with_prelude(alloc, path, source, &prelude)
}

/// Parse a program from its entry file (T41): the modules it imports (read with `read`) are
/// joined with it ([`frontend::link`]); an entry without module imports is [`parse`]d as is.
/// `command`: a command even if the entry has exports.
pub fn parse_entry<'a>(
    alloc: &'a Allocator,
    path: &str,
    source: &'a str,
    command: bool,
    read: &mut dyn FnMut(&str) -> std::io::Result<String>,
) -> Result<Program<'a>, Diagnostics> {
    let linked = frontend::link(alloc, path, source, command, read)?;
    let Some(map) = linked.map else {
        return parse(alloc, path, source);
    };
    let source: &'a str = alloc.alloc_str(&linked.source);
    let mut prelude = String::from(PRELUDE);
    prelude.push_str(&unicode_sets(source));
    frontend::parse_linked(alloc, path, source, Some(map), &prelude)
}

/// `$$reUnicodeSet(text)` (T27): the code point set of `\p{text}`, for the texts node accepts that
/// the source names (`\p{…}`/`\P{…}` in a regex literal, `\\p{…}` in a string), or `null`. The
/// sets are encoded like unicode-case.js's runs: ranges as (gap from the previous range, length).
fn unicode_sets(source: &str) -> String {
    use std::fmt::Write;
    let mut found: std::collections::BTreeMap<&str, u16> = Default::default();
    let b = source.as_bytes();
    for i in 0..b.len().saturating_sub(2) {
        if b[i] == b'\\' && (b[i + 1] == b'p' || b[i + 1] == b'P') && b[i + 2] == b'{' {
            let rest = &source[i + 3..];
            if let Some(end) = rest.find('}')
                && let Ok(k) = NAMES.binary_search_by(|(n, _)| n.cmp(&&rest[..end]))
            {
                found.insert(&rest[..end], NAMES[k].1);
            }
        }
    }
    let mut js = String::from("function $$reUnicodeSet(text) {\n");
    for (text, t) in found {
        let mut nums = Vec::new();
        let mut prev = 0;
        for r in TABLES[t as usize].chunks(2) {
            nums.push(r[0] - prev);
            nums.push(r[1] - r[0]);
            prev = r[1];
        }
        let _ = write!(js, "  if (text === {text:?}) {{\n    return \"{}\";\n  }}\n", encode(&nums));
    }
    js.push_str("  return null;\n}\n");
    js
}

/// Non-negative numbers as unicode-case.js encodes them: zigzag (so `2 * v`), base-0x4000 digits,
/// continuation units (0x4000–0x7FFF) first, as JS string literal text.
fn encode(nums: &[u32]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for &v in nums {
        let mut z = 2 * v as u64;
        let mut digits = vec![z % 0x4000];
        z /= 0x4000;
        while z > 0 {
            digits.push(0x4000 + z % 0x4000);
            z /= 0x4000;
        }
        for &u in digits.iter().rev() {
            if (0x20..0x7f).contains(&u) && u != 0x22 && u != 0x5c {
                s.push(u as u8 as char);
            } else {
                let _ = write!(s, "\\u{u:04x}");
            }
        }
    }
    s
}

/// What the lowering needs to know about the prelude of one program.
#[derive(Default)]
pub(crate) struct Prelude {
    /// Top-level prelude functions and variables by name.
    pub names: HashMap<String, SymbolId>,
    /// User references to prelude names (errors: they aren't JS globals).
    pub user_refs: Vec<(Span, String)>,
    /// The spans of the prelude functions not needed, in source order.
    dead: Vec<Span>,
    /// The names of the needed top-level declarations.
    live: HashSet<String>,
    /// Does the user's code name a property `bind` (T25: `Function.prototype.bind` is installed
    /// only then)?
    pub bind: bool,
    /// Does it name a property `toString` (T25: functions keep their source text for
    /// `Function.prototype.toString` only then)?
    pub to_string: bool,
    /// Built-in constructors the user's code uses as values (T25: `arr.map(String)`,
    /// `x.constructor === Array`), and `Math`/`JSON`.
    pub ctor_values: HashSet<String>,
    /// Does it name a `constructor` property (T25: the other built-in prototypes then get
    /// constructor stubs)?
    pub constructor: bool,
    /// The property names (and strings) in the user's code (T25: native methods are linked only
    /// when named, see `ir::Module::names`).
    pub user_props: HashSet<String>,
    /// Can the user's code make a BigInt (T40: a literal, or it names the global `BigInt`)?
    pub bigint: bool,
}

/// Properties a built-in constructor has as a function (`Array.apply`, `String.name`): reading
/// them uses the constructor value (T25).
pub(crate) const FUNCTION_PROPS: &[&str] = &["apply", "call", "bind", "name", "length"];

/// `Object.prototype` methods: read from a namespace (`Reflect.hasOwnProperty(…)`, T37) they go
/// through the namespace value, which inherits them.
pub(crate) const OBJECT_PROTO_PROPS: &[&str] = &[
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toString",
    "toLocaleString",
    "valueOf",
    "constructor",
];

/// Is `global.prop` inherited rather than a static of the prelude: `Function.prototype`'s
/// properties of a constructor value, `Object.prototype`'s methods of a namespace or (T26) of a
/// constructor value (`Date.hasOwnProperty(…)`)?
pub(crate) fn inherited_prop(global: &str, prop: &str) -> bool {
    FUNCTION_PROPS.contains(&prop)
        || ((js2wasi3_ir::Namespace::parse(global).is_some() || CTOR_VALUES.contains(&global))
            && OBJECT_PROTO_PROPS.contains(&prop))
}

/// Names that gate native built-in code (T25): `keys`/`entries` are linked only when the user's
/// code names them; `console.log` recognizes array iterators only when it can make one.
pub(crate) const GATED: &[&str] = &[
    "keys",
    "entries",
    "values",
    "iterator",
    "toPrimitive",
    "hasInstance",
    "toStringTag",
    "any",
    "withResolvers",
];

/// The built-in constructors that can be values (T25): their prelude function (`$$Array`) is what
/// calling and constructing them runs.
pub(crate) const CTOR_VALUES: &[&str] = &[
    "Object",
    "String",
    "Number",
    "Boolean",
    "Array",
    "Map",
    "Set",
    "ArrayBuffer",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "RegExp",
    "Date",
    "BigInt",
];

/// Constructors whose prelude function `$$X` is what `new X(…)` runs and that throw when called
/// without `new` (T25: `Map`, `Set`; T20: `ArrayBuffer` and the typed arrays).
pub(crate) const NEW_ONLY: &[&str] = &[
    "Map",
    "Set",
    "ArrayBuffer",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
];

impl Prelude {
    /// `extra`: prelude names needed by what the lowering emits (not visible in the AST).
    pub fn new(p: &Program, extra: &[&str]) -> Prelude {
        let mut out = Prelude::default();
        // Top-level prelude declarations in source order: functions and (state) variables.
        let mut items: Vec<Span> = Vec::new();
        let mut item_of: HashMap<SymbolId, usize> = HashMap::new();
        for stmt in &p.ast.body {
            if !p.in_prelude(stmt.span().start) {
                continue;
            }
            let mut add = |name: &str, sym: Option<SymbolId>| {
                if let Some(sym) = sym {
                    item_of.insert(sym, items.len());
                    out.names.insert(name.to_string(), sym);
                }
            };
            match stmt {
                Statement::FunctionDeclaration(f) => {
                    if let Some(id) = &f.id {
                        add(&id.name, id.symbol_id.get());
                    }
                }
                Statement::VariableDeclaration(d) => {
                    for decl in &d.declarations {
                        if let BindingPattern::BindingIdentifier(b) = &decl.id {
                            add(&b.name, b.symbol_id.get());
                        }
                    }
                }
                _ => {}
            }
            items.push(stmt.span());
        }
        let scoping = p.semantic.scoping();
        let mut roots: Vec<usize> = extra
            .iter()
            .filter_map(|n| out.names.get(*n))
            .map(|s| item_of[s])
            .collect();
        // Can the user's code make a BigInt (T40): a BigInt literal or the global `BigInt`?
        out.bigint = p.semantic.nodes().iter().any(|node| match node.kind() {
            AstKind::BigIntLiteral(b) => !p.in_prelude(b.span.start),
            AstKind::IdentifierReference(r) => {
                r.name == "BigInt"
                    && !p.in_prelude(r.span.start)
                    && scoping.get_reference(r.reference_id()).symbol_id().is_none()
            }
            _ => false,
        });
        // Prototype methods by property name: `slice` → `$$String_prototype_slice`, ….
        let mut methods: HashMap<String, Vec<usize>> = HashMap::new();
        for (name, sym) in &out.names {
            if let Some((b, m)) = name.split_once("_prototype_") {
                // `BigInt.prototype`'s only where there can be BigInts (T40).
                if b == "$$BigInt" && !out.bigint {
                    continue;
                }
                // `Date.prototype[Symbol.toPrimitive]` (T26) is reached by naming `toPrimitive`.
                let m = if m == "$toPrimitive" { "toPrimitive" } else { m };
                methods.entry(m.to_string()).or_default().push(item_of[sym]);
            }
        }
        for (prop, name) in ALIASES {
            if let Some(sym) = out.names.get(*name) {
                methods
                    .entry(prop.to_string())
                    .or_default()
                    .push(item_of[sym]);
            }
        }
        let mut user_props: HashSet<String> = HashSet::new();
        let mut deps: HashMap<usize, Vec<usize>> = HashMap::new();
        for (from, tos) in IMPLIED {
            let Some(from) = out.names.get(*from) else {
                continue;
            };
            for to in *tos {
                if let Some(to) = out.names.get(*to) {
                    deps.entry(item_of[from]).or_default().push(item_of[to]);
                }
            }
        }
        let mut add = |target: usize, span: Span, roots: &mut Vec<usize>| {
            if !p.in_prelude(span.start) {
                roots.push(target);
                return;
            }
            // The prelude item containing it.
            let i = items.partition_point(|s| s.start <= span.start);
            if i > 0 && span.end <= items[i - 1].end {
                deps.entry(i - 1).or_default().push(target);
            }
        };
        for node in p.semantic.nodes().iter() {
            // `yield*` in an async generator (T29): the lowering calls the delegation helpers.
            if let AstKind::YieldExpression(y) = node.kind()
                && y.delegate
                && p.semantic
                    .nodes()
                    .ancestors(node.id())
                    .find_map(|a| match a.kind() {
                        AstKind::Function(f) => Some(f.r#async && f.generator),
                        AstKind::ArrowFunctionExpression(_) => Some(false),
                        _ => None,
                    })
                    .unwrap_or(false)
            {
                for name in ASYNC_DELEGATE {
                    if let Some(sym) = out.names.get(name) {
                        add(item_of[sym], y.span, &mut roots);
                    }
                }
            }
            // A regex literal (T27).
            if let AstKind::RegExpLiteral(r) = node.kind()
                && let Some(sym) = out.names.get(REGEXP_CREATE)
            {
                add(item_of[sym], r.span, &mut roots);
            }
            // A string in the user's code may name a static read with a computed key
            // (`Math[k]`, T25)…
            if let AstKind::StringLiteral(s) = node.kind()
                && !p.in_prelude(s.span.start)
            {
                user_props.insert(s.value.to_string());
                // `console.log` format strings (T25).
                if s.value.contains('%')
                    && let Some(sym) = out.names.get("$$consoleFormat")
                {
                    roots.push(item_of[sym]);
                }
                // …or a prototype method (`s[m]()` with `m = "slice"`).
                for &t in methods.get(s.value.as_str()).into_iter().flatten() {
                    roots.push(t);
                }
            }
            // A property name: the prototype methods it may read.
            let prop = match node.kind() {
                // `Symbol.match` in the prelude is a well-known symbol, not a use of `match` (T27).
                AstKind::StaticMemberExpression(m)
                    if p.in_prelude(m.span.start)
                        && matches!(&m.object, Expression::Identifier(id) if id.name == "Symbol") =>
                {
                    None
                }
                AstKind::StaticMemberExpression(m) => Some((m.property.name.as_str(), m.span)),
                AstKind::ComputedMemberExpression(m) => match &m.expression {
                    Expression::StringLiteral(k) => Some((k.value.as_str(), m.span)),
                    _ => None,
                },
                _ => None,
            };
            if let Some((name, span)) = prop {
                if !p.in_prelude(span.start) {
                    out.bind |= name == "bind";
                    out.to_string |= name == "toString";
                    out.constructor |= name == "constructor";
                    user_props.insert(name.to_string());
                }
                for &t in methods.get(name).into_iter().flatten() {
                    add(t, span, &mut roots);
                }
            }
            let (target, span) = match node.kind() {
                AstKind::IdentifierReference(r) => {
                    let Some(sym) = scoping.get_reference(r.reference_id()).symbol_id() else {
                        // A global function (`parseInt` → `$$parseInt`); a built-in constructor
                        // only when called (`Array(3)`, not `Array.isArray`).
                        let called = || match p.semantic.nodes().parent_kind(node.id()) {
                            AstKind::CallExpression(c) => c.callee.span() == r.span,
                            AstKind::NewExpression(n) => n.callee.span() == r.span,
                            _ => false,
                        };
                        // A constructor as a value (T25): not called, not `Array.x`, not the
                        // right side of `instanceof`.
                        let value = || match p.semantic.nodes().parent_kind(node.id()) {
                            AstKind::StaticMemberExpression(m) => {
                                m.object.span() != r.span
                                    || inherited_prop(&r.name, &m.property.name)
                            }
                            // A spread call goes through the value.
                            AstKind::CallExpression(c) if c.callee.span() == r.span => {
                                c.arguments.iter().any(|a| a.is_spread())
                            }
                            AstKind::BinaryExpression(b) => {
                                b.operator != oxc_syntax::operator::BinaryOperator::Instanceof
                                    || b.right.span() != r.span
                            }
                            // `new String(…)`: through the value (wrapper objects trap, F88).
                            AstKind::NewExpression(n) if n.callee.span() == r.span => {
                                matches!(r.name.as_str(), "String" | "Number" | "Boolean")
                            }
                            _ => !called(),
                        };
                        if (CTOR_VALUES.contains(&r.name.as_str())
                            || js2wasi3_ir::Namespace::parse(&r.name).is_some())
                            && !p.in_prelude(r.span.start)
                            && value()
                        {
                            out.ctor_values.insert(r.name.to_string());
                        }
                        if (js2wasi3_ir::Builtin::parse(&r.name).is_none() || called())
                            && let Some(sym) = out.names.get(&format!("$${}", r.name))
                        {
                            add(item_of[sym], r.span, &mut roots);
                        }
                        continue;
                    };
                    let Some(&item) = item_of.get(&sym) else {
                        continue;
                    };
                    if p.is_generated(r.span.start) {
                        // The module linker's code (T41: namespace objects).
                        roots.push(item);
                        continue;
                    }
                    if !p.in_prelude(r.span.start) {
                        out.user_refs.push((r.span, r.name.to_string()));
                        continue;
                    }
                    (item, r.span)
                }
                AstKind::StaticMemberExpression(m) => {
                    let Some(mut name) = static_name(p, &m.object, &m.property.name) else {
                        continue;
                    };
                    // Read as a value, not called: `$$A_b$value` if there is one (T28).
                    let called = matches!(p.semantic.nodes().parent_kind(node.id()),
                        AstKind::CallExpression(c) if c.callee.span() == m.span);
                    let value = format!("{name}{VALUE}");
                    if !called && out.names.contains_key(&value) {
                        name = value;
                    }
                    let Some(sym) = out.names.get(&name) else {
                        continue;
                    };
                    (item_of[sym], m.span)
                }
                _ => continue,
            };
            add(target, span, &mut roots);
        }
        // The runtime compares strings with BigInts through the prelude's StringToBigInt, and
        // ToPrimitive of a BigInt wrapper reaches `valueOf`/`toString` (T40).
        if out.bigint {
            for name in [
                STRING_TO_BIGINT,
                "$$BigInt_prototype_valueOf",
                "$$BigInt_prototype_toString",
            ] {
                if let Some(sym) = out.names.get(name) {
                    roots.push(item_of[sym]);
                }
            }
        }
        // Constructors used as values: their function, and the statics the code names.
        for (name, sym) in &out.names {
            let Some(rest) = name.strip_prefix("$$") else {
                continue;
            };
            let (b, key) = rest.split_once('_').unwrap_or((rest, ""));
            let key = key.strip_suffix(VALUE).unwrap_or(key);
            if out.ctor_values.contains(b)
                && !key.starts_with("prototype")
                && (key.is_empty() || user_props.contains(key))
            {
                roots.push(item_of[sym]);
            }
        }
        // `RegExp.prototype[Symbol.x]` (T27): reachable from every RegExp's creation when the
        // code names a method that uses it (not from the string methods' lookups, so string
        // `split`/`replace` don't link the engine).
        if let Some(create) = out.names.get(REGEXP_CREATE_ANY) {
            for (sym, props) in REGEXP_SYMBOL_METHODS {
                if props.iter().any(|p| user_props.contains(*p))
                    && let Some(m) = out.names.get(&format!("$$RegExp_prototype_${sym}"))
                {
                    deps.entry(item_of[create]).or_default().push(item_of[m]);
                }
            }
        }
        out.user_props = user_props;
        let mut live = vec![false; items.len()];
        while let Some(i) = roots.pop() {
            if !live[i] {
                live[i] = true;
                roots.extend(deps.get(&i).into_iter().flatten().copied());
            }
        }
        out.live = out
            .names
            .iter()
            .filter(|(_, s)| live[item_of[s]])
            .map(|(n, _)| n.clone())
            .collect();
        out.dead = items
            .iter()
            .zip(&live)
            .filter(|(_, l)| !**l)
            .map(|(s, _)| *s)
            .collect();
        out
    }

    /// Is the prelude declaration `name` needed (so lowered)?
    pub fn is_live(&self, name: &str) -> bool {
        self.live.contains(name)
    }

    /// Does the prelude implement statics of the global `name` (`Math` → `$$Math_floor`, …)?
    pub fn has_statics(&self, name: &str) -> bool {
        let prefix = format!("$${name}_");
        self.names
            .keys()
            .any(|n| n.starts_with(&prefix) && !n[prefix.len()..].starts_with("prototype_"))
    }

    /// Is this offset inside a prelude declaration that isn't needed (so not lowered)?
    pub fn is_dead(&self, offset: u32) -> bool {
        let i = self.dead.partition_point(|s| s.start <= offset);
        i > 0 && offset < self.dead[i - 1].end
    }
}

/// The `length` of a built-in whose parameter list can't say it (a rest parameter).
pub(crate) fn length(name: &str) -> Option<u32> {
    match name {
        "$$Math_max" | "$$Math_min" | "$$Math_hypot" => Some(2),
        "$$Object_defineProperty$value" => Some(3),
        "$$Object_getOwnPropertyDescriptor$value" | "$$Object_hasOwn$value" => Some(2),
        "$$String"
        | "$$Number"
        | "$$String_fromCharCode"
        | "$$String_fromCodePoint"
        | "$$String_prototype_concat"
        | "$$String_prototype_padStart"
        | "$$String_prototype_padEnd"
        | "$$String_prototype_indexOf"
        | "$$String_prototype_lastIndexOf"
        | "$$String_prototype_includes"
        | "$$String_prototype_startsWith"
        | "$$String_prototype_endsWith"
        | "$$Array"
        | "$$Array_from"
        | "$$Array_prototype_concat"
        | "$$Array_prototype_unshift"
        | "$$Array_prototype_reduce"
        | "$$Array_prototype_reduceRight"
        | "$$Array_prototype_lastIndexOf"
        | "$$Array_prototype_forEach"
        | "$$Array_prototype_map"
        | "$$Array_prototype_filter"
        | "$$Array_prototype_some"
        | "$$Array_prototype_every"
        | "$$Array_prototype_find"
        | "$$Array_prototype_findIndex"
        | "$$Array_prototype_findLast"
        | "$$Array_prototype_findLastIndex"
        | "$$Array_prototype_flatMap"
        | "$$Array_prototype_indexOf"
        | "$$Array_prototype_includes"
        | "$$Array_prototype_fill"
        | "$$Map_prototype_forEach"
        | "$$Set_prototype_forEach" => Some(1),
        "$$Object_assign"
        | "$$Array_prototype_copyWithin"
        | "$$Array_prototype_splice"
        | "$$Array_prototype_toSpliced" => Some(2),
        "$$Array_prototype_flat" | "$$Map" | "$$Set" => Some(0),
        "$$Date" | "$$Date_UTC" => Some(7),
        "$$Date_prototype_toLocaleString"
        | "$$Date_prototype_toLocaleDateString"
        | "$$Date_prototype_toLocaleTimeString" => Some(0),
        _ => None,
    }
}

/// `A.b` with `A` an undeclared global: the prelude name `$$A_b` that would implement it.
/// The suffix of a prelude function that is a static read as a value when calls of it are
/// intrinsics (`$$Object_defineProperty$value`, T28).
pub(crate) const VALUE: &str = "$value";

pub(crate) fn static_name(p: &Program, object: &Expression, prop: &str) -> Option<String> {
    let Expression::Identifier(id) = object.without_parentheses() else {
        return None;
    };
    let global = p
        .semantic
        .scoping()
        .get_reference(id.reference_id())
        .symbol_id()
        .is_none();
    global.then(|| match (id.name.as_str(), prop) {
        // The same function objects as the globals.
        ("Number", "parseFloat" | "parseInt") => format!("$${prop}"),
        _ => format!("$${}_{}", id.name, prop),
    })
}
