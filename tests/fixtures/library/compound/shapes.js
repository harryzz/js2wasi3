// T20 slice 4b-1: compound WIT types with jco's JS shapes.
export function norm(p) {
  return Math.sqrt(p.x * p.x + p.y * p.y);
}
export function makePoint(x, y) {
  return { x, y };
}
export function describe(p) {
  return `${p.label}@${p.at.x},${p.at.y} max=${p.maxSize}`;
}
export function relabel(p, label) {
  return { ...p, label, maxSize: p.maxSize === undefined ? 7 : undefined };
}
const colors = ["red", "green", "dark-blue"];
export function nextColor(c) {
  return colors[(colors.indexOf(c) + 1) % 3];
}
export function permsOf(p) {
  return { read: !p.read, write: p.write, execNow: true };
}
export function swap([s, n]) {
  return [n, s];
}
export function maybeHalf(x) {
  return x === undefined ? undefined : x / 2;
}
export function deep(x) {
  return JSON.stringify(x) ?? "undefined";
}
export function parse(s) {
  const n = Number(s);
  if (Number.isNaN(n)) throw `not a number: ${s}`;
  return n;
}
export function check(n) {
  if (n < 0) throw "dark-blue";
}
export function unitResult(ok) {
  if (!ok) throw { payload: undefined };
}
export function nested(r) {
  if (r.tag === "ok") return r.val === "" ? undefined : r.val;
  const e = new Error("tuple");
  e.payload = [r.val, r.val + 1];
  throw e;
}
export async function slowPoint(x) {
  await null;
  if (x < 0) throw "negative";
  return { x, y: -x };
}
export function badColor() {
  return "purple";
}
export function errorThrown() {
  throw new Error("an Error without payload traps");
}
