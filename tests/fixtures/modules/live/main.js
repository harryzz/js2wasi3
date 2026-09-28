// Live bindings: an importer sees the exporter's later assignments.
import { count, increment, reset, name as label } from './counter.js';

console.log(count, label);
increment();
increment();
console.log(count);
reset();
console.log(count);
const read = () => count;
increment();
console.log(read(), typeof increment);
// Imports are read-only: the value is computed, then the write throws.
for (const write of [() => (count = 5), () => count++, () => ([label] = ['x'])]) {
  try {
    write();
  } catch (e) {
    console.log(e.name, e.message, count, label);
  }
}
