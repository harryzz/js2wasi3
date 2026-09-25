//! Each rule of the numbers-only subset rejects instead of miscompiling.

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

#[test]
fn boolean_as_value() {
    assert_eq!(
        one("export function f(a) { return a < 1; }"),
        "1:31 unsupported: boolean used as a value (planned: T08)"
    );
    assert_eq!(
        one("export function f(a) { let b = !a; return 0; }"),
        "1:32 unsupported: boolean used as a value (planned: T08)"
    );
}

#[test]
fn postfix_update_as_value() {
    assert_eq!(
        one("export function f(a) { return a++; }"),
        "1:31 unsupported: postfix `++`/`--` used as a value (planned: T08)"
    );
}

#[test]
fn falling_off_the_end() {
    assert_eq!(
        one("export function f(a) {\n  if (a) return 1;\n}"),
        "3:1 unsupported: function may reach its end without returning a value (implicit `undefined`) (planned: T08)"
    );
    assert_eq!(
        one("export function f(a) { if (a) return 1; return; }"),
        "1:41 unsupported: `return;` in a function that also returns a value (implicit `undefined`) (planned: T08)"
    );
}

#[test]
fn if_else_both_returning_is_fine() {
    let alloc = Allocator::default();
    let program = frontend::parse(
        &alloc,
        "t.js",
        "export function f(a) { if (a) { return 1; } else return 2; }",
    )
    .unwrap();
    assert!(js2wasi3_lower::lower(&program).is_ok());
}

#[test]
fn arity_and_callee() {
    assert_eq!(
        one("function g(a, b) { return a; }\nexport function f() { return g(1); }"),
        "2:30 unsupported: call with 1 argument(s) to a function with 2 parameter(s) (planned: T08)"
    );
    assert_eq!(
        one("export function f() { return Math.abs(1); }"),
        "1:30 unsupported: call of something other than a named function (planned: T14)"
    );
    assert_eq!(
        one("function g() { }\nexport function f() { return g(); }"),
        "2:30 unsupported: using the result of a function that returns nothing (`undefined`) (planned: T08)"
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
        "1:5 unsupported: top-level variable without a numeric-literal initializer (needs module init code) (planned: T10)"
    );
    assert_eq!(
        one("export let a = 1;"),
        "1:1 unsupported: exporting a variable (only functions can be exported) (planned: T20)"
    );
}

#[test]
fn declarations_and_functions() {
    assert_eq!(
        one("export function f() { let x; return 0; }"),
        "1:27 unsupported: declaration without an initializer (its value is `undefined`) (planned: T08)"
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
    let e = errors("export function f() {\n  let s = 'a';\n  let n = null;\n  return 1 ** 2;\n}");
    assert_eq!(
        e,
        vec![
            "2:11 unsupported: string (planned: T09)",
            "3:11 unsupported: `null` (planned: T08)",
            "4:10 unsupported: operator `**` (planned: T21 (Math.pow))",
        ]
    );
}
