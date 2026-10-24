//! node's zstd contexts on libzstd (zstd-sys): the same calls, the same errors.

use crate::{Failure, Step, failure};
use std::ffi::{CStr, c_void};
use zstd_sys as z;
use std::ffi::{c_int, c_uint};

// libzstd's entry points that take enums, with plain integers (a parameter number node passes may be one this binding has no variant for).
unsafe extern "C" {
    fn ZSTD_CCtx_setParameter(cctx: *mut z::ZSTD_CCtx, param: c_uint, value: c_int) -> usize;
    fn ZSTD_DCtx_setParameter(dctx: *mut z::ZSTD_DCtx, param: c_uint, value: c_int) -> usize;
    fn ZSTD_compressStream2(cctx: *mut z::ZSTD_CCtx, output: *mut z::ZSTD_outBuffer, input: *mut z::ZSTD_inBuffer, end_op: c_uint) -> usize;
    fn ZSTD_CCtx_reset(cctx: *mut z::ZSTD_CCtx, reset: c_uint) -> usize;
    fn ZSTD_DCtx_reset(dctx: *mut z::ZSTD_DCtx, reset: c_uint) -> usize;
}

const RESET_SESSION_ONLY: c_uint = 1;

const Z_BUF_ERROR: i32 = -5;
const END: u32 = 2;

fn is_error(code: usize) -> bool {
    unsafe { z::ZSTD_isError(code) != 0 }
}

/// libzstd's error as node reports it: libzstd's text, its error number; `code` ("ZSTD_error_prefix_unknown") is made from the
/// constants' names by the JavaScript binding.
fn zstd_failure(rc: usize) -> Failure {
    unsafe {
        let code = z::ZSTD_getErrorCode(rc);
        let message = CStr::from_ptr(z::ZSTD_getErrorString(code)).to_string_lossy().into_owned();
        failure(&message, code as i32, "")
    }
}

pub struct Compressor {
    ctx: *mut z::ZSTD_CCtx,
    params: Vec<i32>,
    pledged: Option<u64>,
    dictionary: Vec<u8>,
    started: bool,
}

impl Compressor {
    /// `params[key]` is the value of `ZSTD_c_*` number `key`, -1 when not given.
    pub fn new(params: &[i32], pledged: Option<u64>, dictionary: &[u8]) -> Result<Compressor, Failure> {
        let ctx = unsafe { z::ZSTD_createCCtx() };
        if ctx.is_null() {
            return Err(failure("Could not initialize zstd instance", -1, "ERR_ZLIB_INITIALIZATION_FAILED"));
        }
        let mut c = Compressor { ctx, params: params.to_vec(), pledged, dictionary: dictionary.to_vec(), started: false };
        c.apply()?;
        Ok(c)
    }

    fn apply(&mut self) -> Result<(), Failure> {
        unsafe {
            for (key, &value) in self.params.iter().enumerate() {
                if value == -1 {
                    continue;
                }
                if is_error(ZSTD_CCtx_setParameter(self.ctx, key as c_uint, value)) {
                    return Err(failure("Setting parameter failed", -1, "ERR_ZSTD_PARAM_SET_FAILED"));
                }
            }
            if let Some(size) = self.pledged {
                let rc = z::ZSTD_CCtx_setPledgedSrcSize(self.ctx, size);
                if is_error(rc) {
                    return Err(failure("Could not set pledged src size", -1, "ERR_ZLIB_INITIALIZATION_FAILED"));
                }
            }
            if !self.dictionary.is_empty()
                && is_error(z::ZSTD_CCtx_loadDictionary(self.ctx, self.dictionary.as_ptr() as *const c_void, self.dictionary.len()))
            {
                return Err(failure("Failed to load zstd dictionary", -1, "ERR_ZLIB_DICTIONARY_LOAD_FAILED"));
            }
        }
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), Failure> {
        unsafe {
            ZSTD_CCtx_reset(self.ctx, RESET_SESSION_ONLY);
        }
        self.started = false;
        // The session-only reset keeps the parameters and the dictionary; the pledged size is per frame.
        if let Some(size) = self.pledged {
            unsafe { z::ZSTD_CCtx_setPledgedSrcSize(self.ctx, size) };
        }
        Ok(())
    }

    pub fn process(&mut self, end_directive: u32, input: &[u8], out_capacity: usize) -> Result<Step, Failure> {
        let mut out = vec![0u8; out_capacity];
        let mut inb = z::ZSTD_inBuffer { src: input.as_ptr() as *const c_void, size: input.len(), pos: 0 };
        let mut outb = z::ZSTD_outBuffer { dst: out.as_mut_ptr() as *mut c_void, size: out_capacity, pos: 0 };
        // With the whole input and ZSTD_e_end in the first call libzstd takes the input size as the frame's size and ignores a
        // pledged one; node's wrong-size error needs the data to go in first.
        if self.pledged.is_some() && end_directive == END && !input.is_empty() && !self.started {
            let first = unsafe { ZSTD_compressStream2(self.ctx, &mut outb, &mut inb, 0) };
            if is_error(first) {
                return Err(zstd_failure(first));
            }
        }
        self.started = true;
        let rc = unsafe { ZSTD_compressStream2(self.ctx, &mut outb, &mut inb, end_directive) };
        if is_error(rc) {
            return Err(zstd_failure(rc));
        }
        out.truncate(outb.pos);
        Ok(Step { consumed: inb.pos as u32, output: out })
    }
}

impl Drop for Compressor {
    fn drop(&mut self) {
        unsafe { z::ZSTD_freeCCtx(self.ctx) };
    }
}

pub struct Decompressor {
    ctx: *mut z::ZSTD_DCtx,
    params: Vec<i32>,
    dictionary: Vec<u8>,
    complete: bool,
}

impl Decompressor {
    /// `params[key]` is the value of `ZSTD_d_*` number `key`, -1 when not given.
    pub fn new(params: &[i32], dictionary: &[u8]) -> Result<Decompressor, Failure> {
        let ctx = unsafe { z::ZSTD_createDCtx() };
        if ctx.is_null() {
            return Err(failure("Could not initialize zstd instance", -1, "ERR_ZLIB_INITIALIZATION_FAILED"));
        }
        let mut d = Decompressor { ctx, params: params.to_vec(), dictionary: dictionary.to_vec(), complete: false };
        d.apply()?;
        Ok(d)
    }

    fn apply(&mut self) -> Result<(), Failure> {
        unsafe {
            for (key, &value) in self.params.iter().enumerate() {
                if value == -1 {
                    continue;
                }
                if is_error(ZSTD_DCtx_setParameter(self.ctx, key as c_uint, value)) {
                    return Err(failure("Setting parameter failed", -1, "ERR_ZSTD_PARAM_SET_FAILED"));
                }
            }
            if !self.dictionary.is_empty()
                && is_error(z::ZSTD_DCtx_loadDictionary(self.ctx, self.dictionary.as_ptr() as *const c_void, self.dictionary.len()))
            {
                return Err(failure("Failed to load zstd dictionary", -1, "ERR_ZLIB_DICTIONARY_LOAD_FAILED"));
            }
        }
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), Failure> {
        unsafe {
            ZSTD_DCtx_reset(self.ctx, RESET_SESSION_ONLY);
        }
        self.complete = false;
        Ok(())
    }

    pub fn process(&mut self, end_directive: u32, input: &[u8], out_capacity: usize) -> Result<Step, Failure> {
        let mut out = vec![0u8; out_capacity];
        let mut inb = z::ZSTD_inBuffer { src: input.as_ptr() as *const c_void, size: input.len(), pos: 0 };
        let mut outb = z::ZSTD_outBuffer { dst: out.as_mut_ptr() as *mut c_void, size: out_capacity, pos: 0 };
        let rc = unsafe { z::ZSTD_decompressStream(self.ctx, &mut outb, &mut inb) };
        if is_error(rc) {
            return Err(zstd_failure(rc));
        }
        // `rc` is a hint for the next input (0: a frame is complete). The stream's final call has no input after the last
        // frame ended, so only a stream that ends inside a frame (or never had one) is cut off.
        if rc == 0 {
            self.complete = true;
        } else if inb.pos > 0 {
            self.complete = false;
        }
        if end_directive == END && rc != 0 && !self.complete && outb.pos < out_capacity {
            return Err(failure("unexpected end of file", Z_BUF_ERROR, "Z_BUF_ERROR"));
        }
        out.truncate(outb.pos);
        Ok(Step { consumed: inb.pos as u32, output: out })
    }
}

impl Drop for Decompressor {
    fn drop(&mut self) {
        unsafe { z::ZSTD_freeDCtx(self.ctx) };
    }
}
