//! The helper component behind node's zstd streams (T150 slice 3): node's `ZstdCompressContext` / `ZstdDecompressContext`
//! (`node_zlib.cc`) on libzstd (C, built for wasm with clang by `scripts/build-zstd-helper.sh`). `zlib.js` (node's lib, ported)
//! does the streams and options; this runs the codec, one `process` call per native `write`.

wit_bindgen::generate!({ path: "wit", world: "helper" });

use exports::js2wasi3::zstd::codecs::{Codec, Failure, Guest, GuestCodec, Step};
use std::cell::RefCell;

mod zstdctx;

const MODE_COMPRESS: u32 = 10;
const MODE_DECOMPRESS: u32 = 11;

struct Helper;

impl Guest for Helper {
    type Codec = Handle;
}

pub enum Inner {
    Compress(zstdctx::Compressor),
    Decompress(zstdctx::Decompressor),
}

pub struct Handle(RefCell<Inner>);

impl GuestCodec for Handle {
    fn create(mode: u32, params: Vec<i32>, pledged_src_size: Option<u64>, dictionary: Option<Vec<u8>>) -> Result<Codec, Failure> {
        let dictionary = dictionary.unwrap_or_default();
        let inner = match mode {
            MODE_COMPRESS => Inner::Compress(zstdctx::Compressor::new(&params, pledged_src_size, &dictionary)?),
            MODE_DECOMPRESS => Inner::Decompress(zstdctx::Decompressor::new(&params, &dictionary)?),
            _ => return Err(failure("unsupported mode", -1, "ERR_ZSTD_INIT_FAILED")),
        };
        Ok(Codec::new(Handle(RefCell::new(inner))))
    }

    fn process(&self, end_directive: u32, input: Vec<u8>, out_capacity: u32) -> Result<Step, Failure> {
        match &mut *self.0.borrow_mut() {
            Inner::Compress(c) => c.process(end_directive, &input, out_capacity as usize),
            Inner::Decompress(c) => c.process(end_directive, &input, out_capacity as usize),
        }
    }

    fn reset(&self) -> Result<(), Failure> {
        match &mut *self.0.borrow_mut() {
            Inner::Compress(c) => c.reset(),
            Inner::Decompress(c) => c.reset(),
        }
    }
}

pub fn failure(message: &str, errno: i32, code: &str) -> Failure {
    Failure { message: message.to_string(), errno, code: code.to_string() }
}

export!(Helper);
