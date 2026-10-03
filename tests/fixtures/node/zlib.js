// `zlib` (T65): node's shape, constants, legacy `Z_*` properties and `crc32`.
import zlib from "node:zlib";
import { Transform } from "node:stream";

console.log(Object.keys(zlib).sort().join(" "));
console.log(Object.keys(zlib.constants).length, zlib.constants.Z_SYNC_FLUSH, zlib.constants.BROTLI_PARAM_QUALITY,
  zlib.constants.Z_DEFAULT_WINDOWBITS, zlib.constants.ZSTD_c_compressionLevel);
console.log(zlib.Z_SYNC_FLUSH, zlib.Z_DEFAULT_WINDOWBITS, zlib.Z_BEST_COMPRESSION, zlib.BROTLI_PARAM_QUALITY,
  Object.getOwnPropertyDescriptor(zlib, "Z_OK"));
console.log(zlib.codes, Object.isFrozen(zlib.codes));
console.log(zlib.crc32("hello"), zlib.crc32(Buffer.from("hello")), zlib.crc32("world", zlib.crc32("hello ")),
  zlib.crc32("hello world"), zlib.crc32(""), zlib.crc32(new Uint8Array([1, 2, 3]).subarray(1)));
for (const f of [() => zlib.crc32(5), () => zlib.crc32("a", -1), () => zlib.crc32("a", 1.5)]) {
  try { f(); } catch (e) { console.log(e.name, e.code, e.message); }
}
console.log(Object.getPrototypeOf(zlib.Gzip.prototype) === Transform.prototype || Transform.prototype.isPrototypeOf(zlib.Gzip.prototype));
console.log(typeof zlib.createDeflateRaw, typeof zlib.gzipSync, typeof zlib.inflateRaw, zlib.Deflate.name);
