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
    include_str!("../builtins/global.js"),
    include_str!("../builtins/dispose.js"),
    include_str!("../builtins/promise.js"),
    include_str!("../builtins/uri.js"),
    include_str!("../builtins/clone.js"),
    include_str!("../builtins/cjs.js"),
    include_str!("../builtins/node.js"),
    include_str!("../builtins/process.js"),
    include_str!("../builtins/base64.js"),
);

/// Prelude dependencies the AST doesn't show (T25): a `Map`/`Set` iterates through
/// `[Symbol.iterator]` (installed with `entries`/`values`) and its iterator's `next`, which the
/// user's code never names; a RegExp converts to a string through its `toString` (T27).
const IMPLIED: &[(&str, &[&str])] = &[
    // The event loop calls it when the program ends (T55).
    ("$$Process", &["$$processAtExit"]),
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

/// `console` as a value (T53).
pub(crate) const CONSOLE: &str = "$$Console";

/// The `process` and `performance` globals (T55), and `process`'s at-exit hook.
pub(crate) const PROCESS: &str = "$$Process";
pub(crate) const PERFORMANCE: &str = "$$Performance";
pub(crate) const PROCESS_AT_EXIT: &str = "$$processAtExit";

/// `util.inspect.custom` methods (T56): the formatter calls `$$inspectCustom(v, depth)` first.
pub(crate) const INSPECT_CUSTOM: &str = "$$inspectCustom";

/// Is the reference `process` (node `id`) the object of `process.nextTick` (T25: lowered without
/// the `process` object)?
fn next_tick(p: &Program, id: oxc_semantic::NodeId) -> bool {
    matches!(p.semantic.nodes().parent_kind(id), AstKind::StaticMemberExpression(m)
        if !m.optional && m.property.name == "nextTick")
}

/// `console._stdout`/`_stderr` (T63, winston): node's streams, `process.stdout`/`stderr`.
pub(crate) const CONSOLE_STREAMS: &[&str] = &["_stdout", "_stderr"];

/// The `console` methods lowered directly when called (T53): `Some(stderr)`.
pub(crate) fn console_stream(method: &str) -> Option<bool> {
    match method {
        "log" | "info" | "debug" => Some(false),
        "error" | "warn" => Some(true),
        _ => None,
    }
}

/// Is the reference `console` (node `id`) the object of a direct call `console.log(…)` (T53: no
/// spread, not optional), which the lowering writes without the `console` object?
fn console_direct(p: &Program, id: oxc_semantic::NodeId) -> bool {
    let nodes = p.semantic.nodes();
    let AstKind::StaticMemberExpression(m) = nodes.parent_kind(id) else {
        return false;
    };
    let member = nodes.parent_id(id);
    matches!(nodes.parent_kind(member), AstKind::CallExpression(c)
        if c.callee.span() == m.span
            && !m.optional
            && !c.optional
            && !c.arguments.iter().any(|a| a.is_spread())
            && console_stream(&m.property.name).is_some())
}

/// The prelude's StringToNumber: the runtime's ToNumber of a string calls it.
pub(crate) const STRING_TO_NUMBER: &str = "$$StringToNumber";

/// The prelude's StringToBigInt (T40): the runtime's comparisons of strings with BigInts call it.
pub(crate) const STRING_TO_BIGINT: &str = "$$StringToBigInt";
/// The prelude's ToBigInt (T48): the runtime's element writes of the BigInt arrays call it.
pub(crate) const TO_BIGINT: &str = "$$toBigInt";

/// The prelude's stream reader and pump (T20 slice 6b): the backend lifts and lowers a user
/// world's streams with them; T59: the blocking ones of a command's WASI imports.
pub(crate) const STREAMS: [&str; 5] = [
    "$$StreamReader",
    "$$StreamPump",
    "$$StreamReadAll",
    "$$StreamDefer",
    "$$StreamFlush",
];

/// GetTemplateObject (T51, `builtins/string.js`): what a tagged template passes its tag.
pub(crate) const TEMPLATE_OBJECT: &str = "$$templateObject";

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
    options: &frontend::Options,
    host: &mut dyn frontend::Host,
) -> Result<Program<'a>, Diagnostics> {
    let linked = frontend::link(alloc, path, source, options, host)?;
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
    /// `console.x` for a method node has that isn't implemented (T53: `console.table`): errors,
    /// unless the code changes `console` (its own members).
    pub console_members: Vec<(Span, String)>,
    /// Does the user's code change the `console` object (T53: `console.log = f`,
    /// `Object.assign(console, …)`)? Then every `console.x(…)` goes through the object.
    pub console_patched: bool,
    /// The spans of the prelude functions not needed, in source order.
    dead: Vec<Span>,
    /// The names of the needed top-level declarations.
    live: HashSet<String>,
    /// Does the user's code name a property `bind` (T25: `Function.prototype.bind` is installed
    /// only then)?
    pub bind: bool,
    /// Does the user's code (not node's built-in modules, T55) name a property `toString` (T25:
    /// functions keep their source text for `Function.prototype.toString` only then) or
    /// `structuredClone` (T53: its error shows the function)?
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
    /// Does the user's code name `SuppressedError` (T45; `using` code does)?
    pub suppressed: bool,
    /// Does it name `DisposableStack` or `AsyncDisposableStack` (T45)?
    pub stacks: bool,
    /// Does the user's source hold a RegExp modifier group, `(?i`/`(?m`/`(?s`/`(?-` (T46)? The
    /// prelude's `if (__intrinsic_regexp_mods()) { … }` blocks are kept only then.
    pub regexp_mods: bool,
    /// Does the user's source hold `maxByteLength` or `transfer` (T47: it can make a resizable
    /// `ArrayBuffer` or detach one)? Then `new ArrayBuffer` is `$$ArrayBuffer$resizable` and the
    /// prelude's `if (__intrinsic_rab_mode()) { … }` blocks are kept.
    pub rab: bool,
    /// Does the program have CommonJS modules (T54: the linker's generated code calls `$$cjs…`,
    /// which user code can't name)? The prelude's `if (__intrinsic_cjs_mode()) { … }` blocks are
    /// kept only then.
    pub cjs: bool,
    /// Is the entry CommonJS (T60: the linker's `$$cjsMain(…)` call; `ir::Module::cjs_main`)?
    pub cjs_main: bool,
    /// Does the user's code have a class with `extends`, or name `construct` (T47, see
    /// `ir::Module::subclassing`)?
    pub subclassing: bool,
    /// Does the user's code name `BigInt64Array` or `BigUint64Array` (T48)? Then [`Prelude::bigint`]
    /// too.
    pub big_typed: bool,
    /// Does the user's code name `globalThis` or `global` (T42: the global object exists)?
    pub global_object: bool,
    /// Does the user's code name the global `Promise` (T50: the statics it names are linked for
    /// the `Promise` value)?
    pub promise_value: bool,
    /// The global object's built-in properties (T42): the [`GLOBAL_NAMES`] the code names.
    pub global_props: Vec<String>,
    /// Writes that replace a built-in global (T42: `globalThis.Array = …`, `delete global.Map`,
    /// `Object.defineProperty(globalThis, "Math", …)`): rejected, as reads of the name stay static.
    pub replaced: Vec<(Span, String)>,
}

/// The prelude functions of the global object (T42).
pub(crate) const GLOBAL_FNS: &[&str] = &["$$GlobalObject", "$$GlobalGet", "$$GlobalRead", "$$GlobalSet"];

/// The implemented built-in globals that can be values, so global-object properties (T42), in
/// node's order where it has one. `NaN`, `Infinity`, `undefined` are added by the prelude.
pub(crate) const GLOBAL_NAMES: &[&str] = &[
    "Object", "Function", "Array", "Number", "parseFloat", "parseInt", "Boolean", "String",
    "Symbol", "Date", "Promise", "RegExp", "Error", "AggregateError", "EvalError", "RangeError",
    "ReferenceError", "SyntaxError", "TypeError", "URIError", "globalThis", "JSON", "Math",
    "ArrayBuffer", "Uint8Array", "Int8Array", "Uint16Array", "Int16Array", "Uint32Array",
    "Int32Array", "Float32Array", "Float64Array", "Uint8ClampedArray", "BigInt", "Map", "Set",
    "Reflect", "decodeURI", "decodeURIComponent", "encodeURI", "encodeURIComponent", "escape",
    "unescape", "isFinite", "isNaN", "console", "global", "clearImmediate", "setImmediate",
    "clearInterval", "clearTimeout", "setInterval", "setTimeout", "queueMicrotask",
    "structuredClone", "eval",
    // T61: node's `process` (npm's `process` package is `module.exports = global.process`).
    "process",
    "SuppressedError", "DisposableStack", "AsyncDisposableStack", "BigInt64Array",
    "BigUint64Array",
];

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
/// constructor value (`Date.hasOwnProperty(…)`; T68: `Error.isPrototypeOf(…)`, `Error` has a static)?
pub(crate) fn inherited_prop(global: &str, prop: &str) -> bool {
    FUNCTION_PROPS.contains(&prop)
        || ((js2wasi3_ir::Namespace::parse(global).is_some()
            || CTOR_VALUES.contains(&global)
            || global == "Error")
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
    // T45: `Symbol.dispose`/`asyncDispose` (the well-known symbols' array has their slots then).
    "dispose",
    "asyncDispose",
    // T63: errors' `stack` is made on its first read (else it stays a trapping sentinel).
    "stack",
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
    // T42: calling it throws (code known at build time is compiled where `Function` is called).
    "Function",
    // T45.
    "DisposableStack",
    "AsyncDisposableStack",
    // T48.
    "BigInt64Array",
    "BigUint64Array",
    // T49: calls by name are `Intrinsic::NewSymbol`; the value's call entry is `$$Symbol`.
    "Symbol",
];

/// The `ArrayBuffer` constructor taking `options` (T47): `$$ArrayBuffer` in a module with
/// [`Prelude::rab`], so other programs keep the one-parameter function.
pub(crate) const RESIZABLE_BUFFER: &str = "$$ArrayBuffer$resizable";

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
    // T45.
    "DisposableStack",
    "AsyncDisposableStack",
    // T48.
    "BigInt64Array",
    "BigUint64Array",
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
        // Resizable or detached buffers (T47): decided on the source text, like T46's modifiers.
        let user = &p.source[..p.user_end as usize];
        out.rab = user.contains("maxByteLength") || user.contains("transfer");
        if out.rab
            && let Some(&sym) = out.names.get(RESIZABLE_BUFFER)
        {
            out.names.insert("$$ArrayBuffer".to_string(), sym);
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
        // The BigInt arrays (T48): they make BigInts.
        out.big_typed = p.semantic.nodes().iter().any(|node| match node.kind() {
            AstKind::IdentifierReference(r) => {
                matches!(r.name.as_str(), "BigInt64Array" | "BigUint64Array")
                    && !p.in_prelude(r.span.start)
                    && scoping.get_reference(r.reference_id()).symbol_id().is_none()
            }
            _ => false,
        });
        out.bigint |= out.big_typed;
        // A RegExp modifier group anywhere in the user's source (T46).
        out.regexp_mods = p.source[..p.user_end as usize]
            .as_bytes()
            .windows(3)
            .any(|w| w[0] == b'(' && w[1] == b'?' && matches!(w[2], b'i' | b'm' | b's' | b'-'));
        out.cjs = p.source[..p.user_end as usize].contains("$$cjs");
        out.cjs_main = p.source[..p.user_end as usize].contains("$$cjsMain(");
        // Prelude code in `if (__intrinsic_regexp_mods()) { … }` blocks, when they're dropped.
        // T47/T48: the same for `if (__intrinsic_rab_mode()) { … }`, `__intrinsic_big_typed_mode()`.
        let mut dropped: Vec<Span> = Vec::new();
        let gates: Vec<&str> = [
            ("__intrinsic_regexp_mods", out.regexp_mods),
            ("__intrinsic_rab_mode", out.rab),
            ("__intrinsic_big_typed_mode", out.big_typed),
            ("__intrinsic_cjs_mode", out.cjs),
        ]
            .into_iter()
            .filter(|(_, keep)| !keep)
            .map(|(name, _)| name)
            .collect();
        if !gates.is_empty() {
            for node in p.semantic.nodes().iter() {
                if let AstKind::IfStatement(i) = node.kind()
                    && p.in_prelude(i.span.start)
                    && matches!(&i.test, Expression::CallExpression(c)
                        if matches!(&c.callee, Expression::Identifier(id) if gates.contains(&id.name.as_str())))
                {
                    dropped.push(i.span);
                }
            }
        }
        // Subclasses of built-ins, `Reflect.construct` (T47).
        out.subclassing = p.semantic.nodes().iter().any(|node| match node.kind() {
            AstKind::Class(c) => c.heritage.is_some() && !p.in_prelude(c.span.start),
            AstKind::IdentifierName(n) => n.name == "construct" && !p.in_prelude(n.span.start),
            AstKind::StringLiteral(l) => l.value == "construct" && !p.in_prelude(l.span.start),
            _ => false,
        });
        // Can the user's code make a SuppressedError (T45)?
        out.suppressed = p.semantic.nodes().iter().any(|node| match node.kind() {
            AstKind::IdentifierReference(r) => {
                r.name == "SuppressedError"
                    && !p.in_prelude(r.span.start)
                    && scoping.get_reference(r.reference_id()).symbol_id().is_none()
            }
            _ => false,
        });
        // T53: writes to `console`'s properties (not through an alias).
        let is_console = |e: &Expression| {
            matches!(e.without_parentheses(), Expression::Identifier(id)
                if id.name == "console"
                    && !p.in_prelude(id.span.start)
                    && scoping.get_reference(id.reference_id()).symbol_id().is_none())
        };
        out.console_patched = p.semantic.nodes().iter().any(|node| match node.kind() {
            AstKind::AssignmentExpression(a) => a.left.as_member_expression().is_some_and(|m| is_console(m.object())),
            AstKind::UpdateExpression(u) => u.argument.as_member_expression().is_some_and(|m| is_console(m.object())),
            AstKind::UnaryExpression(u) if u.operator == oxc_syntax::operator::UnaryOperator::Delete => {
                u.argument.as_member_expression().is_some_and(|m| is_console(m.object()))
            }
            AstKind::CallExpression(c) => {
                c.callee.as_member_expression().is_some_and(|m| {
                    matches!(
                        m.static_property_name(),
                        Some("defineProperty" | "defineProperties" | "assign" | "setPrototypeOf" | "set")
                    )
                }) && c.arguments.first().and_then(|a| a.as_expression()).is_some_and(is_console)
            }
            _ => false,
        });
        out.promise_value = p.semantic.nodes().iter().any(|node| match node.kind() {
            AstKind::IdentifierReference(r) => {
                r.name == "Promise"
                    && !p.in_prelude(r.span.start)
                    && scoping.get_reference(r.reference_id()).symbol_id().is_none()
            }
            _ => false,
        });
        out.stacks = p.semantic.nodes().iter().any(|node| match node.kind() {
            AstKind::IdentifierReference(r) => {
                matches!(r.name.as_str(), "DisposableStack" | "AsyncDisposableStack")
                    && !p.in_prelude(r.span.start)
                    && scoping.get_reference(r.reference_id()).symbol_id().is_none()
            }
            _ => false,
        });
        // Does the user's code name the global object (T42)?
        out.global_object = p.semantic.nodes().iter().any(|node| match node.kind() {
            AstKind::IdentifierReference(r) => {
                matches!(r.name.as_str(), "globalThis" | "global")
                    && !p.in_prelude(r.span.start)
                    && scoping.get_reference(r.reference_id()).symbol_id().is_none()
            }
            _ => false,
        });
        if out.global_object {
            roots.extend(GLOBAL_FNS.iter().filter_map(|n| out.names.get(*n)).map(|s| item_of[s]));
        }
        // Prototype methods by property name: `slice` → `$$String_prototype_slice`, ….
        let mut methods: HashMap<String, Vec<usize>> = HashMap::new();
        for (name, sym) in &out.names {
            if let Some((b, m)) = name.split_once("_prototype_") {
                // `BigInt.prototype`'s only where there can be BigInts (T40).
                if b == "$$BigInt" && !out.bigint {
                    continue;
                }
                // The stacks' only where there are stacks (T49: their prototypes don't exist
                // otherwise, so naming `dispose` alone failed to build).
                if matches!(b, "$$DisposableStack" | "$$AsyncDisposableStack") && !out.stacks {
                    continue;
                }
                // `Date.prototype[Symbol.toPrimitive]` (T26) is reached by naming `toPrimitive`.
                // T45: `[Symbol.dispose]`/`[Symbol.asyncDispose]` by naming `dispose`/`asyncDispose`.
                // T60: `Function.prototype[Symbol.hasInstance]` by naming `hasInstance`.
                // T63: `get %TypedArray%.prototype[Symbol.toStringTag]` by naming `toStringTag`.
                let m = match m {
                    "$toPrimitive" | "$dispose" | "$asyncDispose" | "$hasInstance" | "$toStringTag" => &m[1..],
                    _ => m,
                };
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
            if dropped.iter().any(|d| d.start <= node.kind().span().start && node.kind().span().end <= d.end) {
                continue;
            }
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
                // `util.inspect.custom` (T56): the formatter asks the prelude first.
                if s.value == "nodejs.util.inspect.custom"
                    && let Some(sym) = out.names.get(INSPECT_CUSTOM)
                {
                    roots.push(item_of[sym]);
                }
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
                    // T55: not node's built-in modules (`Object.prototype.toString.call(v)` in
                    // `util` would keep every function's source text, whole CommonJS modules).
                    out.to_string |= name == "toString" && !p.is_generated(span.start);
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
                                // T49: `new Symbol()` throws through the value's construct entry.
                                matches!(r.name.as_str(), "String" | "Number" | "Boolean" | "Symbol")
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
                        // T53: a function that `structuredClone` rejects shows its source.
                        out.to_string |= r.name == "structuredClone" && !p.in_prelude(r.span.start);
                        // T53: node's other `console` methods are build errors, not TypeErrors.
                        if r.name == "console"
                            && !out.console_patched
                            && !p.in_prelude(r.span.start)
                            && let AstKind::StaticMemberExpression(m) = p.semantic.nodes().parent_kind(node.id())
                            && m.object.span() == r.span
                            && console_stream(&m.property.name).is_none()
                            && m.property.name != "assert"
                            && !CONSOLE_STREAMS.contains(&m.property.name.as_str())
                            && !OBJECT_PROTO_PROPS.contains(&m.property.name.as_str())
                        {
                            out.console_members.push((m.span, m.property.name.to_string()));
                        }
                        // T53: `console` other than `console.log(…)` without spread is `$$Console()`.
                        if r.name == "console"
                            && !p.in_prelude(r.span.start)
                            && (out.console_patched || !console_direct(p, node.id()))
                            && let Some(sym) = out.names.get(CONSOLE)
                        {
                            add(item_of[sym], r.span, &mut roots);
                        }
                        // T55: `process` other than `process.nextTick` is `$$Process()`,
                        // `performance` is `$$Performance()`.
                        // T63: `console._stdout`/`_stderr` are `process.stdout`/`stderr`.
                        let streams = r.name == "console"
                            && matches!(p.semantic.nodes().parent_kind(node.id()),
                                AstKind::StaticMemberExpression(m) if m.object.span() == r.span
                                    && CONSOLE_STREAMS.contains(&m.property.name.as_str()));
                        let made = match r.name.as_str() {
                            "process" if !next_tick(p, node.id()) => Some(PROCESS),
                            "console" if streams => Some(PROCESS),
                            "performance" => Some(PERFORMANCE),
                            _ => None,
                        };
                        if let Some(made) = made
                            && !p.in_prelude(r.span.start)
                            && let Some(sym) = out.names.get(made)
                        {
                            add(item_of[sym], r.span, &mut roots);
                        }
                        // `Symbol(…)` is `Intrinsic::NewSymbol` (T49: `$$Symbol` is its value's).
                        if (js2wasi3_ir::Builtin::parse(&r.name).is_none()
                            || (called() && r.name != "Symbol"))
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
                    // T55: `Object.create(proto, descriptors)` is a call of the value's function.
                    let called = matches!(p.semantic.nodes().parent_kind(node.id()),
                        AstKind::CallExpression(c) if c.callee.span() == m.span
                            && !(name == "$$Object_create" && c.arguments.len() > 1));
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
        // T48: element writes of the BigInt arrays call ToBigInt.
        if out.big_typed
            && let Some(sym) = out.names.get(TO_BIGINT)
        {
            roots.push(item_of[sym]);
        }
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
        // Writes replacing a built-in global (T42), with the object written as `globalThis`/`global`.
        if out.global_object {
            let is_global = |e: &Expression| {
                matches!(e.without_parentheses(), Expression::Identifier(id)
                    if matches!(id.name.as_str(), "globalThis" | "global")
                        && scoping.get_reference(id.reference_id()).symbol_id().is_none())
            };
            let member_key = |m: &oxc_ast::ast::MemberExpression| -> Option<String> {
                if !is_global(m.object()) {
                    return None;
                }
                match m {
                    oxc_ast::ast::MemberExpression::StaticMemberExpression(s) => Some(s.property.name.to_string()),
                    oxc_ast::ast::MemberExpression::ComputedMemberExpression(c) => match &c.expression {
                        Expression::StringLiteral(k) => Some(k.value.to_string()),
                        _ => None,
                    },
                    _ => None,
                }
            };
            for node in p.semantic.nodes().iter() {
                let hit = match node.kind() {
                    AstKind::AssignmentExpression(a) => a.left.as_member_expression().and_then(member_key),
                    AstKind::UpdateExpression(u) => u.argument.as_member_expression().and_then(member_key),
                    AstKind::UnaryExpression(u)
                        if u.operator == oxc_syntax::operator::UnaryOperator::Delete =>
                    {
                        u.argument.as_member_expression().and_then(member_key)
                    }
                    AstKind::CallExpression(c) => match (c.callee.as_member_expression(), c.arguments.first(), c.arguments.get(1)) {
                        (Some(m), Some(_), Some(oxc_ast::ast::Argument::StringLiteral(k)))
                            if m.static_property_name() == Some("defineProperty")
                                && c.arguments[0].as_expression().is_some_and(is_global) =>
                        {
                            Some(k.value.to_string())
                        }
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(name) = hit
                    && !p.in_prelude(node.kind().span().start)
                    && GLOBAL_NAMES.contains(&name.as_str())
                {
                    out.replaced.push((node.kind().span(), name));
                }
            }
        }
        // T61: every function's `constructor` (through `Function.prototype`; async and generator
        // functions' too, F256) is `Function` as a value: reading it trapped (readable-stream's
        // `Object.getPrototypeOf(async function () {}).constructor`).
        if out.constructor {
            out.ctor_values.insert("Function".to_string());
        }
        // The global object's properties (T42): the built-ins the code names, as values.
        if out.global_object {
            for name in GLOBAL_NAMES {
                if !user_props.contains(*name) {
                    continue;
                }
                // Built-ins whose runtime support is gated on naming them as identifiers aren't
                // properties when it's off (T60: `node:util` has "BigInt64Array" as a string).
                let off = match *name {
                    "BigInt" => !out.bigint,
                    "BigInt64Array" | "BigUint64Array" => !out.big_typed,
                    "SuppressedError" => !out.suppressed,
                    "DisposableStack" | "AsyncDisposableStack" => !out.stacks,
                    _ => false,
                };
                if off {
                    continue;
                }
                out.global_props.push(name.to_string());
                if CTOR_VALUES.contains(name) || js2wasi3_ir::Namespace::parse(name).is_some() {
                    out.ctor_values.insert(name.to_string());
                } else if let Some(sym) = out
                    .names
                    .get(&format!("$${name}"))
                    .or_else(|| (*name == "console").then(|| out.names.get(CONSOLE)).flatten())
                    .or_else(|| (*name == "process").then(|| out.names.get(PROCESS)).flatten())
                {
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
            // T50: the `Promise` value's statics the code names (also through `constructor` or
            // the global object).
            if b == "Promise"
                && !key.is_empty()
                && user_props.contains(key)
                && (out.promise_value || out.global_object || user_props.contains("constructor"))
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
        "$$ArrayBuffer" | "$$ArrayBuffer$resizable" => Some(1),
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
        "$$Array_prototype_flat" | "$$Map" | "$$Set" | "$$Symbol" => Some(0),
        "$$Function" => Some(1),
        "$$structuredClone" => Some(2),
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
