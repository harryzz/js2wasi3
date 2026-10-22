//! The helper component behind node's Brotli streams (T150 slice 2): node's `BrotliEncoderContext` / `BrotliDecoderContext`
//! (`node_zlib.cc`) on the `brotli` crate. `zlib.js` (node's lib, ported) does the streams and options; this runs the codec,
//! one `process` call per native `write`.

wit_bindgen::generate!({ path: "wit", world: "helper" });

use exports::js2wasi3::brotli::codecs::{Codec, Failure, Guest, GuestCodec, Step};
use std::cell::RefCell;

mod brotlictx;

const MODE_BROTLI_DECODE: u32 = 8;
const MODE_BROTLI_ENCODE: u32 = 9;

struct Helper;

impl Guest for Helper {
    type Codec = Handle;
}

pub enum Inner {
    Encoder(brotlictx::Encoder),
    Decoder(brotlictx::Decoder),
}

pub struct Handle(RefCell<Inner>);

impl GuestCodec for Handle {
    fn create(mode: u32, params: Vec<i32>) -> Result<Codec, Failure> {
        let inner = match mode {
            MODE_BROTLI_DECODE => Inner::Decoder(brotlictx::Decoder::new(&params)?),
            MODE_BROTLI_ENCODE => Inner::Encoder(brotlictx::Encoder::new(&params)?),
            _ => return Err(failure("unsupported mode", -2, "Z_STREAM_ERROR")),
        };
        Ok(Codec::new(Handle(RefCell::new(inner))))
    }

    fn process(&self, operation: u32, input: Vec<u8>, out_capacity: u32) -> Result<Step, Failure> {
        match &mut *self.0.borrow_mut() {
            Inner::Encoder(c) => c.process(operation, &input, out_capacity as usize),
            Inner::Decoder(c) => c.process(operation, &input, out_capacity as usize),
        }
    }

    fn reset(&self) -> Result<(), Failure> {
        match &mut *self.0.borrow_mut() {
            Inner::Encoder(c) => c.reset(),
            Inner::Decoder(c) => c.reset(),
        }
    }
}

pub fn failure(message: &str, errno: i32, code: &str) -> Failure {
    Failure { message: message.to_string(), errno, code: code.to_string() }
}

export!(Helper);
