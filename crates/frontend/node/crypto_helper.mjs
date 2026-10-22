// The crypto helper component's functions (T149): RustCrypto in a Wasm component composed with the
// program (`build` composes it, `compose.rs`). Linked only into programs that name `pbkdf2`, `scrypt` or `hkdf`.
import { pbkdf2 as $pbkdf2, scrypt as $scrypt, hkdf as $hkdf } from "js2wasi3:crypto/primitives@0.1.0";

export const pbkdf2 = $pbkdf2;
export const scrypt = $scrypt;
export const hkdf = $hkdf;
