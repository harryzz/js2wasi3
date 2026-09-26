// Built-ins in a library (T21): string → number through the runtime, string and array methods,
// JSON.stringify and Object statics.
export function times(n) {
  const s = "5";
  return s * n;
}

/**
 * @param {string} s
 * @returns {string}
 */
export function shout(s) {
  return s.trim().toUpperCase().padEnd(6, "!");
}

/** @param {string} s */
export function parse(s) {
  return parseFloat(s) + Number(s.slice(0, 1));
}

/** @param {number} n */
export function evens(n) {
  return Array.from({ length: n }, (_, i) => i)
    .filter((x) => x % 2 === 0)
    .map((x) => x * x)
    .reduce((s, x) => s + x, 0);
}

/**
 * @param {number} n
 * @returns {string}
 */
export function json(n) {
  const o = Object.assign({ n }, Object.fromEntries([["list", [n, "x", null]]]));
  return JSON.stringify(o);
}
