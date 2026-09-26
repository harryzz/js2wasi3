// An async function that throws with nobody awaiting it: an unhandled rejection ends the program.
async function work(n) {
  console.log("working", n);
  await null;
  if (n > 1) throw new TypeError(`bad n: ${n}`);
  return n;
}
work(1).then((v) => console.log("ok", v));
work(2);
console.log("sync end");
