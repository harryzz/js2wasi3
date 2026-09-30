// require() of ES modules (T54, node 24): evaluated synchronously; the namespace (with
// `__esModule: true` when it has a default export), or its "module.exports" export; a graph
// with top-level await is ERR_REQUIRE_ASYNC_MODULE and runs nothing; a failed module's error
// is thrown again without running it again.
console.log('main');
const a = require('./a.mjs');
console.log(a, require('./a.mjs') === a, Object.keys(a));
const b = require('./b.mjs');
console.log(b, b.__esModule);
console.log(require('./me.mjs'));
try {
  require('./tla.mjs');
} catch (e) {
  console.log(e.code, e.message.split('\n')[0]);
}
try { require('./throws.mjs'); } catch (e) { console.log('1', e.message); }
try { require('./throws.mjs'); } catch (e) { console.log('2', e.message); }
console.log(require.resolve('./b.mjs') === __dirname + '/b.mjs');
