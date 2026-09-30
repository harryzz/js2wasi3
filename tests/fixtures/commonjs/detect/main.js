// Which files node runs as CommonJS (T54): .cjs, a "type": "commonjs" package's .js, a .js file
// without ESM syntax where there is no "type"; the entry here too (it uses `require`).
const a = require('./esm-pkg/helper.cjs');
const b = require('./cjs-pkg/lib.js');
const c = require('./plain.js');
console.log(a, b, c);
