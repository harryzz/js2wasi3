//! Component: pick/generate the WIT world, embed it into the core module and encode
//! a WASI 0.3 component (wit-component). No JS semantics here.
//! Implemented from task T07.

pub fn package(_core: &[u8]) -> anyhow::Result<Vec<u8>> {
    anyhow::bail!("component::package not implemented (task T07)")
}
