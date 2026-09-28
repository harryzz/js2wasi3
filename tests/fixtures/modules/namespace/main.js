// Namespace objects (`import * as ns`, `export * as ns from`), `export *` and re-exports.
import * as ns from './lib.js';
import * as again from './lib.js';
import { tools, sum, Point as P, answer } from './reexports.js';
import * as empty from './empty.js';

console.log(ns);
console.log(ns === again, typeof ns, Object.prototype.toString.call(ns), empty);
console.log(Object.keys(ns), 'count' in ns, 'nope' in ns, ns.nope, Object.getPrototypeOf(ns));
console.log(Object.getOwnPropertyDescriptor(ns, 'count'));
console.log(Object.isExtensible(ns), Object.isSealed(ns), Object.isFrozen(ns), Object.isFrozen(empty));
// Live: the namespace reads the binding.
ns.increment();
console.log(ns.count, ns.self() === ns, JSON.stringify(ns), { ...ns }.count);
for (const op of [() => { ns.count = 2; }, () => { ns.added = 2; }, () => { delete ns.count; }]) {
  try {
    op();
  } catch (e) {
    console.log(`${e.name}: ${e.message}`);
  }
}
console.log(delete ns.nope, Reflect.set(ns, 'count', 1), Reflect.ownKeys(ns));
console.log(tools, sum(1, 2), new P(3).x, answer);
