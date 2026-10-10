// T102: array/string iteration skips the protocol only while the built-ins it relies on are untouched.
'use strict';
const show = (label, f) => {
  try {
    console.log(label, JSON.stringify(f()));
  } catch (e) {
    console.log(label, 'throws', e.constructor.name, e.message);
  }
};
const forOf = (v) => { const r = []; for (const x of v) r.push(x); return r; };
const all = (tag) => {
  show(tag + ' for-of', () => forOf([1, 2, 3]));
  show(tag + ' spread', () => [...[4, 5]]);
  show(tag + ' destructure', () => { const [a, b] = [6, 7]; return [a, b]; });
  show(tag + ' Array.from', () => Array.from([8, 9]));
  show(tag + ' Set', () => [...new Set([1, 1, 2])]);
  show(tag + ' string', () => forOf('ab'));
  show(tag + ' string spread', () => [...'cd']);
};
all('plain');

// Per-object cases (no built-in changes).
const own = [1, 2];
own[Symbol.iterator] = function* () { yield 'own'; };
show('own iterator', () => forOf(own));
// Another prototype (subclass instances get `Array.prototype` today: F106).
const P = Object.create(Array.prototype);
Object.defineProperty(P, Symbol.iterator, { value: function* () { yield 'proto2'; } });
const other = Object.setPrototypeOf([1, 2], P);
show('other prototype', () => forOf(other));
const m = /(b)/.exec('abc');
show('match array', () => forOf(m));
all('after per-object');

// %ArrayIteratorPrototype%.next replaced, then restored.
const AIP = Object.getPrototypeOf([][Symbol.iterator]());
const next = AIP.next;
AIP.next = function () { return { done: true, value: undefined }; };
all('next replaced');
AIP.next = next;
all('next restored');

// Array.prototype[Symbol.iterator]: replaced, deleted, redefined.
const values = Array.prototype[Symbol.iterator];
Array.prototype[Symbol.iterator] = function* () { yield 'proto'; };
all('array iterator replaced');
delete Array.prototype[Symbol.iterator];
show('deleted for-of', () => forOf([1]));
show('deleted spread', () => [...[1]]);
Object.defineProperty(Array.prototype, Symbol.iterator, { value: values, writable: true, configurable: true });
all('array iterator redefined');

// String.prototype[Symbol.iterator].
const strIter = String.prototype[Symbol.iterator];
String.prototype[Symbol.iterator] = function* () { yield 'str'; };
show('string replaced', () => forOf('ab'));
show('string replaced spread', () => [...'ab']);
show('arrays unaffected', () => forOf([1]));
String.prototype[Symbol.iterator] = strIter;
all('string restored');

// A getter for `next`, then the original back.
let reads = 0;
Object.defineProperty(AIP, 'next', { get() { reads++; return next; }, configurable: true });
show('next getter', () => forOf([1, 2]));
console.log('next getter reads', reads);
Object.defineProperty(AIP, 'next', { value: next, writable: true, configurable: true });
Object.freeze(Array.prototype);
all('frozen');
