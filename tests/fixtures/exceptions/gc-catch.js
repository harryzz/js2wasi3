// F224: a GC reference assigned inside a `try` and read in its `catch` after a collection during the throwing
// call (wasmtime 49.0.1 lost or corrupted it under both collectors; fixed in 49.0.2, GHSA-hw8m-q44c-ggrf).
function f(c) {
  let r = null;
  try {
    r = [{}];
    c.m();
    return 0;
  } catch (e) {
    return typeof r[0];
  }
}
const c = {
  m() {
    const keep = [];
    for (let i = 0; i < 100000; i++) keep.push({ i });
    throw new Error("x");
  },
};
const out = [];
for (let k = 0; k < 5; k++) out.push(f(c));
console.log(out.join(" "));
