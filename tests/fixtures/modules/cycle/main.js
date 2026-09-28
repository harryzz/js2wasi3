// A cycle: a.js and b.js import each other; hoisted functions are callable across it.
import { isEven } from './a.js';
import { log } from './b.js';

console.log(isEven(10), isEven(7));
console.log(log.join(' '));
