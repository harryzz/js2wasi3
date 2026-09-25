// A library using closures inside its exports (checked with --invoke in crates/js2wasi3/tests/build.rs).
function apply(f, x) {
  return f(x);
}

export function sumSquares(n) {
  let total = 0;
  const add = (k) => {
    total += k * k;
  };
  for (let i = 1; i <= n; i++) add(i);
  return total;
}

export function viaValue(x) {
  return apply((y) => y * 10, x) + apply(half, x);
}

function half(x) {
  return x / 2;
}

/** @returns {string} */
export function self() {
  return this === undefined ? "undefined this" : "other";
}

export function notAFunction(x) {
  return x();
}
