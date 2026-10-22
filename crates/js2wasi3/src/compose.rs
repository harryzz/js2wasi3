//! The helper components (T149 `crypto`, T150 `zlib`) composed into the program at build time: a program whose component imports
//! a helper's interface gets that helper plugged in (`wac plug`, as the `wac-graph` crate does it), so `build` writes one finished
//! component that runs on a stock wasmtime. The helpers' `.wasm` files are built by `scripts/build-*-helper.sh` and checked in.

use anyhow::{Context, Result};
use wac_graph::{CompositionGraph, EncodeOptions, plug};
use wac_types::Package;

/// (an interface the helper exports, the helper component)
const HELPERS: [(&[u8], &[u8], &str); 2] = [
    (
        b"js2wasi3:crypto/primitives@0",
        include_bytes!("../assets/crypto-helper.wasm"),
        "crypto-helper",
    ),
    (
        b"js2wasi3:zlib/codecs@0",
        include_bytes!("../assets/zlib-helper.wasm"),
        "zlib-helper",
    ),
];

/// The component with every helper it imports plugged in; other bytes (a core module, a program without helpers) unchanged.
pub fn helpers(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if !wasmparser::Parser::is_component(&bytes) {
        return Ok(bytes);
    }
    let wanted: Vec<_> = HELPERS
        .iter()
        .filter(|(marker, _, _)| bytes.windows(marker.len()).any(|w| w == *marker))
        .collect();
    if wanted.is_empty() {
        return Ok(bytes);
    }
    let mut graph = CompositionGraph::new();
    let socket = Package::from_bytes("program", None, bytes, graph.types_mut())?;
    let socket = graph.register_package(socket)?;
    let mut plugs = Vec::new();
    for (_, helper, name) in wanted {
        let p = Package::from_bytes(name, None, helper.to_vec(), graph.types_mut())?;
        plugs.push(graph.register_package(p)?);
    }
    plug(&mut graph, plugs, socket).context("composing the helper components")?;
    graph.encode(EncodeOptions::default()).context("encoding the composed component")
}
