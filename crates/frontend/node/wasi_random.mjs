// `wasi:random` for `node:crypto` (T65): cryptographically secure bytes from the host.
import { getRandomBytes as $getRandomBytes } from "wasi:random/random@0.3.1";

export const getRandomBytes = $getRandomBytes;
