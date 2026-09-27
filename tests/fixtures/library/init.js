// T20 (F11): a library's top-level code runs once, before the first export call.
const squares = [];
for (let i = 0; i < 5; i++) squares.push(i * i);
let greeting = "hello";
greeting += ", world";
const byName = new Map([
  ["a", 1],
  ["b", 2],
]);
class Point {
  constructor(x, y) {
    this.x = x;
    this.y = y;
  }
  norm() {
    return Math.hypot(this.x, this.y);
  }
}
const origin = new Point(3, 4);
let calls = 0;

export function square(i) {
  return squares[i];
}
/** @returns {string} */
export function greet() {
  return greeting;
}
/** @param {string} k */
export function lookup(k) {
  return byName.get(k) ?? -1;
}
export function norm() {
  return origin.norm();
}
export function count() {
  calls++;
  return calls;
}
