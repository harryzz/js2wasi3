console.log('a starting');
exports.done = false;
const b = require('./b.js');
console.log('in a, b.done = %j', b.done);
exports.done = true;
exports.b = b;
exports.loaded = module.loaded;
console.log('a done');
