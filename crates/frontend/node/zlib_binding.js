// What node's `lib/zlib.js` gets from `internalBinding('zlib')` (T150), over the Rust helper component's `codec` resource
// (`node:internal/zlib_helper`): handles with `init`, `write` (completes on a later turn), `writeSync`, `params`, `reset` and
// `close`, results in the shared `writeState` (`[availOutAfter, availInAfter]`), failures through `onerror(message, errno, code)`.
'use strict';

const zlibHelper = require('node:internal/zlib_helper');
const { Buffer } = require('buffer');

const WINDOW = 65536;
const EMPTY = new Uint8Array(0);

function bytes(view) {
  return new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
}

function initFailed() {
  const e = new Error('Initialization failed');
  e.code = 'ERR_ZLIB_INITIALIZATION_FAILED';
  return e;
}

// The helper's `failure` record, as it comes thrown from an import.
function failureOf(e) {
  const f = e !== null && typeof e === 'object' ? e.payload : undefined;
  if (f !== undefined && f !== null && typeof f.message === 'string') return f;
  return { message: String(e && e.message), errno: -2, code: 'Z_STREAM_ERROR' };
}

class Handle {
  constructor(mode, helper) {
    this.mode = mode;
    this.helper = helper;
    this.h = undefined;
    this.window = 16384;
    this.writeState = undefined;
    this.processCallback = undefined;
  }

  // `create`: the call to the helper's `create` (its arguments differ by helper).
  _init(writeState, processCallback, create) {
    this.writeState = writeState;
    this.processCallback = processCallback;
    try {
      this.h = create();
    } catch (e) {
      throw initFailed();
    }
  }

  // The helper's failure as `onerror` takes it (Brotli adds the decoder's error name).
  _failure(e) {
    return failureOf(e);
  }

  // One native write; null when it failed. The helper gets the input in windows (sized from what the last call used): a native `write` only
  // consumes as much input as fits the output chunk, so passing all of the rest each time would copy the input again for every 16 KB of output. Earlier windows
  // run without a flush; only the last carries `flush`.
  _run(flush, input, inOff, inLen, out, outOff, outLen) {
    const src = inLen > 0 ? bytes(input) : EMPTY;
    let used = 0;
    let produced = 0;
    for (;;) {
      const rest = inLen - used;
      const size = rest < this.window ? rest : this.window;
      const last = size === rest;
      let step;
      try {
        step = this.helper.process(this.h, last ? flush : 0, size > 0 ? src.subarray(inOff + used, inOff + used + size) : EMPTY, outLen - produced);
      } catch (e) {
        return this._failure(e);
      }
      out.set(step.output, outOff + produced);
      produced += step.output.length;
      used += step.consumed;
      // Next window: what the codec could use, doubled (an inflater uses a fraction of it per 16 KB of output).
      this.window = step.consumed === size ? (this.window * 2 < WINDOW ? this.window * 2 : WINDOW) : Math.max(4096, step.consumed * 2 + 1024);
      if (!last && step.consumed === size && produced < outLen) continue;
      break;
    }
    this.writeState[0] = outLen - produced;
    this.writeState[1] = inLen - used;
    return null;
  }

  writeSync(flush, input, inOff, inLen, out, outOff, outLen) {
    const failure = this._run(flush, input, inOff, inLen, out, outOff, outLen);
    if (failure !== null) this.onerror(failure.message, failure.errno, failure.code);
  }

  // node runs the work on its thread pool and calls back on the loop.
  write(flush, input, inOff, inLen, out, outOff, outLen) {
    const failure = this._run(flush, input, inOff, inLen, out, outOff, outLen);
    setImmediate(() => {
      if (failure !== null) this.onerror(failure.message, failure.errno, failure.code);
      else this.processCallback.call(this);
    });
  }

  params(level, strategy) {
    if (this.helper.params === undefined) return;
    try {
      this.helper.params(this.h, level, strategy);
    } catch (e) {
      const f = this._failure(e);
      this.onerror(f.message, f.errno, f.code);
    }
  }

  reset() {
    try {
      this.helper.reset(this.h);
    } catch (e) {
      const f = this._failure(e);
      this.onerror(f.message, f.errno, f.code);
    }
  }

  close() {
    if (this.h !== undefined) {
      this.helper.drop(this.h);
      this.h = undefined;
    }
  }
}

class Zlib extends Handle {
  constructor(mode) {
    super(mode, zlibHelper);
  }

  init(windowBits, level, memLevel, strategy, writeState, processCallback, dictionary, rejectGarbage) {
    this._init(writeState, processCallback, () => zlibHelper.create(
      this.mode, [windowBits, level, memLevel, strategy], dictionary === undefined ? undefined : bytes(dictionary), rejectGarbage));
  }
}

let crcTable;

// zlib's CRC-32 of a string (UTF-8) or a view, continuing from `value`.
function crc32(input, value) {
  if (crcTable === undefined) {
    crcTable = new Int32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      crcTable[n] = c;
    }
  }
  const view = typeof input === 'string' ? Buffer.from(input) : bytes(input);
  let c = ~value;
  for (let i = 0; i < view.length; i++) c = crcTable[(c ^ view[i]) & 0xff] ^ (c >>> 8);
  return ~c >>> 0;
}

// Brotli and zstd are helpers of their own (`node:internal/brotli_impl`, `node:internal/zstd_impl`), linked when a program names
// `brotli` / `zstd`; `Handle` is their base class.
function unavailable(name, why) {
  return class {
    constructor() {
      throw require('node:internal/unavailable').unavailableError(name, why);
    }
  };
}

const brotli = require('node:internal/brotli_impl');
const zstd = require('node:internal/zstd_impl');
const noZstd = (name) => unavailable(name, 'the zstd helper is linked only into programs that name `zstd`');
const noBrotli = (name) => unavailable(name, 'the Brotli helper is linked only into programs that name `brotli`');

module.exports = {
  Zlib,
  crc32,
  BrotliEncoder: brotli.makeEncoder === undefined ? noBrotli('zlib.BrotliCompress') : brotli.makeEncoder(Handle, bytes, failureOf),
  BrotliDecoder: brotli.makeDecoder === undefined ? noBrotli('zlib.BrotliDecompress') : brotli.makeDecoder(Handle, bytes, failureOf),
  ZstdCompress: zstd.makeCompress === undefined ? noZstd('zlib.ZstdCompress') : zstd.makeCompress(Handle, bytes, failureOf),
  ZstdDecompress: zstd.makeDecompress === undefined ? noZstd('zlib.ZstdDecompress') : zstd.makeDecompress(Handle, bytes, failureOf),
};
