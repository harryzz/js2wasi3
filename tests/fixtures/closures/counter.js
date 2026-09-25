// Counters: closures sharing and not sharing captured variables.
function makeCounter(start) {
  let count = start;
  return function () {
    count = count + 1;
    return count;
  };
}

const a = makeCounter(0);
const b = makeCounter(10);
console.log(a(), a(), b(), a(), b());

function pair() {
  let n = 0;
  const inc = () => ++n;
  const get = () => n;
  return { inc, get };
}
const p = pair();
p.inc();
p.inc();
console.log(p.get(), typeof p.inc, typeof pair);

function outer(x) {
  function inner(y) {
    return x + y;
  }
  return inner(1) + inner(2);
}
console.log(outer(10));

const add = (x, y) => x + y;
console.log(add(2, 3), add("a", "b"), add(1));
console.log(add, makeCounter, function () {}, () => 1);
