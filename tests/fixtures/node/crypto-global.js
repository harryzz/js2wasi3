// The `crypto` global (T65): node's `webcrypto`, with random values and UUIDs; no import names it.
const a = crypto.getRandomValues(new Uint8Array(8));
console.log(typeof crypto, a instanceof Uint8Array, a.length, crypto.getRandomValues(new Int16Array(3)).length);
console.log(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(crypto.randomUUID()));
try { crypto.getRandomValues(new Float32Array(1)); } catch (e) { console.log(e.name, e.code); }
console.log(Object.prototype.toString.call(crypto), typeof crypto.subtle);
