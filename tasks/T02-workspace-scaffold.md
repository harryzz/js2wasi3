# T02: Workspace scaffold

Status: **done** (2026-09-25) | Depends on: T01

## Result
- Cargo workspace with `crates/{js2wasi3,frontend,ir,backend,component}`. Dependencies are pinned in the root `Cargo.toml` and the toolchain in `rust-toolchain.toml`.
- The CLI `js2wasi3 build <entry> -o out.wasm` runs the whole pipeline. Every stage bails with "not implemented (task Txx)".
- Fixtures: `tests/fixtures/add.js`, `tests/fixtures/hello.js`.
- `cargo build` passes.
