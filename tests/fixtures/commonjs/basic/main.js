// CommonJS basics (T54): require, exports, module.exports, this, the wrapper's arguments.
const counter = require('./counter');
const { add, twice } = require('./math.js');
const same = require('./counter.js');

console.log(counter.next(), counter.next(), same === counter, same.next());
console.log(add(2, 3), twice(add)(1, 2));
console.log(this === module.exports, this === exports, typeof require, typeof module);
console.log(arguments.length, typeof arguments[1], arguments[2] === module);
console.log(__filename.endsWith('/basic/main.js'), __dirname.endsWith('/commonjs/basic'), __filename.startsWith(__dirname + '/'));
console.log(require.main === module, module.id, module.loaded, module.children.length);
console.log(module.children.map((c) => c.id.slice(c.id.lastIndexOf('/') + 1)), module.children[0].loaded);
console.log(Object.keys(module), module.parent, module.children[0].parent === module);
console.log(require.resolve('./math').endsWith('/basic/math.js'), module.paths.at(-1), module.paths[0].endsWith('/basic/node_modules'));
exports.late = 1;
console.log(module.exports);
if (counter.value() > 0) return;
console.log('not reached');
