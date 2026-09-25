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
        one("let a = 1;\na = 2;"),
        "2:1 unsupported: top-level ExpressionStatement (top-level code runs in the command world) (planned: T10)"
    );
    assert_eq!(
        one("let a = 1 + 2;"),
        "1:5 unsupported: top-level variable whose initializer isn't a literal (needs module init code) (planned: T10)"
    );
    assert_eq!(
        one("export let a = 1;"),
        "1:1 unsupported: exporting a variable (only functions can be exported) (planned: T20)"
    );
}

#[test]
fn declarations_and_functions() {
    assert_eq!(
        one("export function f(a) { return typeof a; }"),
        "1:31 unsupported: `typeof` used as a value (it's a string); only `typeof x === \"...\"` works so far (planned: T09)"
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
    let e = errors("export function f() {\n  let s = 'a';\n  delete 1;\n  return 1 ** 2;\n}");
    assert_eq!(
        e,
        vec![
            "2:11 unsupported: string (planned: T09)",
            "3:3 unsupported: operator `delete` (planned: T13)",
            "4:10 unsupported: operator `**` (planned: T21 (Math.pow))",
        ]
    );
}
