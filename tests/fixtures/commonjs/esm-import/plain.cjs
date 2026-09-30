console.log('plain.cjs runs', module.parent, require.main);
exports.a = 1;
module.exports.b = 2;
exports['d-e'] = 3;
Object.defineProperty(exports, 'getter', { enumerable: true, get() { return 4; } });
if (false) exports.dead = 5;
