// Built-ins in a library (T21): string → number through the runtime, string methods.
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
