// node's CommonJS resolution of relative specifiers: the file, .js, .json, a directory's
// package.json "main", index.js; JSON files; each file is one module whatever the specifier.
const data = require('./data.json');
console.log(data, require('./data') === data, data.list.length);
console.log(require('./lib'), require('./lib/') === require('./lib/index.js'));
console.log(require('./pkgdir'), require('./pkgdir/src/entry.js') === require('./pkgdir'));
console.log(require('./noext'), require('./proto.json').__proto__, Object.getPrototypeOf(require('./proto.json')) === Object.prototype);
console.log(Object.keys(require('./proto.json')), require('./scalar.json'));
data.mutated = true;
console.log(require('./data.json').mutated);
console.log(require('./lib/dot.js'));
