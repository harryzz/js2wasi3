WASI 0.3.1 WIT packages, vendored from `probes/wit/deps/` (fetched with `wkg wit fetch`, wkg 0.16.1).
`wasi-cli` uses the others, so all five are needed to resolve a world that imports `wasi:cli/*`.
They are embedded into the compiler with `include_str!` (see `src/lib.rs`, `command_world`).
To update: rerun `wkg wit fetch` in `probes/wit`, copy each `deps/<pkg>/package.wit` here as `<pkg>.wit`.
