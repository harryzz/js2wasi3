// T59: a command imports a WASI interface directly.
import { getRandomBytes, getRandomU64 } from "wasi:random/random@0.3.1";
const b = getRandomBytes(8);
console.log(b instanceof Uint8Array, b.length, typeof getRandomU64());
