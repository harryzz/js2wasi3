// T07: JS export names → WIT names (docs/wit-names.md).
export function sumOdd(n, _n) {
  let s = 0;
  for (let i = 1; i <= n; i += 2) s += i;
  return s;
}
function _x(a) {
  return a * 10;
}
export { _x as "times-ten" };
export default function type(a) {
  return -a;
}
