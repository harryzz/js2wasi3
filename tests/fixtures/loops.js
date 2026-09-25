let calls = 0;
const LIMIT = -1;

// sum of odd numbers below n, skipping multiples of 5 (for + continue)
export function sumOdd(n) {
  calls++;
  let s = 0;
  for (let i = 0; i < n; i++) {
    if (i % 2 === 0) continue;
    if (i % 5 === 0) continue;
    s += i;
  }
  return s;
}

// while + labelled break out of a nested loop
export function firstPair(target) {
  let found = LIMIT;
  let i = 0;
  outer: while (i < 10) {
    let j = 0;
    while (true) {
      if (j >= 10) break;
      if (i * j === target) {
        found = i * 10 + j;
        break outer;
      }
      j = j + 1;
    }
    i++;
  }
  return found;
}

// do-while with continue, bitwise ops, ternary and && / || as values
export function mix(a, b) {
  let k = 0;
  let acc = 0;
  do {
    k += 1;
    if (!(k & 1)) continue;
    acc = acc ^ (k << 2) | 1;
  } while (k < 8 && a > 0);
  const m = a > b ? a : b;
  return (acc || -1) + (b && m) + ~a;
}

function helper(x) {
  calls = calls + 1;
}

export function count() {
  helper(1);
  return calls;
}
