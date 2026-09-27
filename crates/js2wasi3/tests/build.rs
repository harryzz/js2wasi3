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
    invoke_with(component, call, &[])
}

/// [`invoke`] with more `wasmtime run` options.
fn invoke_with(component: &Path, call: &str, options: &[&str]) -> String {
    let out = Command::new("wasmtime")
        .arg("run")
        .args(options)
        .args(["--invoke", call])
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
         console.log(\"\".anchor);\n",
    )
    .unwrap();
    let out = dir.join("stream.wasm");
    let b = build(&src, &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let (ok, stdout, stderr) = run_command(&out);
    assert!(!ok, "the unimplemented method traps");
    assert!(String::from_utf8_lossy(&stderr).contains("builtin_not_implemented"));
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

/// T21: prelude built-ins in a library (string → number, string methods, `parseFloat`).
#[test]
fn builtins_in_a_library() {
    let out = out_dir("builtins").join("lib.wasm");
    let b = build(&fixture("builtins/lib.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    for (call, want) in [
        ("times(3)", "15"),
        ("shout(\"  hi \")", "ok(\"HI!!!!\")"),
        ("parse(\"2.5e1\")", "ok(27)"),
        ("evens(7)", "ok(56)"),
        ("distinct(\"hello\")", "ok(402)"),
        (
            "json(2)",
            "ok(\"{\\\"n\\\":2,\\\"list\\\":[2,\\\"x\\\",null]}\")",
        ),
        (
            "reparse(\"{\\\"a\\\":[1,2.5,\\\"x\\\"]}\")",
            "ok(\"{\\\"a\\\":[2,3.5,\\\"x\\\"]}\")",
        ),
        (
            "reparse(\"[1,]\")",
            "err({name: \"SyntaxError\", message: \"Unexpected token \\']\\', \\\"[1,]\\\" is not valid JSON\"})",
        ),
    ] {
        assert_eq!(invoke(&out, call), want, "{call}");
    }
}

/// T12 acceptance: a Wasm trap (here an unimplemented wrapper object, F106) is not a JS exception:
/// neither `catch` nor `finally` runs, and the command fails with the trap.
#[test]
fn traps_are_not_caught() {
    let out = out_dir("trap").join("trap.wasm");
    let b = build(&fixture("exceptions/trap.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let (ok, stdout, stderr) = run_command(&out);
    assert!(!ok);
    let stderr = String::from_utf8_lossy(&stderr);
    assert!(
        stderr.contains("builtin_not_implemented"),
        "{stderr}"
    );
    assert!(!stderr.contains("Uncaught"), "{stderr}");
    assert_eq!(String::from_utf8(stdout).unwrap(), "before\n");
}

/// T13: what isn't implemented yet traps instead of giving a wrong value: reading a built-in
/// method (functions are T14/T21), `console.log` of an error inside an object (no stack: F24), a built-in
/// `toString` that isn't implemented (`Function.prototype.toString` when the code never names
/// `toString`: functions then carry no source text, T25).
#[test]
fn unimplemented_object_features_trap() {
    let dir = out_dir("objtrap");
    for (name, src, trap) in [
        (
            "method",
            "const s = \"x\";\nconsole.log(\"before\");\nconsole.log(typeof s.match);\n",
            "builtin_not_implemented",
        ),
        (
            "inspect",
            "const o = { e: new Error(\"x\") };\nconsole.log(\"before\");\nconsole.log(o);\n",
            "inspect_object_not_implemented",
        ),
        (
            "user",
            "const f = function () {};\nconsole.log(\"before\");\nconsole.log(\"\" + f);\n",
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

/// T19 acceptance: two concurrent timers take as long as one (their waits overlap), two in a row
/// twice as long. Wall times, best of three runs each: a run can't be shorter than its timers,
/// and the concurrent run must be clearly shorter than the sequential one (300 ms timers leave
/// room for wasmtime's startup and for other tests running in parallel).
#[test]
fn concurrent_timers_overlap() {
    let dir = out_dir("timers");
    let program = |name: &str, body: &str| {
        let src = dir.join(format!("{name}.js"));
        std::fs::write(
            &src,
            format!(
                "const sleep = (ms, v) => new Promise((r) => setTimeout(() => r(v), ms));\n{body}\n"
            ),
        )
        .unwrap();
        let out = dir.join(format!("{name}.wasm"));
        let b = build(&src, &out, &[]);
        assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
        out
    };
    let time = |component: &Path, expect: &str| {
        (0..3)
            .map(|_| {
                let start = std::time::Instant::now();
                let (ok, stdout, stderr) = run_command(component);
                let t = start.elapsed().as_millis();
                assert!(ok, "{}", String::from_utf8_lossy(&stderr));
                assert_eq!(String::from_utf8(stdout).unwrap(), expect);
                t
            })
            .min()
            .unwrap()
    };
    let both = program(
        "both",
        "console.log((await Promise.all([sleep(300, \"a\"), sleep(300, \"b\")])).join(\"\"));",
    );
    let seq = program(
        "seq",
        "console.log((await sleep(300, \"a\")) + (await sleep(300, \"b\")));",
    );
    let both = time(&both, "ab\n");
    let seq = time(&seq, "ab\n");
    assert!(both >= 300, "concurrent: {both} ms");
    assert!(seq >= 600, "sequential: {seq} ms");
    assert!(
        both + 200 < seq,
        "concurrent {both} ms vs sequential {seq} ms"
    );
}

/// T20 (F11): a library's top-level code runs before the first export call (each `--invoke` is a
/// fresh instance, so every call sees it run once).
#[test]
fn library_init_runs_first() {
    let out = out_dir("init").join("init.wasm");
    let b = build(&fixture("library/init.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    for (call, want) in [
        ("square(3)", "ok(9)"),
        ("greet()", "ok(\"hello, world\")"),
        ("lookup(\"b\")", "ok(2)"),
        ("lookup(\"z\")", "ok(-1)"),
        ("norm()", "ok(5)"),
        ("count()", "ok(1)"),
    ] {
        assert_eq!(invoke(&out, call), want, "{call}");
    }
    // An init that throws fails every call with its error, like a failed module evaluation.
    let out = out_dir("init-throws").join("init-throws.wasm");
    let b = build(&fixture("library/init-throws.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let err = "err({name: \"SyntaxError\", message: \"Expected property name or \\'}\\' in JSON at position 1 (line 1 column 2)\"})";
    for call in ["one()", "limit()"] {
        assert_eq!(invoke(&out, call), err, "{call}");
    }
    // Only numbers: no JS values anywhere but the init state, and no `result`.
    let dir = out_dir("init-numbers");
    let src = dir.join("sum.js");
    std::fs::write(
        &src,
        "let total = 0;\nfor (let i = 1; i <= 4; i++) total += i;\nexport function sum(x) { return total + x; }\n",
    )
    .unwrap();
    let out = dir.join("sum.wasm");
    let b = build(&src, &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    assert_eq!(invoke(&out, "sum(0.5)"), "10.5");
}

/// T20 (F65): jobs queued by a library call run before it returns; a rejection nobody handled
/// (also one from a job) fails the call.
#[test]
fn async_exports_await_their_promise() {
    let out = out_dir("async").join("async.wasm");
    let b = build(&fixture("library/async.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    let wit = Command::new("wasm-tools")
        .args(["component", "wit"])
        .arg(&out)
        .output()
        .unwrap();
    let wit = String::from_utf8(wit.stdout).unwrap();
    for line in [
        "export add: async func(a: f64, b: f64) -> result<f64, js-error>;",
        "export greet: async func(name: string) -> result<string, js-error>;",
        "export nothing: async func() -> result<_, js-error>;",
    ] {
        assert!(wit.contains(line), "{line}\n{wit}");
    }
    // As node gives them (`await m.f(...)`).
    for (call, want) in [
        ("add(1, 2)", "ok(4)"),
        ("greet(\"bob\")", "ok(\"hi bob\")"),
        ("inferred()", "ok(\"str1\")"),
        ("nothing()", "ok"),
        (
            "fails(1)",
            "err({name: \"RangeError\", message: \"too big: 1\"})",
        ),
        ("fails(-1)", "ok(-1)"),
        ("rejects-string()", "err({name: \"\", message: \"plain\"})"),
        ("len(\"h\u{e9}llo\")", "ok(5)"),
        ("default()", "ok(1)"),
        // Not in node's terms: nothing can settle the promise, another rejection went unhandled.
        (
            "never()",
            "err({name: \"Error\", message: \"the promise of an async export never settled (nothing is left that could settle it)\"})",
        ),
        (
            "stray-rejection()",
            "err({name: \"TypeError\", message: \"nobody handled me\"})",
        ),
    ] {
        assert_eq!(invoke(&out, call), want, "{call}");
    }
}

#[test]
fn async_exports_wait_for_timers() {
    let out = out_dir("async-timers").join("async-timers.wasm");
    let b = build(&fixture("library/async-timers.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    for (call, want) in [
        ("waits()", "ok(\"init timer;slept;tick;immediate;\")"),
        ("interval()", "ok(3)"),
        // node crashes with the exception; the call fails.
        (
            "timer-throws()",
            "err({name: \"TypeError\", message: \"in a timer\"})",
        ),
        (
            "rejects-later()",
            "err({name: \"RangeError\", message: \"later\"})",
        ),
        ("race()", "ok(2)"),
    ] {
        assert_eq!(invoke(&out, call), want, "{call}");
    }
}

#[test]
fn user_world_scalars() {
    let out = out_dir("scalars").join("scalars.wasm");
    let dir = fixture("library/scalars");
    let b = build(
        &dir.join("scalars.js"),
        &out,
        &["--wit", dir.join("world.wit").to_str().unwrap()],
    );
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    // Numbers wrap like ToInt32 and then to the width; 64-bit ones saturate.
    for (call, want) in [
        ("add-u8(200, 100)", "44"),
        ("neg-s8(-128)", "-128"),
        ("wrap-s16(40000)", "-25536"),
        ("mul-u16(300, 300)", "24464"),
        ("to-s32(3000000000.7)", "-1294967296"),
        ("to-u32(-1)", "4294967295"),
        ("big(9007199254740991, 1)", "9007199254740992"),
        ("half(1.5)", "0.75"),
        ("not(true)", "false"),
        ("truthy(\"\")", "false"),
        ("truthy(\"x\")", "true"),
        ("next-char('a')", "'b'"),
        ("next-char('\u{1F600}')", "'\u{1F601}'"),
        ("first(\"\u{1F600}x\")", "'\u{1F600}'"),
        (
            "describe(7, '\u{e9}', true)",
            "\"7 \u{e9} true number string boolean\"",
        ),
        ("slow-add(4000000000, 1000000000)", "705032704"),
        ("slow-char(\"h\u{e9}llo\")", "'h'"),
        ("fails(-1)", "-1"),
    ] {
        assert_eq!(invoke(&out, call), want, "{call}");
    }
    // No `result` in the world: a throw traps.
    let trap = Command::new("wasmtime")
        .args(["run", "--invoke", "fails(1)"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(!trap.status.success());
    // Numbers only: no JS runtime, no memory.
    let dir = out_dir("scalars-numbers");
    std::fs::create_dir_all(&dir).unwrap();
    let wit = dir.join("calc.wit");
    std::fs::write(
        &wit,
        "package acme:math;\nworld calc { export add: func(a: u32, b: u32) -> u32; }\n",
    )
    .unwrap();
    let out = dir.join("calc.wasm");
    let b = build(&fixture("add.js"), &out, &["--wit", wit.to_str().unwrap()]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    assert_eq!(invoke(&out, "add(4000000000, 1000000000)"), "705032704");
}

#[test]
fn user_world_compound_types() {
    let out = out_dir("compound").join("shapes.wasm");
    let dir = fixture("library/compound");
    let b = build(
        &dir.join("shapes.js"),
        &out,
        &["--wit", dir.join("world.wit").to_str().unwrap()],
    );
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    // jco's shapes: records/flags as objects (camelCase keys), tuples as arrays, options as
    // the value or `undefined`, enums as their names, nested results as `{tag, val}`; a
    // `result` result is return or throw (an own `payload`, else the thrown value).
    for (call, want) in [
        ("norm({x: 3.0, y: 4.0})", "5"),
        ("make-point(1.5, -2.0)", "{x: 1.5, y: -2}"),
        (
            "describe({label: \"a\", at: {x: 1.0, y: 2.0}, max-size: some(5)})",
            "\"a@1,2 max=5\"",
        ),
        (
            "describe({label: \"b\", at: {x: 0.0, y: 0.0}, max-size: none})",
            "\"b@0,0 max=undefined\"",
        ),
        (
            "relabel({label: \"a\", at: {x: 1.0, y: 2.0}, max-size: none}, \"z\")",
            "{label: \"z\", at: {x: 1, y: 2}, max-size: some(7)}",
        ),
        ("next-color(dark-blue)", "red"),
        ("perms-of({read, exec-now})", "{exec-now}"),
        ("swap((\"x\", 5))", "(5, \"x\")"),
        ("maybe-half(some(3.0))", "some(1.5)"),
        ("maybe-half(none)", "none"),
        ("deep(some(some(4)))", "\"4\""),
        ("parse(\"42\")", "ok(42)"),
        ("parse(\"x\")", "err(\"not a number: x\")"),
        ("check(1)", "ok"),
        ("check(-1)", "err(dark-blue)"),
        ("unit-result(true)", "ok"),
        ("unit-result(false)", "err"),
        ("nested(ok(\"hi\"))", "ok(some(\"hi\"))"),
        ("nested(ok(\"\"))", "ok(none)"),
        ("nested(err(7))", "err((7, 8))"),
        ("slow-point(2.0)", "ok({x: 2, y: -2})"),
        ("slow-point(-1.0)", "err(\"negative\")"),
    ] {
        assert_eq!(invoke(&out, call), want, "{call}");
    }
    // A value that doesn't fit (not a case of the enum), or an Error without `payload`: traps.
    for call in ["bad-color()", "error-thrown()"] {
        let r = Command::new("wasmtime")
            .args(["run", "--invoke", call])
            .arg(&out)
            .output()
            .unwrap();
        assert!(!r.status.success(), "{call}");
    }
}

#[test]
fn user_world_lists_and_variants() {
    let out = out_dir("lists").join("lists.wasm");
    let dir = fixture("library/lists");
    let b = build(
        &dir.join("lists.js"),
        &out,
        &["--wit", dir.join("world.wit").to_str().unwrap()],
    );
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    // jco's shapes: numeric lists as typed arrays (any array-like lowers), other lists as
    // arrays, variants as `{tag, val}` (no `val` without a payload). Params past 16 flat values
    // come in memory. Results as node gives with those arguments.
    for (call, want) in [
        ("sum([1.5, 2.0, 3.0])", "6.5"),
        ("sum([])", "0"),
        ("reverse-bytes([1, 2, 3])", "[3, 2, 1]"),
        ("bytes-from-array(4)", "[0, 100, 200, 44]"),
        ("byte-info([9, 8])", "\"true 2 9\""),
        ("words(\"a bc d\")", "[\"a\", \"bc\", \"d\"]"),
        ("join-words([\"x\", \"y\"])", "\"x+y\""),
        (
            "items([{name: \"a\", tags: [\"t\"]}, {name: \"b\", tags: []}])",
            "[{name: \"A\", tags: [\"t\", \"seen\"]}, {name: \"B\", tags: [\"seen\"]}]",
        ),
        ("matrix(3)", "[[-1, 1, 2], [3, -1, 5], [6, 7, -1]]"),
        ("floats([1.5, 0.1])", "[3, 0.2]"),
        ("wide([1, 2])", "[2, 3]"),
        ("area(circle(2.0))", "13"),
        ("area(rect((2.0, 4.0)))", "8"),
        ("area(empty)", "0"),
        ("area(named(\"abcd\"))", "4"),
        (
            "shapes()",
            "[circle(1.5), rect((2, 3)), empty, named(\"hex\")]",
        ),
        ("grow(circle(1.0))", "ok(circle(2))"),
        ("grow(empty)", "err(\"can\\'t grow empty\")"),
        (
            "count-many((1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1), \"abc\", 100)",
            "119",
        ),
        (
            "many-strings(\"a\", \"b\", \"c\", \"d\", \"e\", \"f\", \"g\", \"h\", \"i\")",
            "\"abcdefghi\"",
        ),
        ("slow-bytes(3)", "[7, 7, 7]"),
    ] {
        assert_eq!(invoke(&out, call), want, "{call}");
    }
    // A `tag` that isn't a case traps.
    let r = Command::new("wasmtime")
        .args(["run", "--invoke", "bad-shape()"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(!r.status.success());
}

#[test]
fn user_world_imports_compose_with_wac() {
    let dir = out_dir("imports");
    let fx = fixture("library/imports");
    let wit = fx.join("world.wit");
    let (app, provider, composed) = (
        dir.join("app.wasm"),
        dir.join("provider.wasm"),
        dir.join("composed.wasm"),
    );
    for (js, out, world) in [("provider.js", &provider, "provider"), ("app.js", &app, "app")] {
        let b = build(
            &fx.join(js),
            out,
            &["--wit", wit.to_str().unwrap(), "--world", world],
        );
        assert!(b.status.success(), "{js}: {}", String::from_utf8_lossy(&b.stderr));
    }
    // The provider's interface export works on its own.
    assert_eq!(invoke(&provider, "greet(\"x\")"), "\"hello x\"");
    let plug = Command::new("wac")
        .arg("plug")
        .arg(&app)
        .arg("--plug")
        .arg(&provider)
        .arg("-o")
        .arg(&composed)
        .output()
        .expect("wac is installed (TOOLS.md)");
    assert!(plug.status.success(), "{}", String::from_utf8_lossy(&plug.stderr));
    // Every WIT type both ways (the app lowers, the provider lifts, and back); a `result`'s
    // `err` is thrown as jco's ComponentError (an Error with `payload`).
    let want = [
        "5",
        "43",
        "-3",
        "6",
        "1.5",
        "π",
        "hello js",
        "false",
        r#"[{\"x\":4,\"y\":-1},{\"x\":11,\"y\":-8}]"#,
        "[true,0,100,200]",
        "xxx",
        "undefined",
        r#"[4,\"AB\"]"#,
        "dark-blue:true:false",
        "12",
        "0",
        "ok 42",
        "threw true not a number: x not a number: x",
        "ok undefined",
        "threw true undefined (see error.payload) undefined",
        "19",
    ];
    let want = format!(
        "[{}]",
        want.iter()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert_eq!(invoke(&composed, "run(3)"), want);
}

#[test]
fn async_imports_compose_with_wac() {
    let dir = out_dir("async-imports");
    let fx = fixture("library/imports");
    let wit = fx.join("world.wit");
    let (app, provider, composed) = (
        dir.join("slow-app.wasm"),
        dir.join("slow-provider.wasm"),
        dir.join("slow-composed.wasm"),
    );
    for (js, out, world) in [
        ("slow-provider.js", &provider, "slow-provider"),
        ("slow-app.js", &app, "slow-app"),
    ] {
        let b = build(
            &fx.join(js),
            out,
            &["--wit", wit.to_str().unwrap(), "--world", world],
        );
        assert!(b.status.success(), "{js}: {}", String::from_utf8_lossy(&b.stderr));
    }
    let plug = Command::new("wac")
        .arg("plug")
        .arg(&app)
        .arg("--plug")
        .arg(&provider)
        .arg("-o")
        .arg(&composed)
        .output()
        .expect("wac is installed (TOOLS.md)");
    assert!(plug.status.success(), "{}", String::from_utf8_lossy(&plug.stderr));
    // `tick` returns at once; `fetch` (50 ms) and two `delay-echo`s overlap (the provider's
    // backpressure makes the later calls wait to start); the app's 1 ms timer runs meanwhile;
    // an `err` rejects with a ComponentError.
    assert_eq!(
        invoke(&composed, "go(3)"),
        "\"7,7,7 hihi true empty/empty first,timer,fetch,tick\""
    );
}

/// `future<T>` both ways (T20 slice 6a): the provider returns futures (written when their
/// promise settles, the call going on after `task.return`) and reads one it takes; the app
/// reads the futures its imports return and writes the one it passes.
///
/// Runs with a larger initial GC heap: with wasmtime 49.0.1's default copying collector a GC
/// in these composed components hits a wasmtime internal error (see the follow-up in
/// `tasks/README.md`).
#[test]
fn futures_compose_with_wac() {
    let dir = out_dir("futures");
    let fx = fixture("library/futures");
    let wit = fx.join("world.wit");
    let (app, provider, composed) = (
        dir.join("app.wasm"),
        dir.join("provider.wasm"),
        dir.join("composed.wasm"),
    );
    for (js, out, world) in [("provider.js", &provider, "provider"), ("app.js", &app, "app")] {
        let b = build(
            &fx.join(js),
            out,
            &["--wit", wit.to_str().unwrap(), "--world", world],
        );
        assert!(b.status.success(), "{js}: {}", String::from_utf8_lossy(&b.stderr));
    }
    let plug = Command::new("wac")
        .arg("plug")
        .arg(&app)
        .arg("--plug")
        .arg(&provider)
        .arg("-o")
        .arg(&composed)
        .output()
        .expect("wac is installed (TOOLS.md)");
    assert!(plug.status.success(), "{}", String::from_utf8_lossy(&plug.stderr));
    assert_eq!(
        invoke_with(&composed, "go(3)", &["-O", "gc-heap-initial-size=67108864"]),
        "\"now 3 true; later 6; HEY; awaited 6; point 1,-2; done undefined\""
    );
}

/// T20 slice 6b: `stream<T>` ↔ async iterables of chunks, both ways through a composition: the
/// provider echoes, transforms and stops reading a stream, and writes one from a timer; the app
/// writes (async generators, a sync iterable) and reads them. The acceptance: `echo` returns its
/// input bytes (also 300 KB, several reads). F134: a large initial GC heap.
#[test]
fn streams_compose_with_wac() {
    let dir = out_dir("streams");
    let fx = fixture("library/streams");
    let wit = fx.join("world.wit");
    let (app, provider, composed) = (
        dir.join("app.wasm"),
        dir.join("provider.wasm"),
        dir.join("composed.wasm"),
    );
    for (js, out, world) in [("provider.js", &provider, "provider"), ("app.js", &app, "app")] {
        let b = build(
            &fx.join(js),
            out,
            &["--wit", wit.to_str().unwrap(), "--world", world],
        );
        assert!(b.status.success(), "{js}: {}", String::from_utf8_lossy(&b.stderr));
    }
    let plug = Command::new("wac")
        .arg("plug")
        .arg(&app)
        .arg("--plug")
        .arg(&provider)
        .arg("-o")
        .arg(&composed)
        .output()
        .expect("wac is installed (TOOLS.md)");
    assert!(plug.status.success(), "{}", String::from_utf8_lossy(&plug.stderr));
    assert_eq!(
        invoke_with(
            &composed,
            "go(\"hello streams!\")",
            &["-O", "gc-heap-initial-size=67108864"]
        ),
        "\"echo hello streams!; upper HELLO STREAMS!; take 12; take 5; big 300000 42254; \
         words w0,w1,w2\""
    );
}

#[test]
fn user_world_import_errors() {
    let dir = out_dir("import-errors");
    let wit = dir.join("w.wit");
    let js = dir.join("m.js");
    let out = dir.join("m.wasm");
    let err = |wit_text: Option<&str>, js_text: &str| {
        std::fs::write(&js, js_text).unwrap();
        let b = match wit_text {
            Some(w) => {
                std::fs::write(&wit, w).unwrap();
                build(&js, &out, &["--wit", wit.to_str().unwrap()])
            }
            None => build(&js, &out, &[]),
        };
        assert!(!b.status.success());
        String::from_utf8(b.stderr).unwrap()
    };
    let w = "package a:b;\ninterface h { f: func(); g: async func(); r: func(x: future<u8>); }\n\
             world w { import h; export h2: interface { k: func() -> u32; } export run: func(); }\n";
    let e = err(
        Some(w),
        "import { f, nope } from \"a:b/h\";\nimport { g } from \"a:b/h\";\n\
         import { r } from \"a:b/h\";\nimport x from \"a:b/other\";\n\
         export const h2 = { k() { return 1; }, extra() {} };\nexport function run() { f(); }\n",
    );
    for want in [
        "1:13: error: `a:b/h` has no function `nope`",
        "2:10: error: unsupported: importing the `async func` `g` in a world without an `async func` export",
        "3:10: error: import of `r` from `a:b/h`: unsupported: `r` takes or returns a future or stream and isn't an `async func`",
        "4:1: error: unsupported: import from `a:b/other`: the world doesn't import an interface of that name",
        "5:40: error: the world's interface `h2` has no function `extra`",
    ] {
        assert!(e.contains(want), "{want}\n{e}");
    }
    let e = err(
        Some(w),
        "export const h2 = {};\nexport function run() {}\n",
    );
    assert!(e.contains("interface export `h2` has no function `k`"), "{e}");
    let e = err(Some(w), "export function run() {}\n");
    assert!(
        e.contains("world `w` exports interface `h2`, and the module has no `export const h2 = { … }` for it"),
        "{e}"
    );
    let e = err(
        None,
        "import { f } from \"a:b/h\";\nexport function run() { f(); }\n",
    );
    assert!(
        e.contains("import from `a:b/h` without a world (`--wit`) that imports it"),
        "{e}"
    );
}

#[test]
fn user_world_mismatches() {
    let dir = out_dir("world-mismatch");
    std::fs::create_dir_all(&dir).unwrap();
    let wit = dir.join("w.wit");
    let js = dir.join("m.js");
    let out = dir.join("m.wasm");
    let err = |wit_text: &str, js_text: &str| {
        std::fs::write(&wit, wit_text).unwrap();
        std::fs::write(&js, js_text).unwrap();
        let b = build(&js, &out, &["--wit", wit.to_str().unwrap()]);
        assert!(!b.status.success());
        String::from_utf8(b.stderr).unwrap()
    };
    let e = err(
        "package a:b;\nworld w { export f: func(a: u32) -> u32; export g: async func(); export missing: func(); }\n",
        "export function f(a, b) { return a; }\nexport function g() {}\nexport function extra() {}\n",
    );
    assert!(
        e.contains("3:8: error: export `extra` (WIT name `extra`) isn't in world `w`"),
        "{e}"
    );
    assert!(
        e.contains("world `w` exports `missing: func()`, and the module has no export for it"),
        "{e}"
    );
    let e = err(
        "package a:b;\nworld w { export f: func(a: u32) -> u32; export g: async func(); }\n",
        "export function f(a, b) { return a; }\nexport function g() {}\n",
    );
    assert!(
        e.contains("1:8: error: `f` takes 2 parameters, the world's function 1"),
        "{e}"
    );
    assert!(
        e.contains("2:8: error: the world's `g` is an `async func`"),
        "{e}"
    );
    let e = err(
        "package a:b;\nworld w { export f: func(a: future<u8>); }\n",
        "export function f(a) {}\n",
    );
    assert!(
        e.contains("unsupported: `f` takes or returns a future or stream and isn't an `async func`"),
        "{e}"
    );
    let e = err(
        "package a:b;\nworld w { export f: async func(a: stream); }\n",
        "export async function f(a) {}\n",
    );
    assert!(
        e.contains("parameter `a` of `f`: type `stream<…>` (a stream without an element type) isn't supported yet (planned: later)"),
        "{e}"
    );
    let e = err(
        "package a:b;\nworld w { export f: async func() -> stream<future<u8>>; }\n",
        "export async function f() {}\n",
    );
    assert!(
        e.contains("the result of `f`: type `stream<…>` (a stream of futures or streams) isn't supported yet (planned: later)"),
        "{e}"
    );
    let e = err(
        "package a:b;\nworld w { export f: async func() -> u32; }\n",
        "export async function f() { await new Promise((r) => setTimeout(r, 1)); return 1; }\n",
    );
    assert!(
        e.contains("its world must `import wasi:clocks/monotonic-clock@0.3.1;`"),
        "{e}"
    );
}

#[test]
fn library_calls_run_their_jobs() {
    let out = out_dir("jobs").join("jobs.wasm");
    let b = build(&fixture("library/jobs.js"), &out, &[]);
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    for (call, want) in [
        ("after-init()", "ok(\"init job;\")"),
        ("call()", "ok(\"init job;call;\")"),
        (
            "rejects()",
            "err({name: \"RangeError\", message: \"late failure\"})",
        ),
        ("handled()", "ok(2)"),
        (
            "throws-in-job()",
            "err({name: \"TypeError\", message: \"job failed\"})",
        ),
        ("rejects-in-then()", "err({name: \"\", message: \"plain\"})"),
    ] {
        assert_eq!(invoke(&out, call), want, "{call}");
    }
}
