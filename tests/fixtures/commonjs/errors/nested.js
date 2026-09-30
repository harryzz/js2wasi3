try {
  require('./nope.js');
} catch (e) {
  exports.error = e.message;
  exports.stack = e.requireStack;
}
