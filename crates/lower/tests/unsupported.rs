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
    // T14 (F16): extra arguments are evaluated and dropped.
    lowers("function g(a) { return a; }\nexport function f() { return g(1, 2); }");
    assert_eq!(
        one("export function f() { return Math.abs(1); }"),
        "1:30 unsupported: global `Math` (planned: T21)"
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
    // T24: logical assignment, `**`, `switch`, `debugger`.
    lowers("export function f(a) { a ||= 1; a &&= 2; a ??= 3; return a ** 2; }");
    lowers(
        "export function f(a) { switch (a) { case 1: debugger; return 1; default: return 0; } }",
    );
    lowers("export function f() { function g() {} return 0; }");
    assert_eq!(
        one("export async function f() { }"),
        "1:8 unsupported: exported async function (the boundary needs an async export) (planned: T20)"
    );
    assert_eq!(
        one("async function* g() { }"),
        "1:1 unsupported: async generator function (planned: later)"
    );
    assert_eq!(
        one("const g = async function* () { };"),
        "1:11 unsupported: async generator function (planned: later)"
    );
    assert_eq!(
        one("export function f() { setTimeout(f, 1); return 0; }"),
        "1:23 unsupported: `setTimeout` in a module with exports (planned: T20)"
    );
    lowers("async function f(a) { return (await a) + 1; } f(1);");
    lowers("await 1;");
    lowers("export function f(a = 1) { return a; }");
    assert_eq!(
        one("export function f(...a) { return 0; }"),
        "1:8 unsupported: rest parameter in an exported function (planned: T20)"
    );
    assert_eq!(
        one("export function f() { return x; }"),
        "1:30 unsupported: global `x` (planned: T21)"
    );
    assert_eq!(
        one("export function* f() { yield 1; }"),
        "1:8 unsupported: exported generator function (the boundary needs an iterator type) (planned: T20)"
    );
    // A generator used inside a library is fine.
    lowers(
        "function* g() { yield 1; } export function f() { let n = 0; for (const x of g()) n += x; return n; }",
    );
}

#[test]
fn all_errors_are_reported() {
    let e = errors("export function f() {\n  let s = 'a' * 2;\n  new Map();\n  return /a/;\n}");
    assert_eq!(
        e,
        vec![
            "2:11 unsupported: converting a string to a number (planned: T21)",
            "3:7 unsupported: global `Map` (planned: T21)",
            "4:10 unsupported: expression `RegExpLiteral` (planned: later)",
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
        "1:1 unsupported: global `console` (planned: T21)"
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
        "1:34 unsupported: global `Map` (planned: T21)"
    );
    lowers("export function f() { try { } catch ({ a }) { } }");
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
        // T16: accessors, `instanceof` with any value.
        "export function f() { return { get x() { return 1; }, set x(v) {} }; }",
        "export function f(o, g) { return o instanceof g; }",
    ] {
        lowers(src);
    }
    assert_eq!(
        one("export function f(o) { return o instanceof Map; }"),
        "1:44 unsupported: global `Map` (planned: T21)"
    );
    assert_eq!(
        one("export function f(o) { return Object.create(o, {}); }"),
        "1:31 unsupported: `Object.create` with property descriptors (planned: not scheduled)"
    );
    lowers("export function f(o) { for (o.k in o) {} }");
}

/// T15: arrays, iteration, spread, destructuring lower.
#[test]
fn arrays_and_iteration() {
    for src in [
        "export function f(a) { const [x, , ...r] = [a, 2, 3, 4]; return x + r.length; }",
        "export function f(o) { let n = 0; for (const [k, v] of Object.entries(o)) n += v; return n; }",
        "export function f({ a = 1, ...rest } = {}, [b] = [2]) { return a + b; }",
        "export function f(a) { const s = Symbol('s'); const o = { [s]: a }; return o[s]; }",
        "export function f(a) { return Math2(...[a, 2]); } function Math2(x, y) { return x * y; }",
    ] {
        lowers(src);
    }
    assert_eq!(
        one("for await (const x of []) {}"),
        "1:1 unsupported: `for await` (planned: later)"
    );
    assert_eq!(
        one("export function f(a) { return [...a, , 1]; }"),
        "1:31 unsupported: array hole after a spread element (planned: later)"
    );
}

/// T14: closures, function values and calls lower; what's left for later is rejected.
#[test]
fn closures() {
    for src in [
        "export function f() { return { m() {} }; }",
        "export function f(o) { return o?.m() ?? o.m?.(); }",
        "export function f(o) { return o.m(1, 2); }",
        "export function f(n) { let c = 0; const inc = () => (c += n); inc(); return c; }",
        "export function f() { const fs = {}; for (let i = 0; i < 3; i++) fs[i] = () => i; return fs[1](); }",
        "export function f() { return (function g(n) { return n ? n * g(n - 1) : 1; })(5); }",
        "export function f() { function a(n) { return n ? b(n - 1) : 0; } function b(n) { return a(n); } return a(3); }",
        "export function f() { return this; }",
        "export function f(o) { return f.call(o) === f; }",
    ] {
        lowers(src);
    }
    assert_eq!(
        one("export function f() { return arguments; }"),
        "1:30 unsupported: `arguments` (planned: T21)"
    );
    assert_eq!(
        one("export function f() { function g() {} g = 1; return 0; }"),
        "1:39 unsupported: assignment to function `g` (planned: later)"
    );
    assert_eq!(
        one("export function f(k) { return { [k]: () => 1 }; }"),
        "1:33 unsupported: function with a computed key (its `name` comes from the key at run time) (planned: later)"
    );
}

/// T16: classes lower; what's left for later is rejected.
#[test]
fn classes() {
    for src in [
        "class A { x = 1; constructor(a) { this.a = a; } m() { return this.x; } static s() {} get g() { return 1; } set g(v) {} }\nconsole.log(new A(1).m());",
        "class A {}\nclass B extends A { constructor() { super(); } m() { return super.toString(); } }\nnew B();",
        "const C = class D { static n = D.name; constructor() { this.t = new.target; } };\nconsole.log(C.n);",
        "export function f(x) { class P { v = x; } return new P().v; }",
        "export function f(o) { return Object.isFrozen(Object.freeze(Object.defineProperty(o, 'k', { value: 1 }))); }",
        "export function f(F) { return new F(1, ...[2]); }",
        "class P { #x = 1; static #n = 0; #m() { return this.#x; } static has(o) { return #x in o; } static get(o) { return o.#m() + P.#n++; } }\nconsole.log(P.get(new P()));",
    ] {
        lowers(src);
    }
    for (src, err) in [
        (
            "class A { get #x() { return 1; } }",
            "1:11 unsupported: private getter or setter (planned: later)",
        ),
        (
            "class A { static { } }",
            "1:11 unsupported: class `static` block (planned: later)",
        ),
        (
            "const k = 'a';\nclass A { [k] = 1; }",
            "2:11 unsupported: computed instance field key (planned: later)",
        ),
        (
            "function f() { return new.target; }",
            "1:23 unsupported: `new.target` outside a class constructor (planned: later)",
        ),
        (
            "class A {}\nclass B extends A { constructor() { const f = () => super(); f(); } }",
            "2:53 unsupported: `super()` in an arrow function (planned: later)",
        ),
        (
            "export class A {}",
            "1:1 unsupported: exported declaration other than a function (planned: T20)",
        ),
    ] {
        assert_eq!(one(src), err, "{src}");
    }
    assert!(
        one("class A {}\nexport function f() { return 1; }")
            .starts_with("1:1 unsupported: top-level ClassDeclaration in a module with exports")
    );
}
