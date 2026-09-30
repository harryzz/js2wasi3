// An ES module importing CommonJS (T54): the default is module.exports, named exports are what
// node's cjs-module-lexer finds (also through tsc's __exportStar), values are snapshots taken
// when the module has run; namespaces have 'module.exports'; import() of CommonJS.
import plain, { a, b, 'd-e' as de } from './plain.cjs';
import * as plainNs from './plain.cjs';
import Lib, * as lib from './ts/lib.js';
import { area, version, PI_ISH } from './ts/lib.js';
import pkg, { helper } from 'cjs-pkg';

console.log('main runs');
console.log(plain, a, b, de, plainNs);
plain.a = 100;
console.log(a, plainNs.a, plainNs.default.a);
console.log(typeof Lib, Lib === lib.default, lib.__esModule, new lib.default.default('esm').greet());
console.log(area({ kind: 'circle', r: 1 }), version, PI_ISH, Object.keys(lib));
console.log(pkg, helper());
const again = await import('./plain.cjs');
console.log(again === plainNs);
const late = await import('./late.cjs');
console.log(late.default, late.value, Object.keys(late));
