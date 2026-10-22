// node's Brotli handles (`BrotliEncoder`, `BrotliDecoder` of `internalBinding('zlib')`, T150 slice 2) over the Rust helper component
// (`node:internal/brotli_helper`). `zlib_binding.js` passes in its `Handle` base class; a program that does not name `brotli`
// gets an empty stand-in for this module (`link::link`).
'use strict';

const helper = require('node:internal/brotli_helper');
const data = require('node:internal/builtin_data');

// node's `error.code` for a decoder error: "ERR_" + the name of BrotliDecoderErrorCode (`ERR__ERROR_FORMAT_PADDING_1`).
function decoderCode(errno) {
  for (const key of Object.keys(data.zlibConstants)) {
    if (key.startsWith('BROTLI_DECODER_ERROR_') && data.zlibConstants[key] === errno) {
      return 'ERR_' + key.slice('BROTLI_DECODER'.length);
    }
  }
  return 'ERR_BROTLI_DECODER_ERROR';
}

function makeHandle(Handle, bytes, failureOf, decoder) {
  return class extends Handle {
    constructor(mode) {
      super(mode, helper);
    }

    init(params, writeState, processCallback, dictionary) {
      // The lib's Uint32Array holds -1 (all bits) for a parameter that was not given.
      const values = Array.from(new Int32Array(params.buffer, params.byteOffset, params.length));
      this._init(writeState, processCallback, () => helper.create(this.mode, values));
    }

    _failure(e) {
      const f = failureOf(e);
      if (decoder && f.code === '' && f.errno < 0) return { message: f.message, errno: f.errno, code: decoderCode(f.errno) };
      return f;
    }
  };
}

module.exports = {
  makeEncoder: (Handle, bytes, failureOf) => makeHandle(Handle, bytes, failureOf, false),
  makeDecoder: (Handle, bytes, failureOf) => makeHandle(Handle, bytes, failureOf, true),
};
