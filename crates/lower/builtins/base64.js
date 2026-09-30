// `atob`/`btoa` (T56, F237): the HTML spec's forgiving-base64 decode and base64 encode of a
// binary string, with node's errors (a `DOMException` named `InvalidCharacterError`, `ERR_MISSING_ARGS`).
function $$base64Missing() {
  const e = new TypeError('The "input" argument must be specified');
  e.code = "ERR_MISSING_ARGS";
  return e;
}

function $$btoa(...args) {
  if (+args.length === 0) throw $$base64Missing();
  const s = $$String(args[0]);
  const table = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let out = "";
  const n = s.length;
  for (let i = 0; i < n; i++) {
    if (s.charCodeAt(i) > 255) throw $$domException("Invalid character", "InvalidCharacterError");
  }
  let i = 0;
  for (; i + 2 < n; i += 3) {
    const v = (s.charCodeAt(i) << 16) | (s.charCodeAt(i + 1) << 8) | s.charCodeAt(i + 2);
    out += table[(v >> 18) & 63] + table[(v >> 12) & 63] + table[(v >> 6) & 63] + table[v & 63];
  }
  if (n - i === 1) {
    const v = s.charCodeAt(i) << 16;
    out += table[(v >> 18) & 63] + table[(v >> 12) & 63] + "==";
  } else if (n - i === 2) {
    const v = (s.charCodeAt(i) << 16) | (s.charCodeAt(i + 1) << 8);
    out += table[(v >> 18) & 63] + table[(v >> 12) & 63] + table[(v >> 6) & 63] + "=";
  }
  return out;
}

function $$atob(...args) {
  if (+args.length === 0) throw $$base64Missing();
  const input = $$String(args[0]);
  // Remove ASCII whitespace; then at most two trailing `=` if the length is a multiple of 4.
  let s = "";
  for (let i = 0; i < input.length; i++) {
    const c = input.charCodeAt(i);
    if (c === 9 || c === 10 || c === 12 || c === 13 || c === 32) continue;
    s += input[i];
  }
  if (s.length % 4 === 0) {
    if (s.endsWith("==")) s = s.slice(0, -2);
    else if (s.endsWith("=")) s = s.slice(0, -1);
  }
  if (s.length % 4 === 1) {
    throw $$domException("The string to be decoded is not correctly encoded.", "InvalidCharacterError");
  }
  const table = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let out = "";
  let buffer = 0;
  let bits = 0;
  for (let i = 0; i < s.length; i++) {
    const v = table.indexOf(s[i]);
    if (v < 0) throw $$domException("Invalid character", "InvalidCharacterError");
    buffer = (buffer << 6) | v;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out += String.fromCharCode((buffer >> bits) & 255);
    }
  }
  return out;
}
