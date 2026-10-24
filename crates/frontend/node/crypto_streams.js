// The stream side of `crypto`'s Hash, Hmac, Cipheriv and Decipheriv (T153): node's `LazyTransform` (lib/internal/streams/lazy_transform.js),
// so the stream is set up only when one of its properties is first used. Linked (`link::link`) only where `node:stream` is in the program
// for another reason; else `crypto_streams_off` is empty and the classes have no stream methods.
'use strict';

const { Transform } = require('stream');

function LazyTransform(options) {
  this._options = options;
}
Object.setPrototypeOf(LazyTransform.prototype, Transform.prototype);
Object.setPrototypeOf(LazyTransform, Transform);

function makeGetter(name) {
  return function() {
    Transform.call(this, this._options);
    this._writableState.decodeStrings = false;
    return this[name];
  };
}

function makeSetter(name) {
  return function(val) {
    Object.defineProperty(this, name, { value: val, enumerable: true, configurable: true, writable: true });
  };
}

Object.defineProperties(LazyTransform.prototype, {
  _readableState: { get: makeGetter('_readableState'), set: makeSetter('_readableState'), configurable: true, enumerable: true },
  _writableState: { get: makeGetter('_writableState'), set: makeSetter('_writableState'), configurable: true, enumerable: true },
});

module.exports = { LazyTransform };
