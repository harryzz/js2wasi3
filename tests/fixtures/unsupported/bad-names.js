function f() {
  return 1;
}
export function fooBar() {
  return 1;
}
export { f as foo_bar, f as _1, f as "bad name" };
