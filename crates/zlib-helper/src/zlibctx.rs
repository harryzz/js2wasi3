//! node's `ZlibContext` (src/node_zlib.cc) on libz-rs-sys: the same state machine, the same error texts and codes.

use crate::{Failure, Step, failure};
use libz_rs_sys as z;
use std::ffi::{c_int, c_uint};
use std::mem::MaybeUninit;

const DEFLATE: u32 = 1;
const INFLATE: u32 = 2;
const GZIP: u32 = 3;
const GUNZIP: u32 = 4;
const DEFLATERAW: u32 = 5;
const INFLATERAW: u32 = 6;
const UNZIP: u32 = 7;

const GZIP_HEADER_ID1: u8 = 0x1f;
const GZIP_HEADER_ID2: u8 = 0x8b;

pub struct Context {
    strm: Box<z::z_stream>,
    mode: u32,
    window_bits: c_int,
    level: c_int,
    mem_level: c_int,
    strategy: c_int,
    dictionary: Vec<u8>,
    reject_garbage: bool,
    err: c_int,
    flush: c_int,
    gzip_id_bytes_read: u32,
    init_done: bool,
}

fn code_name(err: c_int) -> &'static str {
    match err {
        z::Z_OK => "Z_OK",
        z::Z_STREAM_END => "Z_STREAM_END",
        z::Z_NEED_DICT => "Z_NEED_DICT",
        z::Z_ERRNO => "Z_ERRNO",
        z::Z_STREAM_ERROR => "Z_STREAM_ERROR",
        z::Z_DATA_ERROR => "Z_DATA_ERROR",
        z::Z_MEM_ERROR => "Z_MEM_ERROR",
        z::Z_BUF_ERROR => "Z_BUF_ERROR",
        z::Z_VERSION_ERROR => "Z_VERSION_ERROR",
        _ => "Z_UNKNOWN_ERROR",
    }
}

impl Context {
    /// `params`: windowBits, level, memLevel, strategy (the arguments of the binding's `init`).
    pub fn new(mode: u32, params: &[i32], dictionary: Option<Vec<u8>>, reject_garbage: bool) -> Result<Context, Failure> {
        let [window_bits, level, mem_level, strategy] = params[..] else {
            return Err(failure("bad parameters", z::Z_STREAM_ERROR, "Z_STREAM_ERROR"));
        };
        let strm = Box::new(unsafe { MaybeUninit::<z::z_stream>::zeroed().assume_init() });
        let mut c = Context {
            strm,
            mode,
            window_bits,
            level,
            mem_level,
            strategy,
            dictionary: dictionary.unwrap_or_default(),
            reject_garbage,
            err: z::Z_OK,
            flush: z::Z_NO_FLUSH,
            gzip_id_bytes_read: 0,
            init_done: false,
        };
        c.init()?;
        Ok(c)
    }

    fn init(&mut self) -> Result<(), Failure> {
        let mut window_bits = self.window_bits;
        // node: raw deflate with 8 bits is made 9 (zlib refuses 8 for raw deflate).
        if self.mode == DEFLATERAW && window_bits == 8 {
            window_bits = 9;
        }
        match self.mode {
            GZIP | GUNZIP => window_bits += 16,
            UNZIP => window_bits += 32,
            DEFLATERAW | INFLATERAW => window_bits *= -1,
            _ => {}
        }
        self.window_bits = window_bits;
        let size = std::mem::size_of::<z::z_stream>() as c_int;
        self.err = unsafe {
            match self.mode {
                DEFLATE | GZIP | DEFLATERAW => z::deflateInit2_(
                    &mut *self.strm,
                    self.level,
                    z::Z_DEFLATED,
                    window_bits,
                    self.mem_level,
                    self.strategy,
                    z::zlibVersion(),
                    size,
                ),
                INFLATE | GUNZIP | INFLATERAW | UNZIP => z::inflateInit2_(&mut *self.strm, window_bits, z::zlibVersion(), size),
                _ => z::Z_STREAM_ERROR,
            }
        };
        if self.err != z::Z_OK {
            self.mode = 0;
            return Err(failure("Init error", self.err, code_name(self.err)));
        }
        self.init_done = true;
        self.set_dictionary()
    }

    /// Dictionaries are set at init and after a reset for deflate and raw inflate (inflate asks for it by `Z_NEED_DICT`).
    fn set_dictionary(&mut self) -> Result<(), Failure> {
        if self.dictionary.is_empty() {
            return Ok(());
        }
        self.err = z::Z_OK;
        unsafe {
            match self.mode {
                DEFLATE | DEFLATERAW => {
                    self.err = z::deflateSetDictionary(&mut *self.strm, self.dictionary.as_ptr(), self.dictionary.len() as c_uint)
                }
                INFLATERAW => {
                    self.err = z::inflateSetDictionary(&mut *self.strm, self.dictionary.as_ptr(), self.dictionary.len() as c_uint)
                }
                _ => {}
            }
        }
        if self.err != z::Z_OK {
            return Err(self.error_for("Failed to set dictionary"));
        }
        Ok(())
    }

    fn msg(&self) -> Option<String> {
        if self.strm.msg.is_null() {
            None
        } else {
            Some(unsafe { std::ffi::CStr::from_ptr(self.strm.msg) }.to_string_lossy().into_owned())
        }
    }

    fn error_for(&self, message: &str) -> Failure {
        let message = self.msg().unwrap_or_else(|| message.to_string());
        failure(&message, self.err, code_name(self.err))
    }

    pub fn params(&mut self, level: i32, strategy: i32) -> Result<(), Failure> {
        self.err = z::Z_OK;
        if matches!(self.mode, DEFLATE | GZIP | DEFLATERAW) {
            self.err = unsafe { z::deflateParams(&mut *self.strm, level, strategy) };
        }
        if self.err != z::Z_OK && self.err != z::Z_BUF_ERROR {
            return Err(self.error_for("Failed to set parameters"));
        }
        self.err = z::Z_OK;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), Failure> {
        self.reset_stream()
    }

    fn reset_stream(&mut self) -> Result<(), Failure> {
        self.err = z::Z_OK;
        unsafe {
            match self.mode {
                DEFLATE | DEFLATERAW | GZIP => self.err = z::deflateReset(&mut *self.strm),
                INFLATE | INFLATERAW | GUNZIP => self.err = z::inflateReset(&mut *self.strm),
                _ => {}
            }
        }
        if self.err != z::Z_OK {
            return Err(self.error_for("Failed to reset stream"));
        }
        self.set_dictionary()
    }

    /// node's `DoThreadPoolWork` then `GetErrorInfo`.
    pub fn process(&mut self, flush: c_int, input: &[u8], out_capacity: usize) -> Result<Step, Failure> {
        let mut out = vec![0u8; out_capacity];
        self.flush = flush;
        self.strm.next_in = input.as_ptr();
        self.strm.avail_in = input.len() as c_uint;
        self.strm.next_out = out.as_mut_ptr();
        self.strm.avail_out = out_capacity as c_uint;
        let strm: *mut z::z_stream = &mut *self.strm;
        unsafe {
            match self.mode {
                DEFLATE | GZIP | DEFLATERAW => self.err = z::deflate(strm, flush),
                UNZIP | INFLATE | GUNZIP | INFLATERAW => {
                    if self.mode == UNZIP {
                        self.sniff_header(input);
                    }
                    self.err = z::inflate(strm, flush);
                    // Data encoded with a dictionary (INFLATERAW had it set at init).
                    if self.mode != INFLATERAW && self.err == z::Z_NEED_DICT && !self.dictionary.is_empty() {
                        self.err = z::inflateSetDictionary(strm, self.dictionary.as_ptr(), self.dictionary.len() as c_uint);
                        if self.err == z::Z_OK {
                            self.err = z::inflate(strm, flush);
                        } else if self.err == z::Z_DATA_ERROR {
                            // Make it possible to tell a bad dictionary from bad input.
                            self.err = z::Z_NEED_DICT;
                        }
                    }
                    // Another gzip member in the same input, or trailing garbage; trailing zeros are padding.
                    while self.strm.avail_in > 0
                        && self.mode == GUNZIP
                        && self.err == z::Z_STREAM_END
                        && *self.strm.next_in != 0
                    {
                        if self.reject_garbage {
                            return Err(failure("unexpected data after the end of the stream", -3, "ERR_TRAILING_JUNK_AFTER_STREAM_END"));
                        }
                        self.reset_stream()?;
                        self.err = z::inflate(strm, flush);
                    }
                }
                _ => {}
            }
        }
        // node's CheckError.
        match self.err {
            z::Z_OK | z::Z_BUF_ERROR => {
                if self.strm.avail_out != 0 && flush == z::Z_FINISH {
                    return Err(self.error_for("unexpected end of file"));
                }
            }
            z::Z_STREAM_END => {}
            z::Z_NEED_DICT => {
                let message = if self.dictionary.is_empty() { "Missing dictionary" } else { "Bad dictionary" };
                return Err(self.error_for(message));
            }
            _ => return Err(self.error_for("Zlib error")),
        }
        let produced = out_capacity - self.strm.avail_out as usize;
        out.truncate(produced);
        let consumed = (input.len() - self.strm.avail_in as usize) as u32;
        Ok(Step { consumed, output: out })
    }

    /// UNZIP: the first bytes tell gzip from zlib (node's `gzip_id_bytes_read_`).
    fn sniff_header(&mut self, input: &[u8]) {
        let mut at = 0;
        if self.gzip_id_bytes_read == 0 {
            match input.first() {
                None => return,
                Some(&GZIP_HEADER_ID1) => {
                    self.gzip_id_bytes_read = 1;
                    at = 1;
                    if input.len() == 1 {
                        return;
                    }
                }
                Some(_) => {
                    self.mode = INFLATE;
                    return;
                }
            }
        }
        if self.gzip_id_bytes_read == 1 {
            match input.get(at) {
                None => {}
                Some(&GZIP_HEADER_ID2) => {
                    self.gzip_id_bytes_read = 2;
                    self.mode = GUNZIP;
                }
                Some(_) => self.mode = INFLATE,
            }
        }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        if !self.init_done {
            return;
        }
        unsafe {
            match self.mode {
                DEFLATE | GZIP | DEFLATERAW => {
                    z::deflateEnd(&mut *self.strm);
                }
                INFLATE | GUNZIP | INFLATERAW | UNZIP => {
                    z::inflateEnd(&mut *self.strm);
                }
                _ => {}
            }
        }
    }
}
