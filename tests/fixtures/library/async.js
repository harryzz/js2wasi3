// T20 slice 3: `export async function` is a WIT `async func`. The call runs until the promise
// settles; its value is the result, a rejection is `err`.
async function double(x) {
  await null;
  return x * 2;
}
export async function add(a, b) {
  const x = await double(a);
  return x + (await Promise.resolve(b));
}
/** @returns {Promise<string>} */
export async function greet(name) {
  await null;
  return "hi " + name;
}
export async function inferred() {
  return "str" + 1;
}
export async function nothing() {
  await null;
}
/** @param {number} x */
export async function fails(x) {
  await null;
  if (x > 0) throw new RangeError("too big: " + x);
  return x;
}
export async function rejectsString() {
  await null;
  throw "plain";
}
export async function never() {
  await new Promise(() => {});
  return 1;
}
async function late() {
  await null;
  throw new TypeError("nobody handled me");
}
export async function strayRejection() {
  late();
  return 5;
}
/** @param {string} s */
export async function len(s) {
  return s.length;
}
let calls = 0;
/** @returns {Promise<number>} */
export default async function counter() {
  calls++;
  await null;
  return calls;
}
