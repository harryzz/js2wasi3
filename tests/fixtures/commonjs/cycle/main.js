// node's documented cycle: a requires b, b requires a and sees a's partial exports.
console.log('main starting');
const a = require('./a.js');
const b = require('./b.js');
console.log('in main, a.done = %j, b.done = %j', a.done, b.done);
console.log(a.b === b, b.a === a, a.loaded, b.loaded);
