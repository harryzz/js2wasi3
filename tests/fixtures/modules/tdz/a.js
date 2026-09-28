import { late as early, lateDefault } from './b.js';
export function read() {
  return early;
}
try {
  console.log(early);
} catch (e) {
  console.log(e.name, e.message);
}
try {
  console.log(lateDefault);
} catch (e) {
  console.log(e.name, e.message);
}
