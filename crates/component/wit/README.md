WASI 0.3.1 WIT packages, vendored from `probes/wit/deps/` (fetched with `wkg wit fetch`, wkg 0.16.1).
`wasi-cli` uses the others, so all five are needed to resolve a world that imports `wasi:cli/*`.
They are embedded into the compiler with `include_str!` (see `src/lib.rs`, `command_world`).
To update: rerun `wkg wit fetch` in `probes/wit`, copy each `deps/<pkg>/package.wit` here as `<pkg>.wit`.

`wasi-http-0.3.0.wit` (T72) is wasmtime v49.0.1's `crates/wasi-http/src/p3/wit/deps/http.wit` (the version its
`-S http` serves): its interfaces only (the `service`/`middleware` worlds import 0.3.0 `wasi:cli` and are cut), and
its `use wasi:clocks/types@0.3.0.{duration}` points at the vendored 0.3.1 (`duration` is a `u64` in both).
