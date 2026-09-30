'use strict';
function add(a, b) { return a + b; }
module.exports = { add, twice: (f) => (a, b) => 2 * f(a, b) };
console.log('math loaded', this === exports, Object.keys(module.exports));
