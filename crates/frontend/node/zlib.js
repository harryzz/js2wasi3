// node's `zlib` (T65): loads with node 24's constants, codes and legacy `Z_*` properties, and `crc32`
// works; streams and (de)compression throw (planned: F284).
"use strict";

const { Transform } = require("stream");
const { Buffer } = require("buffer");
const data = require("node:internal/builtin_data");
const { unavailableError, fns } = require("node:internal/unavailable");

// A program that names `zlib` links node's ported lib (T150, `node:internal/zlib_impl`); otherwise it is an empty stand-in
// and this file keeps loading with the constants, codes and `crc32`, its compression throwing.
const impl = require("node:internal/zlib_impl");
if (impl.Deflate !== undefined) {
  module.exports = impl;
  return;
}

const WHY = "no compression in js2wasi3 yet";
const NAMES = ["Deflate", "Inflate", "Gzip", "Gunzip", "DeflateRaw", "InflateRaw", "Unzip", "BrotliCompress",
  "BrotliDecompress", "ZstdCompress", "ZstdDecompress"];

let crcTable;

function crc32(input, value = 0) {
  if (typeof input !== "string" && !ArrayBuffer.isView(input)) {
    throw $$errInvalidArgType("data", "of type string or an instance of Buffer, TypedArray, or DataView", input);
  }
  if (typeof value !== "number") throw $$errInvalidArgType("value", "of type number", value);
  if (!Number.isInteger(value)) throw $$errOutOfRange("value", "an integer", value);
  if (value < 0 || value > 4294967295) throw $$errOutOfRange("value", ">= 0 && <= 4294967295", value);
  if (crcTable === undefined) {
    crcTable = new Int32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      crcTable[n] = c;
    }
  }
  const bytes = typeof input === "string" ? Buffer.from(input)
    : new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
  let c = ~value;
  for (let i = 0; i < bytes.length; i++) c = crcTable[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return ~c >>> 0;
}

module.exports = { crc32 };
for (const name of NAMES) {
  const C = class extends Transform {
    constructor() {
      super();
      throw unavailableError(`zlib.${name}`, WHY);
    }
  };
  Object.defineProperty(C, "name", { value: name });
  module.exports[name] = C;
}
const FUNCTIONS = ["deflate", "gzip", "deflateRaw", "unzip", "inflate", "gunzip", "inflateRaw", "brotliCompress",
  "brotliDecompress", "zstdCompress", "zstdDecompress"];
for (const name of FUNCTIONS) fns(module.exports, "zlib", [name, `${name}Sync`], WHY);
fns(module.exports, "zlib", NAMES.map((n) => `create${n}`), WHY);
module.exports.constants = { ...data.zlibConstants };
const codes = { Z_OK: 0, Z_STREAM_END: 1, Z_NEED_DICT: 2, Z_ERRNO: -1, Z_STREAM_ERROR: -2, Z_DATA_ERROR: -3,
  Z_MEM_ERROR: -4, Z_BUF_ERROR: -5, Z_VERSION_ERROR: -6 };
for (const key of Object.keys(codes)) codes[codes[key]] = key;
module.exports.codes = Object.freeze(codes);
// node's legacy aliases: every constant but Brotli's, read-only and not enumerable.
for (const key of Object.keys(data.zlibConstants)) {
  if (key.startsWith("BROTLI")) continue;
  Object.defineProperty(module.exports, key, { enumerable: false, value: data.zlibConstants[key], writable: false });
}
