// A library whose exports may throw: they return `result<T, js-error>` at the WIT boundary.
// Checked with --invoke in crates/js2wasi3/tests/build.rs.

function check(x) {
  if (x < 0) throw new RangeError("negative: " + x);
  return x;
}

/** @param {number} x */
export function double(x) {
  return check(x) * 2;
}

/** @param {string} s */
export function shout(s) {
  if (s.length === 0) throw "empty";
  return s + "!";
}

export function fail() {
  throw new TypeError();
}

export function safe(x) {
  try {
    return check(x);
  } catch (e) {
    return -1;
  }
}

/** @param {string} s */
export function firstLength(s) {
  return s.length;
}
