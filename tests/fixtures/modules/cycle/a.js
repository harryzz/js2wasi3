import { isOdd, log } from './b.js';
log.push('a');
export function isEven(n) {
  return n === 0 ? true : isOdd(n - 1);
}
// b.js ran first (a.js imported it), so its `log` exists here.
console.log('a sees', isOdd(3));
