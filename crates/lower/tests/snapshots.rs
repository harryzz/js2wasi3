//! IR snapshots for `tests/fixtures/*.js`: each `x.js` has its expected IR in `x.ir`.
//! Regenerate after an intended change with `UPDATE_SNAPSHOTS=1 cargo test -p js2wasi3-lower`
//! and review the diff before committing.

use std::path::Path;

use js2wasi3_frontend::{self as frontend, Allocator};

fn lower_file(path: &Path) -> Result<String, String> {
    let source = std::fs::read_to_string(path).unwrap();
    let alloc = Allocator::default();
    let name = path.file_name().unwrap().to_str().unwrap();
    let program = frontend::parse(&alloc, name, &source).map_err(|d| d.to_string())?;
    js2wasi3_lower::lower(&program)
        .map(|m| m.to_string())
        .map_err(|d| d.to_string())
}

#[test]
fn fixtures_match_ir_snapshots() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    let mut checked = 0;
    for name in ["add", "fib", "loops"] {
        let js = dir.join(format!("{name}.js"));
        let snap = dir.join(format!("{name}.ir"));
        let got = lower_file(&js).unwrap_or_else(|e| panic!("{name}.js failed to lower:\n{e}"));
        if update {
            std::fs::write(&snap, &got).unwrap();
        }
        let want = std::fs::read_to_string(&snap)
            .unwrap_or_else(|_| panic!("missing {name}.ir (run with UPDATE_SNAPSHOTS=1)"));
        assert_eq!(got, want, "IR snapshot mismatch for {name}.js");
        checked += 1;
    }
    assert_eq!(checked, 3);
}

#[test]
fn string_literal_is_rejected_precisely() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/unsupported");
    let err = lower_file(&dir.join("string.js")).expect_err("strings are not supported yet");
    assert_eq!(
        err,
        "string.js:2:13: error: unsupported: string (planned: T09)"
    );
}
