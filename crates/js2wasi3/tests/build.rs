//! CLI tests for `js2wasi3 build`: components run in wasmtime (a pinned tool, see TOOLS.md),
//! naming errors are reported as diagnostics.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn out_dir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("js2wasi3-cli-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn build(entry: &Path, out: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_js2wasi3"))
        .arg("build")
        .arg(entry)
        .arg("-o")
        .arg(out)
        .args(extra)
        .output()
        .unwrap()
}

fn invoke(component: &Path, call: &str) -> String {
    let out = Command::new("wasmtime")
        .args(["run", "--invoke", call])
        .arg(component)
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
fn add_component_runs() {
    let out = out_dir("add").join("add.wasm");
    let b = build(&fixture("add.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    assert_eq!(invoke(&out, "add(1, 2)"), "3");
}

#[test]
fn names_map_to_wit() {
    let out = out_dir("names").join("names.wasm");
    let b = build(&fixture("names.js"), &out, &["--world", "acme:math/calc"]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    assert_eq!(invoke(&out, "sum-odd(10, 0)"), "25");
}

#[test]
fn bad_names_are_diagnostics() {
    let out = out_dir("bad").join("bad.wasm");
    let b = build(&fixture("unsupported/bad-names.js"), &out, &[]);
    assert!(!b.status.success());
    let err = String::from_utf8_lossy(&b.stderr);
    for want in [
        "bad-names.js:7:10: error: export `foo_bar` and export `fooBar` (at 4:8) both map to the WIT name `foo-bar`",
        "bad-names.js:7:24: error: export `_1` has no WIT name",
        "help: give it a WIT name with a string export name: `export { f as \"my-name\" }`",
        "bad-names.js:7:33: error: export name \"bad name\" is not a valid WIT name",
    ] {
        assert!(err.contains(want), "missing {want:?} in:\n{err}");
    }
    assert!(!out.exists());
}

#[test]
fn bad_world_option() {
    let out = out_dir("world").join("x.wasm");
    let b = build(&fixture("add.js"), &out, &["--world", "acme:Math-x/calc"]);
    assert!(!b.status.success());
    assert!(String::from_utf8_lossy(&b.stderr).contains("is not a WIT identifier"));
}

/// Build `<name>.js` and check every line of `<name>.out` (node's results, `jsName(args) result`):
/// numbers compare as numbers, strings (`"..."`, as wasmtime prints them) exactly.
fn matches_node(name: &str) {
    let out = out_dir(name).join(format!("{name}.wasm"));
    let b = build(&fixture(&format!("{name}.js")), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let expected = std::fs::read_to_string(fixture(&format!("{name}.out"))).unwrap();
    for line in expected.lines() {
        let (call, want) = line.split_once(") ").unwrap();
        let (name, args) = call.split_once('(').unwrap();
        let wit = js2wasi3_component::names::to_wit(name).unwrap();
        let got = invoke(&out, &format!("{wit}({args})"));
        if want.starts_with('"') {
            assert_eq!(got, want, "{call})");
        } else {
            assert_eq!(
                got.parse::<f64>().unwrap(),
                want.parse::<f64>().unwrap(),
                "{call})"
            );
        }
    }
}

/// T08 acceptance: `values.js` (typeof, equality and truthiness tables, ToNumber, implicit
/// `undefined`, `>>>`, ...) matches node.
#[test]
fn values_match_node() {
    matches_node("values");
}

/// T09: `strings.js` (concatenation, template literals, `.length`, ToString of every kind, `+`
/// decided at run time, UTF-8 at the boundary, lone surrogates → U+FFFD) matches node.
#[test]
fn strings_match_node() {
    matches_node("strings");
}

/// T09 acceptance.
#[test]
fn greet_takes_and_returns_a_string() {
    let dir = out_dir("greet");
    let src = dir.join("greet.js");
    std::fs::write(
        &src,
        "export function greet(name) { return \"hi \" + name; }\n",
    )
    .unwrap();
    let out = dir.join("greet.wasm");
    let b = build(&src, &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    assert_eq!(invoke(&out, "greet(\"x\")"), "\"hi x\"");
}

/// Run a command component: (exit ok, stdout, stderr).
fn run_command(component: &Path) -> (bool, Vec<u8>, Vec<u8>) {
    let out = Command::new("wasmtime")
        .args(["run", "-Sp3"])
        .arg(component)
        .output()
        .expect("wasmtime is installed (TOOLS.md)");
    (out.status.success(), out.stdout, out.stderr)
}

/// T10 acceptance: a file without exports is a `wasi:cli/run` command.
#[test]
fn hello_command_runs() {
    let out = out_dir("hello").join("hello.wasm");
    let b = build(&fixture("hello.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let (ok, stdout, stderr) = run_command(&out);
    assert!(ok, "{}", String::from_utf8_lossy(&stderr));
    assert_eq!(String::from_utf8(stdout).unwrap(), "hello from js2wasi3\n");
    assert!(stderr.is_empty());
}

/// T10: `console.js` (top-level code, globals, `var` hoisting, every `console` method, number
/// formatting, UTF-8) prints exactly what node prints, on stdout and stderr.
#[test]
fn console_matches_node() {
    let out = out_dir("console").join("console.wasm");
    let b = build(&fixture("console.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let (ok, stdout, stderr) = run_command(&out);
    assert!(ok, "{}", String::from_utf8_lossy(&stderr));
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        std::fs::read_to_string(fixture("console.out")).unwrap()
    );
    assert_eq!(
        String::from_utf8(stderr).unwrap(),
        std::fs::read_to_string(fixture("console.err")).unwrap()
    );
}

/// T10: output larger than any buffer, written while the program runs (a later trap doesn't
/// lose it), and the `%` format-string trap.
#[test]
fn command_output_is_streamed() {
    let dir = out_dir("stream");
    let src = dir.join("stream.js");
    std::fs::write(
        &src,
        "let s = \"0123456789\";\n\
         for (let i = 0; i < 6; i++) s = s + s;\n\
         for (let i = 0; i < 5000; i++) console.log(i, s);\n\
         function fmt() { return \"%s\"; }\n\
         console.log(fmt(), 1);\n",
    )
    .unwrap();
    let out = dir.join("stream.wasm");
    let b = build(&src, &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let (ok, stdout, stderr) = run_command(&out);
    assert!(!ok, "the format string traps");
    assert!(String::from_utf8_lossy(&stderr).contains("no_format"));
    let stdout = String::from_utf8(stdout).unwrap();
    let line = "0123456789".repeat(64);
    assert_eq!(stdout.lines().count(), 5000);
    assert_eq!(stdout.lines().last().unwrap(), format!("4999 {line}"));
}

/// T12: exports that may throw return `result<T, js-error>`; the others keep plain types.
#[test]
fn throwing_exports_return_results() {
    let out = out_dir("throws").join("lib.wasm");
    let b = build(&fixture("exceptions/lib.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    for (call, want) in [
        ("double(2)", "ok(4)"),
        (
            "double(-1.5)",
            "err({name: \"RangeError\", message: \"negative: -1.5\"})",
        ),
        ("shout(\"hi\")", "ok(\"hi!\")"),
        // A thrown non-error: no name, its console.log text as the message.
        ("shout(\"\")", "err({name: \"\", message: \"empty\"})"),
        ("fail()", "err({name: \"TypeError\", message: \"\"})"),
        ("safe(-3)", "-1"),
        ("safe(4)", "4"),
        ("first-length(\"h\u{e9}llo\u{1f600}\")", "7"),
    ] {
        assert_eq!(invoke(&out, call), want, "{call}");
    }
}

/// T14: closures inside exports; `this` is `undefined` for a call from the boundary. Calls through
/// values may throw, so those exports return `result` (conservative, F34).
#[test]
fn closures_in_a_library() {
    let out = out_dir("closures").join("lib.wasm");
    let b = build(&fixture("closures/lib.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    for (call, want) in [
        ("sum-squares(3)", "ok(14)"),
        ("via-value(4)", "ok(42)"),
        ("self()", "\"undefined this\""),
        (
            "not-a-function(1)",
            "err({name: \"TypeError\", message: \"x is not a function\"})",
        ),
    ] {
        assert_eq!(invoke(&out, call), want, "{call}");
    }
}

/// T12 acceptance: a Wasm trap (here the unimplemented `%` format string) is not a JS exception:
/// neither `catch` nor `finally` runs, and the command fails with the trap.
#[test]
fn traps_are_not_caught() {
    let out = out_dir("trap").join("trap.wasm");
    let b = build(&fixture("exceptions/trap.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let (ok, stdout, stderr) = run_command(&out);
    assert!(!ok);
    let stderr = String::from_utf8_lossy(&stderr);
    assert!(stderr.contains("no_format"), "{stderr}");
    assert!(!stderr.contains("Uncaught"), "{stderr}");
    assert_eq!(String::from_utf8(stdout).unwrap(), "before\n");
}

/// T13: what isn't implemented yet traps instead of giving a wrong value: reading a built-in
/// method (functions are T14/T21), `console.log` of a plain object (F24), a user `toString` (T14).
#[test]
fn unimplemented_object_features_trap() {
    let dir = out_dir("objtrap");
    for (name, src, trap) in [
        (
            "method",
            "const o = {};\nconsole.log(\"before\");\nconsole.log(typeof o.toString);\n",
            "builtin_not_implemented",
        ),
        (
            "inspect",
            "console.log(\"before\");\nconsole.log(Object.create(null));\n",
            "inspect_object_not_implemented",
        ),
        (
            "user",
            "const o = { toString: 1 };\nconsole.log(\"before\");\nconsole.log(\"\" + o);\n",
            "user_to_primitive_not_implemented",
        ),
    ] {
        let src_path = dir.join(format!("{name}.js"));
        std::fs::write(&src_path, src).unwrap();
        let out = dir.join(format!("{name}.wasm"));
        let b = build(&src_path, &out, &[]);
        assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
        let (ok, stdout, stderr) = run_command(&out);
        assert!(!ok, "{name}");
        let stderr = String::from_utf8_lossy(&stderr);
        assert!(stderr.contains(trap), "{name}: {stderr}");
        assert_eq!(String::from_utf8(stdout).unwrap(), "before\n", "{name}");
    }
}
