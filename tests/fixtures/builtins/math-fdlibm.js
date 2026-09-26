// Math: the fdlibm functions on special and ordinary values (T21).
const specials = [0, -0, NaN, Infinity, -Infinity, 1, -1, 0.5, -0.5, 2, 1e-300, 5e-324, 1e300];
const fns = ["sin", "cos", "tan", "asin", "acos", "atan", "exp", "expm1", "log", "log1p",
  "log2", "log10", "cbrt", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh"];
const impl = [Math.sin, Math.cos, Math.tan, Math.asin, Math.acos, Math.atan, Math.exp,
  Math.expm1, Math.log, Math.log1p, Math.log2, Math.log10, Math.cbrt, Math.sinh, Math.cosh,
  Math.tanh, Math.asinh, Math.acosh, Math.atanh];
for (let i = 0; i < fns.length; i++) {
  const f = impl[i];
  let line = fns[i];
  for (const x of specials) {
    line += " " + f(x);
  }
  console.log(line);
}

// Ordinary values across the ranges each reduction step handles.
const xs = [0.1, 0.3, 0.7, 0.9, 1.1, 1.5, 2.4, 3, Math.PI / 4, Math.PI / 2, Math.PI, 10, 22.5,
  100, 709.5, 710, -3.7, -0.2, 1e5, 1e10, 1e22, 1e300, 1000, 1024, 3.0000000000000004];
for (let i = 0; i < fns.length; i++) {
  const f = impl[i];
  let line = fns[i];
  for (const x of xs) {
    line += " " + f(x);
  }
  console.log(line);
}

console.log(Math.atan2(0, 0), Math.atan2(-0, 0), Math.atan2(0, -0), Math.atan2(-0, -0),
  Math.atan2(1, 0), Math.atan2(-1, 0), Math.atan2(Infinity, Infinity),
  Math.atan2(-Infinity, -Infinity), Math.atan2(1, Infinity), Math.atan2(1, -Infinity),
  Math.atan2(1, 2), Math.atan2(-3, -4), Math.atan2(NaN, 1), Math.atan2(1e300, 1e-300),
  Math.atan2(1e-300, -1e300));
console.log(Math.exp(1), Math.log(Math.E), Math.log10(1000), Math.log2(8), Math.log2(1 / 1024));
