// require of packages (T54): node's CommonJS resolution in node_modules, `exports` with the
// `require` condition, `main`, a subpath without `exports`, scoped packages, `#imports`, errors.
const cond = require('cond');
const feature = require('cond/feature');
console.log(cond, feature, require('cond') === cond);
try {
  require('cond/internal.js');
} catch (e) {
  console.log(e.code, e.message.replace(__dirname, '.'));
}
console.log(require('legacy'), require('legacy/lib/extra'), require('legacy/lib/data.json'));
console.log(require('@scope/pkg'), require('#local'));
try {
  require('not-installed');
} catch (e) {
  console.log(e.code, e.message.split('\n')[0]);
}
let optional = null;
try {
  optional = require('optional-dep');
} catch {}
console.log(optional);
