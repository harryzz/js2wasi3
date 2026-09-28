import { isEven } from './a.js';
export const log = ['b'];
export function isOdd(n) {
  return n === 0 ? false : isEven(n - 1);
}
// a.js hasn't run yet, but its function declarations exist.
console.log('b sees', isEven(4));
