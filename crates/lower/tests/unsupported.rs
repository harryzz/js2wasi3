//! Each rule of the supported subset rejects instead of miscompiling.

use js2wasi3_frontend::{self as frontend, Allocator};

fn errors(src: &str) -> Vec<String> {
    let alloc = Allocator::default();
    let program = frontend::parse(&alloc, "t.js", src).expect("valid JS");
    match js2wasi3_lower::lower(&program) {
        Ok(m) => panic!("expected errors, got IR:\n{m}"),
        Err(d) => {
            d.0.iter()
                .map(|d| format!("{}:{} {}", d.line, d.col, d.message))
                .collect()
        }
    }
}

fn one(src: &str) -> String {
    let e = errors(src);
    assert_eq!(e.len(), 1, "{e:#?}");
    e.into_iter().next().unwrap()
}

/// T08 made these work (they were rejections in T05).
#[test]
fn primitive_values_lower() {
    for src in [
        "export function f(a) { if (a) { return 1; } else return 2; }",
        "export function f(a) { return a < 1; }",
        "export function f(a) { let b = !a; return b ? 1 : 0; }",
        "export function f(a) { return a++; }",
        "export function f(a) {\n  if (a) return 1;\n}",
        "export function f(a) { if (a) return 1; return; }",
        "function g(a, b) { return b; }\nexport function f() { return g(1); }",
        "function g() { }\nexport function f() { return g(); }",
        "export function f() { let x; var y; return x === y; }",
        "export function f() { let n = null; return n == undefined; }",
        "export function f(a) { return typeof a === 'number' && typeof null !== 'object'; }",
        "export function f(a) { return a >>> 0; }",
        "let g = true;\nexport function f() { return g = !g; }",
        "export function f(a) { return typeof a; }",
        "export function f(a) { return `a${a}b` + 'c'.length; }",
    ] {
        let alloc = Allocator::default();
        let program = frontend::parse(&alloc, "t.js", src).unwrap();
        if let Err(e) = js2wasi3_lower::lower(&program) {
            panic!("{src}\n{e}");
        }
    }
}

#[test]
fn arity_and_callee() {
    assert_eq!(
        one("function g(a) { return a; }\nexport function f() { return g(1, 2); }"),
        "2:30 unsupported: call with 2 argument(s) to a function with 1 parameter(s) (planned: later)"
    );
    assert_eq!(
        one("export function f() { return Math.abs(1); }"),
        "1:30 unsupported: call of something other than a named function (planned: T14)"
    );
}

#[test]
fn module_level_rules() {
    assert_eq!(
        one("import { x } from './x.js';"),
        "1:1 unsupported: import of another module (multi-file modules, F7) (planned: not scheduled)"
    );
    assert_eq!(
        one("export function f() {}\nlet a = 1;\na = 2;"),
        "3:1 unsupported: top-level ExpressionStatement in a module with exports (library init code, F11; a file without exports is a command and may have top-level code) (planned: T20)"
    );
    assert_eq!(
        one("export function f() {}\nlet a = 1 + 2;"),
        "2:5 unsupported: top-level variable whose initializer isn't a literal in a module with exports (library init code, F11) (planned: T20)"
    );
    assert_eq!(
        one("export let a = 1;"),
        "1:1 unsupported: exporting a variable (only functions can be exported) (planned: T20)"
    );
}

#[test]
fn declarations_and_functions() {
    assert_eq!(
        one("/** @param {boolean} a */\nexport function f(a) { return 1; }"),
        "2:8 unsupported: JSDoc type `{boolean}` of parameter `a` at the WIT boundary (only `string` and `number` so far) (planned: T20)"
    );
    assert_eq!(
        one("export function f(a) { a ||= 1; return a; }"),
        "1:24 unsupported: operator `||=` (planned: later)"
    );
    assert_eq!(
        one("export function f() { function g() {} return 0; }"),
        "1:23 unsupported: nested function (closures) (planned: T14)"
    );
    assert_eq!(
        one("export async function f() { }"),
        "1:8 unsupported: async function (planned: T19)"
    );
    assert_eq!(
        one("export function f(a = 1) { return a; }"),
        "1:19 unsupported: default or destructured parameter (planned: T15)"
    );
    assert_eq!(
        one("export function f() { return x; }"),
        "1:30 unsupported: global `x` (planned: T21)"
    );
}

#[test]
fn all_errors_are_reported() {
    let e = errors("export function f() {\n  let s = 'a' * 2;\n  [1];\n  return 1 ** 2;\n}");
    assert_eq!(
        e,
        vec![
            "2:11 unsupported: converting a string to a number (planned: T21)",
            "3:3 unsupported: array literal (planned: T15)",
            "4:10 unsupported: operator `**` (planned: T21 (Math.pow))",
        ]
    );
}

fn lowers(src: &str) -> String {
    let alloc = Allocator::default();
    let program = frontend::parse(&alloc, "t.js", src).unwrap();
    match js2wasi3_lower::lower(&program) {
        Ok(m) => m.to_string(),
        Err(e) => panic!("{src}\n{e}"),
    }
}

/// T10: a file without exports is a command; its top-level code runs in `main`.
#[test]
fn commands_lower() {
    for src in [
        "",
        "console.log();",
        "let a = 1;\na = 2;\nconsole.log(a);",
        "let a = 1 + 2; const s = `a${a}`; console.error(s, a, -0, null);",
        "function f() { return n; }\nlet n = 5;\nconsole.log(f());",
        "for (let i = 0; i < 3; i++) { if (i) console.info(i); }",
        "let u = console.log('x'); console.log(u === undefined);",
        "console.log('100%'); console.log(1, '%s'); console.log('a b', 1);",
        "import {} from './x.js';",
    ] {
        if src.starts_with("import") {
            assert!(errors(src)[0].contains("import of another module"));
            continue;
        }
        let ir = lowers(src);
        assert!(ir.contains("main = main#"), "{ir}");
    }
}

#[test]
fn command_globals() {
    // A literal initializer with no code before it is the initial value.
    let ir = lowers("let a = 1;\nconsole.log(a);");
    assert!(ir.contains("global @a: f64 mut = 1.0"), "{ir}");
    // A `var` that code before it could read starts as `undefined` (hoisting).
    let ir = lowers("console.log(v);\nvar v = 1;");
    assert!(ir.contains("global @v: jsval mut = undefined"), "{ir}");
    assert!(ir.contains("@v = f64->jsval(1.0)"), "{ir}");
    // A non-literal `var` initializer could call a function that reads it.
    let ir = lowers("var w = f();\nfunction f() { return w; }");
    assert!(ir.contains("global @w: jsval mut = undefined"), "{ir}");
}

#[test]
fn command_rules() {
    assert_eq!(
        one("console.log('%d items', 3);"),
        "1:13 unsupported: `console.log` format string (a first argument containing `%`, followed by more arguments) (planned: T21)"
    );
    assert_eq!(
        one("export function f() { console.log(1); }"),
        "1:23 unsupported: `console` in a module with exports (a library can't write to stdout yet) (planned: T20)"
    );
    assert_eq!(
        one("console.table(1);"),
        "1:1 unsupported: call of something other than a named function (planned: T14)"
    );
}

/// T12: exceptions lower; what's left for later is rejected.
#[test]
fn exceptions() {
    for src in [
        "export function f(a) { try { if (a) throw a; return 1; } catch (e) { return 2; } finally { a = 0; } }",
        "export function f() { try { throw new TypeError('x'); } catch { } return 0; }",
        "export function f(a) { return new Error(a).message.length; }",
        "export function f() { for (;;) { try { break; } finally { continue; } } }",
    ] {
        lowers(src);
    }
    // `main` reading a `let` before its declaration throws a ReferenceError (was a build error).
    let ir = lowers("console.log(x);\nlet x = 1;");
    assert!(
        ir.contains("throw $new_ReferenceError(\"Cannot access \\'x\\' before"),
        "{ir}"
    );
    // F22: a function called before a declaration checks an init flag; after it, no flag.
    let ir = lowers("function f() { return n; }\nconsole.log(f());\nlet n = 5;");
    assert!(ir.contains("global @n$init: bool mut = false"), "{ir}");
    let ir = lowers("function f() { return n; }\nlet n = 5;\nconsole.log(f());");
    assert!(!ir.contains("$init"), "{ir}");

    assert_eq!(
        one("export function f() { return new Map(); }"),
        "1:30 unsupported: `new` (only the native error constructors so far) (planned: T16)"
    );
    assert_eq!(
        one("export function f() { try { } catch ({ a }) { } }"),
        "1:38 unsupported: destructuring `catch` parameter (planned: T15)"
    );
}

/// T13: objects lower; what's left for later is rejected.
#[test]
fn objects() {
    for src in [
        "export function f(k) { const o = { a: 1, [k]: 2, ...{ b: 3 }, __proto__: null }; return o.a + o[k]; }",
        "export function f() { const o = {}; o.x = 1; o['y'] += 2; o.x++; delete o.x; return 'x' in o; }",
        "export function f(o) { let n = 0; for (const k in o) n++; return n; }",
        "export function f(o) { return o?.a?.[1] ?? Object.getPrototypeOf(Object.create(Error.prototype)); }",
        "export function f(e) { return e instanceof TypeError && Object.hasOwn(e, 'message'); }",
    ] {
        lowers(src);
    }
    assert_eq!(
        one("export function f() { return { get x() { return 1; } }; }"),
        "1:32 unsupported: getter or setter (planned: T16)"
    );
    assert_eq!(
        one("export function f() { return { m() {} }; }"),
        "1:32 unsupported: method (functions as values) (planned: T14)"
    );
    assert_eq!(
        one("export function f(o) { return o instanceof Map; }"),
        "1:44 unsupported: `instanceof` with something other than `Object` or a native constructor (planned: T14/T16)"
    );
    assert_eq!(
        one("export function f(o) { return o?.m(); }"),
        "1:31 unsupported: optional call `?.()` (planned: T14)"
    );
    assert_eq!(
        one("export function f(o) { return Object.create(o, {}); }"),
        "1:31 unsupported: `Object.create` with property descriptors (planned: not scheduled)"
    );
    assert_eq!(
        one("export function f(o) { for (o.k in o) {} }"),
        "1:29 unsupported: `for-in` target other than a variable (planned: later)"
    );
    assert_eq!(
        one("export function f(o) { return o.m(); }"),
        "1:31 unsupported: call of something other than a named function (planned: T14)"
    );
}
