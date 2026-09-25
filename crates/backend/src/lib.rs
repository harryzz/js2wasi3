//! Backend: IR -> core Wasm bytes (WasmGC types, exception tag, linear memory for the ABI only).
//! Implemented from task T06.

pub fn emit(_module: &js2wasi3_ir::Module) -> anyhow::Result<Vec<u8>> {
    anyhow::bail!("backend::emit not implemented (task T06)")
}
