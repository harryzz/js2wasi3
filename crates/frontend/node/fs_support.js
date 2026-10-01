// node:internal/fs_support (T60): what node 24's lib/internal/fs/streams.js takes from node's
// internal/fs/utils, internal/fs/promises and internal/url (`scripts/gen-node-streams.mjs`).
'use strict';

const { Buffer } = require('node:buffer');
const { codes: { ERR_INVALID_ARG_TYPE, ERR_INVALID_ARG_VALUE } } = require('node:internal/errors');
const { validateInt32, validateAbortSignal } = require('node:internal/validators');

const kEmptyObject = Object.freeze({ __proto__: null });

function copyObject(source) {
  const target = {};
  for (const key in source) target[key] = source[key];
  return target;
}

function assertEncoding(encoding) {
  if (encoding && !Buffer.isEncoding(encoding)) {
    throw new ERR_INVALID_ARG_VALUE('encoding', encoding, 'is invalid encoding');
  }
}

function getOptions(options, defaultOptions = kEmptyObject) {
  if (options == null || typeof options === 'function') {
    return defaultOptions;
  }
  if (typeof options === 'string') {
    defaultOptions = { ...defaultOptions };
    defaultOptions.encoding = options;
    options = defaultOptions;
  } else if (typeof options !== 'object') {
    throw new ERR_INVALID_ARG_TYPE('options', ['string', 'Object'], options);
  }
  if (options.encoding !== 'buffer') assertEncoding(options.encoding);
  if (options.signal !== undefined) validateAbortSignal(options.signal, 'options.signal');
  return options;
}

function getValidatedFd(fd, propName = 'fd') {
  if (Object.is(fd, -0)) return 0;
  validateInt32(fd, propName, 0);
  return fd;
}

function validatePath(path, propName = 'path') {
  if (typeof path !== 'string' && !(path instanceof Uint8Array)) {
    throw new ERR_INVALID_ARG_TYPE(propName, ['string', 'Buffer', 'URL'], path);
  }
  if (typeof path === 'string' ? !path.includes('\u0000') : !path.includes(0)) return;
  throw new ERR_INVALID_ARG_VALUE(propName, path, 'must be a string, Uint8Array, or URL without null bytes');
}

// No `URL` in js2wasi3 yet: paths are strings or Buffers.
function toPathIfFileURL(path) {
  return path;
}

// `fs.promises` `FileHandle`s as a stream's `fd` aren't supported (F252): nothing is one of these.
class FileHandle {}

module.exports = {
  copyObject,
  getOptions,
  getValidatedFd,
  validatePath,
  toPathIfFileURL,
  FileHandle,
  kRef: Symbol('kRef'),
  kUnref: Symbol('kUnref'),
};
