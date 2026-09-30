// node's `buffer` (T56): every encoding both ways, the statics and prototype methods, numbers,
// the module's exports, printing, and the `Buffer` global being the module's.
import buffer from "node:buffer";
const show = (label, f) => { try { const v = f(); console.log(label, v); } catch (e) { console.log(label, "THROWS", e.name, e.code, e.message); } };
const s = "héllo wörld 😀 \u0000";
for (const enc of ["utf8", "utf-8", "utf16le", "ucs2", "latin1", "binary", "ascii", "base64", "base64url", "hex"]) {
  const b = Buffer.from(s, enc === "hex" || enc.startsWith("base64") ? "utf8" : enc);
  show(enc, () => [b.length, b.toString(enc), Buffer.byteLength(s, enc), Buffer.from(b.toString(enc), enc).equals(b)]);
}
show("hex odd", () => Buffer.from("abc", "hex").toString("hex"));
show("hex bad", () => Buffer.from("zz11", "hex"));
show("b64 ws", () => Buffer.from(" aGk=\n", "base64").toString());
show("b64url", () => Buffer.from([251, 255]).toString("base64url"));
show("isEncoding", () => [Buffer.isEncoding("UTF8"), Buffer.isEncoding("nope"), Buffer.isEncoding("")]);
show("from arr", () => Buffer.from([257, -1, 3.7, "4"]).toJSON());
show("from ab", () => { const ab = new ArrayBuffer(8); const b = Buffer.from(ab, 2, 4); b[0] = 9; return [b.length, new Uint8Array(ab)[2]]; });
show("from obj", () => Buffer.from({ length: 2, 0: 5, 1: 6 }).toJSON().data);
show("from bad", () => Buffer.from(5));
show("alloc neg", () => Buffer.alloc(-1));
show("alloc fill", () => [Buffer.alloc(5, "abc").toString(), Buffer.alloc(4, 0x41).toString(), Buffer.alloc(4, "aGk=", "base64").toString()]);
show("allocUnsafe", () => Buffer.allocUnsafe(3).length);
show("concat", () => Buffer.concat([Buffer.from("a"), new Uint8Array([98])], 5).toJSON().data);
show("compare", () => [Buffer.compare(Buffer.from("a"), Buffer.from("b")), Buffer.from("b").compare(Buffer.from("a")), Buffer.from("abc").compare(Buffer.from("xbcx"), 1, 3, 1, 3)]);
const b = Buffer.from("abcdefabc");
show("indexOf", () => [b.indexOf("c"), b.indexOf("c", 3), b.lastIndexOf("abc"), b.indexOf(Buffer.from("fa")), b.includes("z"), b.indexOf(0x64), b.indexOf("")]);
show("slice", () => { const t = b.slice(1, 3); t[0] = 0x5a; return [b.toString(), t.toString(), b.subarray(-3).toString(), Buffer.isBuffer(b.subarray(1))]; });
show("copy", () => { const t = Buffer.alloc(4); const n = b.copy(t, 1, 2, 5); return [n, t.toJSON().data]; });
show("fill", () => [Buffer.alloc(6).fill("xy", 1, 5).toString("hex"), Buffer.alloc(3).fill(0x101).toJSON().data]);
show("write", () => { const t = Buffer.alloc(6); const n = t.write("héllo", 1, "utf8"); return [n, t.toString("hex")]; });
const nb = Buffer.alloc(16);
show("ints", () => { nb.writeUInt16LE(0xbeef, 0); nb.writeInt32BE(-5, 2); nb.writeUIntBE(0x123456, 6, 3); nb.writeIntLE(-0x1234, 9, 2); return [nb.toString("hex"), nb.readUInt16BE(0), nb.readInt32BE(2), nb.readUIntBE(6, 3), nb.readIntLE(9, 2)]; });
show("floats", () => { nb.writeFloatLE(1.5, 0); nb.writeDoubleBE(-0.1, 8); return [nb.readFloatLE(0), nb.readDoubleBE(8)]; });
show("bigint", () => { nb.writeBigInt64LE(-2n, 0); nb.writeBigUInt64BE(2n ** 63n, 8); return [nb.readBigInt64LE(0), nb.readBigUInt64BE(8)]; });
show("range", () => nb.writeUInt8(300, 0));
show("oob", () => nb.readUInt32LE(14));
show("swap", () => [Buffer.from([1, 2, 3, 4]).swap16().toJSON().data, Buffer.from([1, 2, 3, 4]).swap32().toJSON().data]);
show("toString range", () => [b.toString("utf8", 2, 4), b.toString(undefined, -1, 100)]);
show("equals", () => [Buffer.from("x").equals(Buffer.from("x")), Buffer.from("x").equals(new Uint8Array([120]))]);
show("iter", () => [...Buffer.from("ab")].concat([...Buffer.from("ab").entries()].flat()));
show("json", () => JSON.stringify({ b: Buffer.from("hi") }));
show("module", () => [buffer.kMaxLength, buffer.constants.MAX_LENGTH, buffer.constants.MAX_STRING_LENGTH, buffer.INSPECT_MAX_BYTES, typeof buffer.SlowBuffer, Buffer.poolSize]);
show("keys", () => Object.keys(buffer).join(","));

console.log(Buffer.from("hello"), [Buffer.from([1, 2])], Buffer.alloc(60));
show("atob", () => [atob("aGk="), btoa("hi")]);
show("isUtf8", () => [buffer.isUtf8(Buffer.from("héllo")), buffer.isUtf8(Buffer.from([0xc3])), buffer.isUtf8(Buffer.from([0xed, 0xa0, 0x80])), buffer.isAscii(Buffer.from("abc")), buffer.isAscii(Buffer.from("é"))]);
show("atob mod", () => buffer.atob === atob && buffer.btoa === btoa);
