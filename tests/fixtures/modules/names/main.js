// The same top-level names in several modules; function and class names stay as written.
import { helper as h1, Thing as T1, value as v1, getX } from './one.js';
import { helper as h2, Thing as T2, value as v2 } from './two.js';

function helper() {
  return 'main helper';
}
class Thing {}
const value = 'main';
let x = 'main x';
console.log(helper(), h1(), h2());
console.log(helper.name, h1.name, h2.name, Thing.name, T1.name, T2.name);
console.log(value, v1, v2, x, getX());
console.log(h1, T2);
const o = { h1, v2 };
console.log(o, Object.keys({ helper }));
function shadow(helper) {
  // `helper` here is the parameter; one.js's `helper` is still reachable as h1.
  return [helper, h1()];
}
console.log(shadow('param'));
