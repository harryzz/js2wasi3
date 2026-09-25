// Loops: `let` gets a new binding per iteration, `var` is shared.
const lets = {};
for (let i = 0; i < 3; i++) {
  lets[i] = () => i;
}
console.log(lets[0](), lets[1](), lets[2]());

const vars = {};
for (var j = 0; j < 3; j++) {
  vars[j] = () => j;
}
console.log(vars[0](), vars[1](), vars[2]());

// The body sees the value before the update; changes in the body are kept.
const seen = {};
for (let k = 0; k < 6; k++) {
  seen[k] = () => k;
  k++;
}
console.log(seen[0](), seen[2](), seen[4]());

// A `let` in the body is new each time.
const body = {};
let n = 0;
while (n < 3) {
  let m = n * 10;
  body[n] = () => m;
  n++;
}
console.log(body[0](), body[1](), body[2]());

// for-in: `const` per iteration.
const keys = {};
let count = 0;
for (const key in { a: 1, b: 2, c: 3 }) {
  keys[count++] = () => key;
}
console.log(keys[0](), keys[1](), keys[2]());

function inFunction() {
  const fs = {};
  for (let i = 0; i < 3; i++) {
    let sq = i * i;
    fs[i] = function () {
      return i + ":" + sq;
    };
  }
  var total = 0;
  const add = (x) => {
    total += x;
  };
  add(1);
  add(2);
  return fs[0]() + " " + fs[2]() + " " + total;
}
console.log(inFunction());

// Capturing and modifying the loop variable.
const inc = {};
for (let i = 0; i < 3; i++) {
  inc[i] = () => ++i;
}
console.log(inc[0](), inc[0](), inc[1]());
