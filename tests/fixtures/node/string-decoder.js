// node's `string_decoder` (T56): multi-byte characters split across writes, per encoding.
import { StringDecoder } from "node:string_decoder";
const euro = Buffer.from("€😀é");
for (const enc of ["utf8", "utf16le", "base64", "base64url", "hex", "latin1"]) {
  const d = new StringDecoder(enc);
  const bytes = enc === "utf16le" ? Buffer.from("€😀é", "utf16le") : euro;
  let out = "";
  for (let i = 0; i < bytes.length; i++) out += d.write(bytes.subarray(i, i + 1));
  out += d.end();
  console.log(enc, d.encoding, JSON.stringify(out));
}
const d = new StringDecoder();
console.log(JSON.stringify(d.write(Buffer.from([0xe2, 0x82]))), JSON.stringify(d.end()), JSON.stringify(d.end(Buffer.from("x"))));
try { new StringDecoder("nope"); } catch (e) { console.log(e.name, e.code, e.message); }
