// Named, renamed and default imports; evaluation order (dependencies first, in import order).
import { add, PI as pi, Point } from './math.js';
import greet from './greet.js';
import value, { twice } from './value.js';
import './side.js';

console.log('main runs');
console.log(add(2, 3), pi, twice(value));
console.log(greet('modules'), greet.name);
const p = new Point(1, 2);
console.log(p.sum(), Point.name, p instanceof Point);
