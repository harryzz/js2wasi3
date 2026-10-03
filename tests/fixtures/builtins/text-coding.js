// TextEncoder and TextDecoder (T64): node's globals.
const e = new TextEncoder(); console.log(e, e.encoding, e.encode("hé€😀\ud800x"), e.encode(), TextEncoder.length, TextDecoder.length);
console.log(e.encodeInto("héllo", new Uint8Array(3)), e.encodeInto("😀", new Uint8Array(3)));
const d = new TextDecoder(); console.log(d, d.encoding, d.fatal, d.ignoreBOM);
console.log(d.decode(new Uint8Array([0xef,0xbb,0xbf,0x68,0xc3,0xa9,0xf0,0x9f,0x98,0x80,0xff,0xe2,0x82,0x41,0xf0,0x9f])));
console.log(new TextDecoder("utf-8", {ignoreBOM:true}).decode(new Uint8Array([0xef,0xbb,0xbf,0x41])).length);
try { new TextDecoder("utf8", {fatal:true}).decode(new Uint8Array([0xff])) } catch (x) { console.log(x.name, x.code, x.message) }
try { new TextDecoder("foo") } catch (x) { console.log(x.name, x.code, x.message) }
try { d.decode(5) } catch (x) { console.log(x.name, x.code, x.message) }
const s = new TextDecoder(); console.log(JSON.stringify(s.decode(new Uint8Array([0xe2,0x82]), {stream:true})), s.decode(new Uint8Array([0xac])));
console.log(new TextDecoder("utf-16le").decode(new Uint8Array([0x41,0,0x3d,0xd8,0,0xde])), new TextDecoder("UTF8").encoding, new TextDecoder("utf-16").encoding);
console.log(d.decode(new Uint8Array([0x68,0x69]).buffer), d.decode(new DataView(new Uint8Array([0x6a]).buffer)), d.decode(new Uint16Array([0x6968])));
console.log(Object.prototype.toString.call(e), Object.prototype.toString.call(d), Object.getOwnPropertyNames(TextDecoder.prototype), Object.getOwnPropertyNames(TextEncoder.prototype));
console.log(new TextDecoder("latin1").encoding, new TextDecoder("ascii").decode(new Uint8Array([0x80, 0x41])));
