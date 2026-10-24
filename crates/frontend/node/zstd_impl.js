// node's zstd handles (`ZstdCompress`, `ZstdDecompress` of `internalBinding('zlib')`, T150 slice 3) over the Rust helper component
// (`node:internal/zstd_helper`, libzstd). `zlib_binding.js` passes in its `Handle` base class; a program that does not name `zstd`
// gets an empty stand-in for this module (`link::link`).
'use strict';

const helper = require('node:internal/zstd_helper');
const data = require('node:internal/builtin_data');

// node's `error.code` for a libzstd error: the constant's name (`ZSTD_error_prefix_unknown`).
function errorCode(errno) {
  for (const key of Object.keys(data.zlibConstants)) {
    if (key.startsWith('ZSTD_error_') && data.zlibConstants[key] === errno) return key;
  }
  return 'ZSTD_error_GENERIC';
}

function makeHandle(Handle, bytes, failureOf, mode) {
  return class extends Handle {
    constructor() {
      super(mode, helper);
    }

    init(params, pledgedSrcSize, writeState, processCallback, dictionary) {
      // The lib's Uint32Array holds -1 (all bits) for a parameter that was not given.
      const values = Array.from(new Int32Array(params.buffer, params.byteOffset, params.length));
      this._init(writeState, processCallback, () => helper.create(
        mode, values, typeof pledgedSrcSize === 'number' ? pledgedSrcSize : undefined,
        dictionary === undefined ? undefined : bytes(dictionary)));
    }

    _failure(e) {
      const f = failureOf(e);
      if (f.code === '' && f.errno > 0) return { message: f.message, errno: f.errno, code: errorCode(f.errno) };
      return f;
    }
  };
}

module.exports = {
  makeCompress: (Handle, bytes, failureOf) => makeHandle(Handle, bytes, failureOf, 10),
  makeDecompress: (Handle, bytes, failureOf) => makeHandle(Handle, bytes, failureOf, 11),
};
