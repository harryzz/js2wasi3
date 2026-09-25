//! Frontend: parse an ES module (oxc) and run semantic analysis.
//! Owns source text, spans and diagnostics. Knows nothing about Wasm.
//! Implemented in task T03.

/// Parsed and checked program. Filled in by T03.
pub struct Program;

pub fn parse(_path: &str, _source: &str) -> anyhow::Result<Program> {
    anyhow::bail!("frontend::parse not implemented (task T03)")
}
