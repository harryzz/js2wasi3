// Errors: a module that isn't there (node's MODULE_NOT_FOUND), a module that throws (dropped from
// the cache, so it runs again), bad ids.
try {
  require('./missing');
} catch (e) {
  console.log(e.name, e.code, e.message.split('\n')[0], e.requireStack.length, e.requireStack[0] === __filename);
}
const nested = require('./nested.js');
console.log(nested.error.split('\n').length, nested.error.split('\n')[1], nested.stack.length);
for (let i = 0; i < 2; i++) {
  try {
    require('./throws.js');
  } catch (e) {
    console.log(i, e.message);
  }
}
for (const id of [123, undefined, null, {}, '']) {
  try {
    require(id);
  } catch (e) {
    console.log(e.name, e.code, e.message);
  }
}
const r = require;
console.log(r('./nested.js') === nested);
