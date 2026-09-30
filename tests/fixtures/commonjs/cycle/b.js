console.log('b starting');
exports.done = false;
const a = require('./a.js');
console.log('in b, a.done = %j', a.done);
exports.done = true;
exports.a = a;
exports.loaded = module.loaded;
console.log('b done');
