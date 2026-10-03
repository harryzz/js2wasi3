// node's `crypto` subset (T65): random values, hashes, HMAC, AES (ECB, CBC, CTR, CCM), webcrypto's random.
import crypto from "node:crypto";
import { createHash, createHmac, createCipheriv, createDecipheriv, randomBytes, randomInt, randomUUID } from "node:crypto";

const r = randomBytes(16);
console.log(Buffer.isBuffer(r), r.length, randomBytes(0).length);
randomBytes(4, (err, b) => console.log("async", err, b.length));
const ri = Array.from({ length: 200 }, () => randomInt(256));
console.log(ri.every((x) => Number.isInteger(x) && x >= 0 && x < 256), randomInt(5, 6), [1, 2, 3].includes(randomInt(1, 4)));
console.log(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(randomUUID()));
const u32 = crypto.getRandomValues(new Uint32Array(4));
console.log(u32 instanceof Uint32Array, u32.length, crypto.webcrypto.getRandomValues(new Uint8Array(2)).length);
console.log(typeof crypto.randomFillSync(Buffer.alloc(3)), crypto.randomFillSync(new Uint8Array(8), 2, 3).length);
for (const bad of [() => randomBytes(-1), () => randomInt(3, 3), () => randomInt(1.5), () => crypto.getRandomValues(new Float64Array(1)), () => crypto.getRandomValues(new Uint8Array(65537))]) {
  try { bad(); } catch (e) { console.log(e.name, e.code, e.message); }
}

const inputs = ["", "abc", "The quick brown fox jumps over the lazy dog", "é€😀", "a".repeat(1000)];
for (const alg of ["md5", "sha1", "sha224", "sha256", "sha384", "sha512"]) {
  for (const s of inputs) console.log(alg, s.length, createHash(alg).update(s).digest("hex"));
  const h = createHash(alg); h.update("ab"); const c = h.copy(); h.update("c"); c.update(Buffer.from("c"));
  console.log(alg, h.digest("base64"), c.digest().toString("hex") === createHash(alg).update("abc").digest("hex"));
  console.log("hmac", alg, createHmac(alg, "key").update("The quick brown fox jumps over the lazy dog").digest("hex"));
  console.log("hmac-long-key", alg, createHmac(alg, "k".repeat(200)).update(new Uint8Array([1, 2, 3])).digest("base64url"));
}
console.log(createHash("sha256").update("616263", "hex").digest("hex"), createHash("SHA256").update("YWJj", "base64").digest("latin1").length);
console.log(crypto.hash("sha1", "abc"), crypto.hash("md5", "abc", "base64"));
const fin = createHash("md5"); fin.digest();
for (const bad of [() => fin.digest(), () => fin.update("x"), () => createHash("nope"), () => createHash("md5").update(5), () => createHmac("sha1")]) {
  try { bad(); } catch (e) { console.log(e.name, e.code, e.message); }
}
console.log(crypto.getHashes().includes("sha256"), crypto.getCiphers().includes("aes-128-ccm"));

const key16 = Buffer.from("000102030405060708090a0b0c0d0e0f", "hex");
const key24 = Buffer.from("000102030405060708090a0b0c0d0e0f1011121314151617", "hex");
const key32 = Buffer.from("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f", "hex");
const iv = Buffer.from("0f0e0d0c0b0a09080706050403020100", "hex");
const plain = Buffer.from("00112233445566778899aabbccddeeff", "hex");
const run = (alg, key, ivv, data, pad = true) => {
  const c = createCipheriv(alg, key, ivv); c.setAutoPadding(pad);
  const enc = Buffer.concat([c.update(data), c.final()]);
  const d = createDecipheriv(alg, key, ivv); d.setAutoPadding(pad);
  const dec = Buffer.concat([d.update(enc), d.final()]);
  console.log(alg, enc.toString("hex"), dec.equals(data));
};
for (const [n, key] of [[128, key16], [192, key24], [256, key32]]) {
  run(`aes-${n}-ecb`, key, null, plain, false);
  run(`aes-${n}-ecb`, key, null, Buffer.from("hello world"));
  run(`aes-${n}-cbc`, key, iv, Buffer.from("a longer message spanning several AES blocks!"));
  run(`aes-${n}-ctr`, key, iv, Buffer.from("counter mode, any length"));
}
const ecb = createCipheriv("aes-128-ecb", key16, null);
console.log(ecb.update("0011", "hex", "hex"), ecb.update(plain.subarray(2)).toString("hex"), ecb.final("hex"));
const nonce = Buffer.alloc(13, 7);
const ccm = createCipheriv("aes-128-ccm", key16, nonce, { authTagLength: 16 });
const ccmEnc = Buffer.concat([ccm.update(Buffer.from("security key 16!")), ccm.final()]);
const tag = ccm.getAuthTag();
console.log("ccm", ccmEnc.toString("hex"), tag.toString("hex"));
const ccm2 = createCipheriv("aes-128-ccm", key16, Buffer.alloc(7, 1), { authTagLength: 8 });
ccm2.setAAD(Buffer.from("header"), { plaintextLength: 5 });
const enc2 = Buffer.concat([ccm2.update("hello"), ccm2.final()]); const tag2 = ccm2.getAuthTag();
console.log("ccm-aad", enc2.toString("hex"), tag2.toString("hex"));
const dccm = createDecipheriv("aes-128-ccm", key16, Buffer.alloc(7, 1), { authTagLength: 8 });
dccm.setAuthTag(tag2); dccm.setAAD(Buffer.from("header"), { plaintextLength: 5 });
console.log(Buffer.concat([dccm.update(enc2), dccm.final()]).toString());
const bad = createDecipheriv("aes-128-ccm", key16, Buffer.alloc(7, 1), { authTagLength: 8 });
bad.setAuthTag(Buffer.alloc(8)); bad.setAAD(Buffer.from("header"), { plaintextLength: 5 });
try { bad.update(enc2); bad.final(); } catch (e) { console.log(e.name, e.message); }
const badPad = createDecipheriv("aes-128-cbc", key16, iv);
try { badPad.update(Buffer.alloc(16, 1)); badPad.final(); } catch (e) { console.log(e.name, e.code, e.message); }
for (const f of [() => createCipheriv("aes-128-cbc", key16, Buffer.alloc(3)), () => createCipheriv("aes-128-cbc", Buffer.alloc(5), iv), () => createCipheriv("foo-bar", key16, iv), () => createCipheriv("aes-128-ccm", key16, nonce)]) {
  try { f(); } catch (e) { console.log(e.name, e.code, e.message); }
}
const nopad = createCipheriv("aes-128-cbc", key16, iv); nopad.setAutoPadding(false); nopad.update("abc");
try { nopad.final(); } catch (e) { console.log(e.name, e.code, e.message); }
console.log(crypto.timingSafeEqual(Buffer.from("ab"), Buffer.from("ab")), crypto.timingSafeEqual(new Uint8Array([1]), new Uint8Array([2])));
try { crypto.timingSafeEqual(Buffer.from("a"), Buffer.from("ab")); } catch (e) { console.log(e.name, e.code, e.message); }
