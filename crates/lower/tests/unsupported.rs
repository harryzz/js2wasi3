//! Each rule of the supported subset rejects instead of miscompiling.

use js2wasi3_frontend::Allocator;

fn errors(src: &str) -> Vec<String> {
    let alloc = Allocator::default();
    let program = js2wasi3_lower::parse(&alloc, "t.js", src).expect("valid JS");
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
        let program = js2wasi3_lower::parse(&alloc, "t.js", src).unwrap();
        if let Err(e) = js2wasi3_lower::lower(&program) {
            panic!("{src}\n{e}");
        }
    }
}

#[test]
fn arity_and_callee() {
    // T14 (F16): extra arguments are evaluated and dropped.
    lowers("function g(a) { return a; }\nexport function f() { return g(1, 2); }");
    // T36: `Reflect` is a namespace; `Proxy` isn't implemented.
    lowers("export function f() { return Reflect.ownKeys({}); }");
    assert_eq!(
        one("export function f() { return new Proxy({}, {}); }"),
        "1:34 unsupported: global `Proxy` (planned: T21)"
    );
}

/// T21: the built-ins prelude is invisible to user code.
#[test]
fn builtins_prelude() {
    lowers("export function f(x) { return Math.floor(x) + Math.max(x, 2) + Math.PI; }");
    assert_eq!(
        one("export function f() { return Math.nope(1); }"),
        "1:30 unsupported: `Math.nope` (planned: T21)"
    );
    // Prelude names and intrinsics aren't globals of the user's code.
    assert_eq!(
        one("export function f() { return $$Math_abs(1); }"),
        "1:30 unsupported: global `$$Math_abs` (planned: T21)"
    );
    assert_eq!(
        one("export function f() { return __intrinsic_floor(1); }"),
        "1:30 unsupported: global `__intrinsic_floor` (planned: T21)"
    );
    // `Math` as a value (T25 slice 4).
    lowers("export function f(x) { const m = Math; return m.floor(x) + m.PI; }");
    // `Math.random` imports `wasi:random`: commands only.
    assert_eq!(
        one("export function f() { return Math.random(); }"),
        "1:30 unsupported: `Math.random` in a module with exports (planned: T20)"
    );
    // Slice 2: strings and numbers. String → number is a runtime conversion now (F20).
    lowers("export function f(s) { return s.slice(1).length + parseInt('7') + ('5' * 2); }");
    lowers("export function f(n) { return String(n).padStart(3, '0').length + Number('1'); }");
    lowers(
        "export function f(n) { const p = parseFloat; return p('1.5') + Number.MAX_SAFE_INTEGER; }",
    );
    // `String`/`Number` as values (T25 slice 4; `new Number` lowers, its wrapper object traps).
    lowers("export function f() { const s = String; return s(1).length; }");
    lowers("export function f() { return new Number(1); }");
    // Slice 3: arrays. `Array` as a value and a spread `Array(...xs)` too since T25 slice 4.
    lowers("export function f(a) { return a.map((x) => x + 1).indexOf(2) + Array(3).length; }");
    lowers("export function f(n) { return new Array(n).fill(0).concat(Array.of(1)).length; }");
    lowers("export function f(xs) { const A = Array; return new A(...xs).length; }");
    assert_eq!(
        one("export function f() { const s = Symbol; return 1; }"),
        "1:33 unsupported: global `Symbol` (planned: T21)"
    );
    // Slice 4: JSON.stringify, Object statics and Object.prototype methods.
    lowers("export function f(o) { return JSON.stringify(o, null, 2).length; }");
    lowers(
        "export function f(o) { return Object.keys(Object.assign({}, o, Object.fromEntries([['a', 1]]))).length; }",
    );
    lowers(
        "export function f(o) { return o.hasOwnProperty('a') && Object.prototype.isPrototypeOf(o); }",
    );
    // T25 slice 1: JSON.parse.
    lowers("export function f(s) { return JSON.parse(s, (k, v) => v).length; }");
    assert_eq!(
        one("export function f(s) { return JSON.rawJSON(s); }"),
        "1:31 unsupported: `JSON.rawJSON` (planned: T21)"
    );
    lowers("export function f(v) { const j = JSON; return j.stringify(v).length; }");
    // T25 slice 2: Map and Set (subclassing not yet).
    lowers(
        "export function f(a) { const m = new Map([[1, a]]); m.set(2, 3); return new Set(m.keys()).size + m.get(1); }",
    );
    lowers("export function f(m) { return m instanceof Map && Map.prototype.has.call(m, 1); }");
    lowers("export function f(a) { return Map.groupBy(a, (x) => x % 2).size; }");
    // As values since T25 slice 4.
    lowers("export function f() { const M = Map; return new M().size; }");
}

#[test]
fn module_level_rules() {
    assert_eq!(
        one("import { x } from './x.js';"),
        "1:1 unsupported: import of another module (multi-file modules, F7) (planned: not scheduled)"
    );
    // T20 (F11): a library's top-level code is its `init`; `await` there needs async exports.
    lowers("export function f() { return a; }\nlet a = 1;\na = 2;");
    lowers("export function f() { return a; }\nlet a = 1 + 2;");
    assert_eq!(
        one("export function f() {}\nawait null;"),
        "2:1 unsupported: top-level `await` in a module with exports (planned: T20)"
    );
    assert_eq!(
        one("export function f() {}\nfor await (const x of []) {}"),
        "2:1 unsupported: top-level `await` in a module with exports (planned: T20)"
    );
    // Not in a nested function.
    lowers("export function f() { return 1; }\nconst g = async () => { await null; };\ng();");
    // Output and timers are still command-only.
    assert_eq!(
        one("export function f() {}\nconsole.log(1);"),
        "2:1 unsupported: `console` in a module with exports (a library can't write to stdout yet) (planned: T20)"
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
    // T20 slice 3: an async export is a WIT `async func`.
    lowers("export async function f() { await null; return 1; }");
    assert_eq!(
        one("/** @returns {Promise<number>} */\nexport function f() { return 1; }"),
        "2:8 unsupported: JSDoc return type `Promise<…>` on a function that isn't `async` (planned: T20)"
    );
    // T20 slice 1b: async generators; T29: `yield*` in them.
    lowers("async function* g() { yield 1; } g();");
    lowers("const g = async function* () { await null; return 2; }; g();");
    lowers("async function* g() { yield* [1]; } g();");
    assert_eq!(
        one("export function f() { setTimeout(f, 1); return 0; }"),
        "1:23 unsupported: `setTimeout` in a library without async exports (timers run while an async export's call waits) (planned: T20)"
    );
    // T20 slice 3: an async export's call runs the timers while it waits.
    lowers("export async function f() { setTimeout(f, 1); return 0; }");
    assert_eq!(
        one("export function f() { process.nextTick(f); return 0; }"),
        "1:23 unsupported: `nextTick` in a library without async exports (timers run while an async export's call waits) (planned: T20)"
    );
    assert_eq!(
        one("export function f() { setImmediate(f); return 0; }"),
        "1:23 unsupported: `setImmediate` in a library without async exports (timers run while an async export's call waits) (planned: T20)"
    );
    lowers("setImmediate(() => {}); process.nextTick(() => {});");
    lowers("async function f(a) { return (await a) + 1; } f(1);");
    lowers("await 1;");
    lowers("export function f(a = 1) { return a; }");
    assert_eq!(
        one("export function f(...a) { return 0; }"),
        "1:8 unsupported: rest parameter in an exported function (planned: T20)"
    );
    lowers("export function f() { return Reflect; }");
    assert_eq!(
        one("export function f() { return Proxy; }"),
        "1:30 unsupported: global `Proxy` (planned: T21)"
    );
    // A name that isn't a global in node either throws a ReferenceError when read (F139).
    lowers("export function f() { return x; }");
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
    let e = errors(
        "export function f() {\n  let s = Proxy;\n  new WeakMap();\n  return new WeakSet();\n}",
    );
    assert_eq!(
        e,
        vec![
            "2:11 unsupported: global `Proxy` (planned: T21)",
            "3:7 unsupported: global `WeakMap` (planned: T21)",
            "4:14 unsupported: global `WeakSet` (planned: T21)",
        ]
    );
}

fn lowers(src: &str) -> String {
    let alloc = Allocator::default();
    let program = js2wasi3_lower::parse(&alloc, "t.js", src).unwrap();
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
    // Format strings (T25 slice 5).
    lowers("console.log('%d items', 3);");
    assert_eq!(
        one("export function f() { console.log(1); }"),
        "1:23 unsupported: `console` in a module with exports (a library can't write to stdout yet) (planned: T20)"
    );
    assert_eq!(
        one("console.table(1);"),
        "1:1 unsupported: global `console` (planned: T21)"
    );
    // A top-level `await using` is reported, not a panic (F140).
    assert_eq!(
        one("await using x = null;"),
        "1:1 unsupported: `using` declaration (planned: not scheduled)"
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
        one("export function f() { return new WeakMap(); }"),
        "1:34 unsupported: global `WeakMap` (planned: T21)"
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
        one("export function f(o) { return o instanceof WeakMap; }"),
        "1:44 unsupported: global `WeakMap` (planned: T21)"
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
    // T20 slice 1a: `for await` (top level: `main` becomes async).
    lowers("for await (const x of []) {}");
    lowers("async function f(a) { for await (const [k, v] of a) { if (k) break; } } f([]);");
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
    // T25: `arguments` in ordinary functions and methods (T43: and arrows); not in class
    // constructors, with a rest parameter, or in an exported function (a WIT `list`, F55).
    lowers("export function f(a) { function g() { return arguments.length; } return g(a, 1); }");
    lowers("export function f(a) { return { m() { return arguments[0]; } }.m(a); }");
    assert_eq!(
        one("export function f() { return arguments.length; }"),
        "1:8 unsupported: `arguments` in an exported function (the WIT signature needs a `list`) (planned: T20)"
    );
    // T43: arrow functions use the enclosing function's.
    lowers("export function f(a) { function g() { return () => arguments; } return 1; }");
    assert_eq!(
        one("export function f(a) { function g(...r) { return arguments; } return 1; }"),
        "1:24 unsupported: a rest parameter with `arguments` (planned: later)"
    );
    assert_eq!(
        one("export function f() { function g() {} g = 1; return 0; }"),
        "1:39 unsupported: assignment to function `g` (planned: later)"
    );
    // F59: a function under a computed key is named at run time.
    lowers("export function f(k) { return { [k]: () => 1 }; }");
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
        "class A { get #x() { return 1; } set #x(v) {} static get #s() { return 2; } m() { [this.#x] = [this.#x + A.#s]; } }\nnew A().m();",
        "const k = 'a';\nclass A { [k] = 1; static [k + 1] = 2; }\nconsole.log(new A().a);",
        "class A { static #p = 1; static { var v = this.#p; this.f = () => v; } static g(o) { return o?.#p; } }\nconsole.log(A.g(A));",
        // T38: `new.target` in functions and arrows.
        "function f() { return () => new.target; }\nnew f();",
    ] {
        lowers(src);
    }
    for (src, err) in [
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
    // T20: a library's top-level class is set by its `init`.
    lowers("class A {}\nexport function f() { return new A() instanceof A ? 1 : 0; }");
}

/// T42: replacing a built-in through the global object is rejected (reads of the name stay
/// direct references to the built-in); other global-object writes are fine.
#[test]
fn global_object_replacement() {
    assert_eq!(
        errors("globalThis.Array = 1;\ndelete global.Map;\nObject.defineProperty(globalThis, \"Math\", {});\n"),
        [
            "1:1 unsupported: replacing or deleting the built-in global `Array` (planned: later)",
            "2:1 unsupported: replacing or deleting the built-in global `Map` (planned: later)",
            "3:1 unsupported: replacing or deleting the built-in global `Math` (planned: later)",
        ]
    );
    lowers("globalThis.mine = 1; globalThis[\"x\"] = 2; mine++; console.log(x, NaN);");
}
