//! node's `BrotliEncoderContext` / `BrotliDecoderContext` (src/node_zlib.cc) on the `brotli` crate.

use crate::{Failure, Step, failure};
use brotli::enc::encode::BrotliEncoderParameter as P;
use brotli::enc::encode::{BrotliEncoderOperation, BrotliEncoderStateStruct};
use brotli::{BrotliResult, BrotliState};
use brotli::enc::StandardAlloc;

const OPERATION_FINISH: u32 = 2;
const Z_BUF_ERROR: i32 = -5;

fn encoder_parameter(key: usize) -> Option<P> {
    Some(match key {
        0 => P::BROTLI_PARAM_MODE,
        1 => P::BROTLI_PARAM_QUALITY,
        2 => P::BROTLI_PARAM_LGWIN,
        3 => P::BROTLI_PARAM_LGBLOCK,
        4 => P::BROTLI_PARAM_DISABLE_LITERAL_CONTEXT_MODELING,
        5 => P::BROTLI_PARAM_SIZE_HINT,
        6 => P::BROTLI_PARAM_LARGE_WINDOW,
        _ => return None,
    })
}

pub struct Encoder {
    state: BrotliEncoderStateStruct<StandardAlloc>,
    params: Vec<i32>,
}

impl Encoder {
    /// `params[key]` is the value of `BROTLI_PARAM_*` number `key`, -1 when not given.
    pub fn new(params: &[i32]) -> Result<Encoder, Failure> {
        let mut e = Encoder { state: BrotliEncoderStateStruct::new(StandardAlloc::default()), params: params.to_vec() };
        e.apply()?;
        Ok(e)
    }

    fn apply(&mut self) -> Result<(), Failure> {
        for (key, &value) in self.params.iter().enumerate() {
            if value == -1 {
                continue;
            }
            match encoder_parameter(key) {
                Some(p) => {
                    if !self.state.set_parameter(p, value as u32) {
                        return Err(failure("Setting parameter failed", -1, "ERR_BROTLI_PARAM_SET_FAILED"));
                    }
                }
                // NPOSTFIX and NDIRECT (7, 8) only tune the compressor; this encoder has no such knobs.
                None if key == 7 || key == 8 => {}
                None => return Err(failure("Setting parameter failed", -1, "ERR_BROTLI_PARAM_SET_FAILED")),
            }
        }
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), Failure> {
        self.state = BrotliEncoderStateStruct::new(StandardAlloc::default());
        self.apply()
    }

    pub fn process(&mut self, flush: u32, input: &[u8], out_capacity: usize) -> Result<Step, Failure> {
        let op = match flush {
            0 => BrotliEncoderOperation::BROTLI_OPERATION_PROCESS,
            1 => BrotliEncoderOperation::BROTLI_OPERATION_FLUSH,
            2 => BrotliEncoderOperation::BROTLI_OPERATION_FINISH,
            _ => BrotliEncoderOperation::BROTLI_OPERATION_EMIT_METADATA,
        };
        let mut out = vec![0u8; out_capacity];
        let (mut avail_in, mut in_off) = (input.len(), 0usize);
        let (mut avail_out, mut out_off) = (out_capacity, 0usize);
        let mut total = None;
        let ok = self.state.compress_stream(
            op,
            &mut avail_in,
            input,
            &mut in_off,
            &mut avail_out,
            &mut out,
            &mut out_off,
            &mut total,
            &mut |_, _, _, _| (),
        );
        if !ok {
            return Err(failure("Compression failed", -1, "ERR_BROTLI_COMPRESSION_FAILED"));
        }
        out.truncate(out_off);
        Ok(Step { consumed: in_off as u32, output: out })
    }
}

pub struct Decoder {
    state: BrotliState<StandardAlloc, StandardAlloc, StandardAlloc>,
    params: Vec<i32>,
}

impl Decoder {
    /// `params`: BROTLI_DECODER_PARAM_DISABLE_RING_BUFFER_REALLOCATION (0), BROTLI_DECODER_PARAM_LARGE_WINDOW (1); -1 when not given.
    pub fn new(params: &[i32]) -> Result<Decoder, Failure> {
        let mut d = Decoder { state: Self::fresh(), params: params.to_vec() };
        d.apply()?;
        Ok(d)
    }

    fn fresh() -> BrotliState<StandardAlloc, StandardAlloc, StandardAlloc> {
        BrotliState::new(StandardAlloc::default(), StandardAlloc::default(), StandardAlloc::default())
    }

    fn apply(&mut self) -> Result<(), Failure> {
        for (key, &value) in self.params.iter().enumerate() {
            if value == -1 {
                continue;
            }
            match key {
                // DISABLE_RING_BUFFER_REALLOCATION only saves memory; the crate does not expose it.
                0 => {}
                1 => self.state.large_window = value != 0,
                _ => return Err(failure("Setting parameter failed", -1, "ERR_BROTLI_PARAM_SET_FAILED")),
            }
        }
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), Failure> {
        self.state = Self::fresh();
        self.apply()
    }

    pub fn process(&mut self, flush: u32, input: &[u8], out_capacity: usize) -> Result<Step, Failure> {
        let mut out = vec![0u8; out_capacity];
        let (mut avail_in, mut in_off) = (input.len(), 0usize);
        let (mut avail_out, mut out_off) = (out_capacity, 0usize);
        let mut total = 0usize;
        let r = brotli::BrotliDecompressStream(
            &mut avail_in,
            &mut in_off,
            input,
            &mut avail_out,
            &mut out_off,
            &mut out,
            &mut total,
            &mut self.state,
        );
        match r {
            BrotliResult::ResultFailure => {
                let code = self.state.error_code as i32;
                // node's `error.code` is built from the constants' names by the JavaScript binding.
                return Err(failure("Decompression failed", code, ""));
            }
            BrotliResult::NeedsMoreInput if flush == OPERATION_FINISH => {
                return Err(failure("unexpected end of file", Z_BUF_ERROR, "Z_BUF_ERROR"));
            }
            _ => {}
        }
        out.truncate(out_off);
        Ok(Step { consumed: in_off as u32, output: out })
    }
}
