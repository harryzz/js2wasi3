// Built-ins in a library (T21): string → number through the runtime, string and array methods,
// JSON.stringify/parse, Object statics, Map/Set.
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

/**
 * @param {string} s
 * @returns {string}
 */
export function reparse(s) {
  return JSON.stringify(JSON.parse(s, (k, v) => (typeof v === "number" ? v + 1 : v)));
}

/** @param {string} s */
export function distinct(s) {
  const counts = new Map();
  for (const c of s) {
    counts.set(c, (counts.get(c) ?? 0) + 1);
  }
  return new Set(s).size * 100 + counts.get("l");
}

// T26: `Date` in a library: everything but the clock.
/** @returns {string} */
export function isoDay(ms) {
  return new Date(ms).toISOString().slice(0, 10);
}

export function now() {
  return Date.now();
}
