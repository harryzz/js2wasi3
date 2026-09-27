// T20 slice 4a: a user world (`--wit world.wit`) decides the types at the boundary.
export function addU8(a, b) {
  return a + b; // wraps to 8 bits
}
export function negS8(a) {
  return -a;
}
export function wrapS16(a) {
  return a;
}
export function mulU16(a, b) {
  return a * b;
}
export function toS32(x) {
  return x;
}
export function toU32(x) {
  return x;
}
export function big(a, b) {
  return a + b;
}
export function half(x) {
  return x / 2;
}
function not_(b) {
  return !b;
}
export { not_ as "not" };
export function truthy(s) {
  return s;
}
export function nextChar(c) {
  return String.fromCodePoint(c.codePointAt(0) + 1);
}
export function first(s) {
  return s;
}
export function describe(n, c, ok) {
  return `${n} ${c} ${ok} ${typeof n} ${typeof c} ${typeof ok}`;
}
export function nothing(x) {
  return x;
}
export async function slowAdd(a, b) {
  await null;
  return a + b;
}
export async function slowChar(s) {
  await null;
  return s;
}
export function fails(x) {
  if (x > 0) throw new Error("boom");
  return x;
}
