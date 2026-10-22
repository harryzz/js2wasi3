//! The helper component behind node's `zlib` (T150): node's native binding (`node_zlib.cc`) on Rust crates. `zlib.js` (node's own
//! lib, ported) does the streams, options and errors; this only runs the codecs, one `process` call per native `write`.

wit_bindgen::generate!({ path: "wit", world: "helper" });

use exports::js2wasi3::zlib::codecs::{Codec, CodecBorrow, Failure, Guest, GuestCodec, Step};
use std::cell::RefCell;
use std::ffi::c_int;

mod zlibctx;

struct Helper;

impl Guest for Helper {
    type Codec = Handle;
}

pub struct Handle(RefCell<zlibctx::Context>);

impl GuestCodec for Handle {
    fn create(mode: u32, params: Vec<i32>, dictionary: Option<Vec<u8>>, reject_garbage: bool) -> Result<Codec, Failure> {
        let ctx = match mode {
            1..=7 => zlibctx::Context::new(mode, &params, dictionary, reject_garbage)?,
            _ => return Err(failure("unsupported mode", -2, "Z_STREAM_ERROR")),
        };
        Ok(Codec::new(Handle(RefCell::new(ctx))))
    }

    fn process(&self, flush: u32, input: Vec<u8>, out_capacity: u32) -> Result<Step, Failure> {
        self.0.borrow_mut().process(flush as c_int, &input, out_capacity as usize)
    }

    fn params(&self, level: i32, strategy: i32) -> Result<(), Failure> {
        self.0.borrow_mut().params(level, strategy)
    }

    fn reset(&self) -> Result<(), Failure> {
        self.0.borrow_mut().reset()
    }
}

pub fn failure(message: &str, errno: i32, code: &str) -> Failure {
    Failure { message: message.to_string(), errno, code: code.to_string() }
}

#[allow(dead_code)]
fn _unused(_: CodecBorrow<'_>) {}

export!(Helper);
