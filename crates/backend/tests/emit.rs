//! Backend tests: fixtures emit valid Wasm, JS number semantics match exactly (run in
//! wasmtime, which is a pinned tool, see TOOLS.md), and unsupported IR is reported.

use std::path::{Path, PathBuf};
use std::process::Command;

use js2wasi3_frontend::{self as frontend, Allocator};
use js2wasi3_ir as ir;

fn compile(name: &str, src: &str) -> Vec<u8> {
    let alloc = Allocator::default();
    let program = frontend::parse(&alloc, name, src).unwrap_or_else(|e| panic!("{e}"));
    let module = js2wasi3_lower::lower(&program).unwrap_or_else(|e| panic!("{e}"));
    js2wasi3_backend::emit(&module).unwrap_or_else(|e| panic!("{e:#}"))
}

fn validate(bytes: &[u8]) {
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(bytes)
        .expect("valid core module");
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// Run an exported function in wasmtime and return its printed result.
fn invoke(bytes: &[u8], func: &str, args: &[&str]) -> String {
    let dir = std::env::temp_dir().join(format!("js2wasi3-backend-{}-{func}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.wasm");
    std::fs::write(&path, bytes).unwrap();
    let out = Command::new("wasmtime")
        .arg("run")
        .arg("--invoke")
        .arg(func)
        .arg(&path)
        .args(args)
        .output()
        .expect("wasmtime is installed (TOOLS.md)");
    assert!(
        out.status.success(),
        "wasmtime failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn fixtures_emit_valid_modules() {
    for name in ["add", "fib", "loops"] {
        let src = std::fs::read_to_string(fixtures().join(format!("{name}.js"))).unwrap();
        validate(&compile(name, &src));
    }
}

#[test]
fn fib_runs() {
    let src = std::fs::read_to_string(fixtures().join("fib.js")).unwrap();
    assert_eq!(invoke(&compile("fib.js", &src), "fib", &["20"]), "6765");
}

/// A JS number literal expression for `v` (NaN/±∞/-0 need expressions).
fn js(v: f64) -> String {
    if v.is_nan() {
        "(0 / 0)".into()
    } else if v.is_infinite() {
        if v > 0.0 {
            "(1 / 0)".into()
        } else {
            "(-1 / 0)".into()
        }
    } else if v == 0.0 && v.is_sign_negative() {
        "(-0)".into()
    } else {
        format!("({v:?})")
    }
}

fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    (x.trunc().rem_euclid(4294967296.0) as u64 as u32) as i32
}

/// Builds `check()`, which returns the index of the first mismatch, or -1.
/// `same` compares like `Object.is` (distinguishes -0, NaN equals NaN).
fn checker(cases: &[(String, f64)]) -> String {
    let mut src = String::from(
        "function same(a, b) {\n  if (a !== a) { return b !== b ? 1 : 0; }\n  return a === b && 1 / a === 1 / b ? 1 : 0;\n}\nexport function check() {\n",
    );
    for (i, (expr, want)) in cases.iter().enumerate() {
        src.push_str(&format!(
            "  if (!same({expr}, {})) return {i};\n",
            js(*want)
        ));
    }
    src.push_str("  return -1;\n}\n");
    src
}

const EDGE: [f64; 20] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    1.5,
    -2.75,
    3.0,
    0.1,
    7.0,
    -7.5,
    1e17,
    -1e17,
    9007199254740993.0,
    9223372036854775808.0,
    1.7976931348623157e308,
    5e-324,
    2.2250738585072014e-308,
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
];

#[test]
fn remainder_is_exact_fmod() {
    let mut cases = Vec::new();
    for x in EDGE {
        for y in EDGE {
            cases.push((format!("{} % {}", js(x), js(y)), x % y));
        }
    }
    let src = checker(&cases);
    let got = invoke(&compile("rem.js", &src), "check", &[]);
    assert_eq!(
        got,
        "-1",
        "first mismatch: case {got}: {:?}",
        got.parse::<usize>().ok().map(|i| &cases[i])
    );
}

#[test]
fn bitwise_or_zero_is_to_int32() {
    let extra = [
        2147483647.0,
        2147483648.0,
        -2147483649.0,
        4294967296.0 + 5.0,
        3.7e10,
        -3.7e10,
        18446744073709551616.0,
        1.9342813113834067e25,
        -4294967297.5,
        1e308,
    ];
    let cases: Vec<(String, f64)> = EDGE
        .iter()
        .chain(extra.iter())
        .map(|&x| (format!("{} | 0", js(x)), to_int32(x) as f64))
        .collect();
    let src = checker(&cases);
    let got = invoke(&compile("int32.js", &src), "check", &[]);
    assert_eq!(
        got,
        "-1",
        "first mismatch: case {got}: {:?}",
        got.parse::<usize>().ok().map(|i| &cases[i])
    );
}

#[test]
fn unsupported_ir_is_reported_with_kind_function_and_position() {
    let mut m = ir::Module::default();
    let mut f = ir::Function::new("g", &[], None);
    f.pos = Some(ir::Pos { line: 3, col: 1 });
    f.body = vec![ir::Stmt::Try {
        body: vec![],
        catch_local: None,
        catch_body: vec![],
    }];
    m.add_function(f);
    let err = js2wasi3_backend::emit(&m).unwrap_err().to_string();
    assert_eq!(
        err,
        "unsupported IR `Try` in function g at 3:1 (planned: T12)"
    );
}
