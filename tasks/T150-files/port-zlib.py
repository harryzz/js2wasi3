#!/usr/bin/env python3
"""T150: port node 24.21.0's lib/zlib.js to the js2wasi3 built-in module `node:internal/zlib_impl`.

Usage: port-zlib.py <node's lib/zlib.js> (https://raw.githubusercontent.com/nodejs/node/v24.21.0/lib/zlib.js).
Writes crates/frontend/node/zlib_impl.js. Everything but the header (primordials, internal requires, the native
binding) is node's own code; each replacement must match exactly once, or the script fails.
"""
import os
import sys

SRC = sys.argv[1]
OUT = os.path.join(os.path.dirname(__file__), '..', '..', 'crates', 'frontend', 'node', 'zlib_impl.js')
text = open(SRC).read()


def sub(old, new):
    global text
    if text.count(old) != 1:
        sys.exit(f'port-zlib: expected one match of:\n{old}')
    text = text.replace(old, new)


sub('''const {
  ArrayBuffer,
  MathMax,
  NumberIsNaN,
  ObjectDefineProperties,
  ObjectDefineProperty,
  ObjectEntries,
  ObjectFreeze,
  ObjectKeys,
  ObjectSetPrototypeOf,
  Symbol,
  Uint32Array,
  Uint8Array,
} = primordials;
''', '''// js2wasi3 (T150): the primordials this file uses are the globals.
const MathMax = Math.max;
const NumberIsNaN = Number.isNaN;
const ObjectDefineProperties = Object.defineProperties;
const ObjectDefineProperty = Object.defineProperty;
const ObjectEntries = Object.entries;
const ObjectFreeze = Object.freeze;
const ObjectKeys = Object.keys;
const ObjectSetPrototypeOf = Object.setPrototypeOf;
''')

sub('''const {
  codes: {
    ERR_BROTLI_INVALID_PARAM,
    ERR_BUFFER_TOO_LARGE,
    ERR_INVALID_ARG_TYPE,
    ERR_OUT_OF_RANGE,
    ERR_TRAILING_JUNK_AFTER_STREAM_END,
    ERR_ZSTD_INVALID_PARAM,
  },
  genericNodeError,
} = require('internal/errors');
''', '''const {
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_OUT_OF_RANGE,
  },
} = require('internal/errors');

// node's errors that `internal/errors` (the streams' copy) lacks.
function nodeError(Base, code, format) {
  return class extends Base {
    constructor(...args) {
      super(format(...args));
      this.code = code;
    }
    get name() { return Base.name; }
    toString() { return `${Base.name} [${code}]: ${this.message}`; }
  };
}
const ERR_BROTLI_INVALID_PARAM = nodeError(RangeError, 'ERR_BROTLI_INVALID_PARAM',
  (name) => `${name} is not a valid Brotli parameter`);
const ERR_BUFFER_TOO_LARGE = nodeError(RangeError, 'ERR_BUFFER_TOO_LARGE',
  (max) => `Cannot create a Buffer larger than ${max} bytes`);
const ERR_TRAILING_JUNK_AFTER_STREAM_END = nodeError(TypeError, 'ERR_TRAILING_JUNK_AFTER_STREAM_END',
  () => 'Trailing junk found after the end of the compressed stream');
const ERR_ZSTD_INVALID_PARAM = nodeError(RangeError, 'ERR_ZSTD_INVALID_PARAM',
  (name) => `${name} is not a valid zstd parameter`);

function genericNodeError(message, errorProperties) {
  const err = new Error(message);
  Object.assign(err, errorProperties);
  return err;
}
''')

sub('''const {
  assignFunctionName,
  deprecateInstantiation,
} = require('internal/util');
const {
  isArrayBufferView,
  isAnyArrayBuffer,
  isUint8Array,
} = require('internal/util/types');
const binding = internalBinding('zlib');
const { crc32: crc32Native } = binding;
const assert = require('internal/assert');
''', '''// js2wasi3: node's internal/util helpers (the DEP0184 warning is not emitted).
function assignFunctionName(name, fn) {
  ObjectDefineProperty(fn, 'name', { __proto__: null, value: name, configurable: true });
  return fn;
}
function deprecateInstantiation(Ctor, code, ...args) {
  return new Ctor(...args);
}
const isArrayBufferView = ArrayBuffer.isView;
const isAnyArrayBuffer = (v) => v instanceof ArrayBuffer;  // (no SharedArrayBuffer in js2wasi3)
const isUint8Array = (v) => v instanceof Uint8Array;
const binding = require('node:internal/zlib_binding');
const { crc32: crc32Native } = binding;
function assert(value, message) {
  if (!value) throw new Error(message || 'assertion failed');
}
''')

sub('''const { owner_symbol } = require('internal/async_hooks').symbols;
const {
  checkRangesOrGetDefault,
  validateBoolean,
  validateFunction,
  validateUint32,
  validateFiniteNumber,
} = require('internal/validators');
const { FastBuffer } = require('internal/buffer');
''', '''const owner_symbol = Symbol('owner_symbol');
const {
  validateBoolean,
  validateFunction,
  validateNumber,
  validateUint32,
} = require('internal/validators');
const FastBuffer = function FastBuffer(...args) {
  return args.length === 0 ? Buffer.alloc(0) : Buffer.from(...args);
};

// node's internal/validators, the two this file needs.
function validateFiniteNumber(number, name) {
  // Common case
  if (number === undefined) {
    return false;
  }
  if (Number.isFinite(number)) {
    return true; // Is a valid number
  }
  if (NumberIsNaN(number)) {
    return false;
  }
  validateNumber(number, name);
  // Infinite
  throw new ERR_OUT_OF_RANGE(name, 'a finite number', number);
}

function checkRangesOrGetDefault(number, name, lower, upper, def) {
  if (!validateFiniteNumber(number, name)) {
    return def;
  }
  if (number < lower || number > upper) {
    throw new ERR_OUT_OF_RANGE(name, `>= ${lower} and <= ${upper}`, number);
  }
  return number;
}
''')

sub('''const constants = internalBinding('constants').zlib;''',
    '''const constants = { ...require('node:internal/builtin_data').zlibConstants };''')

open(OUT, 'w').write(text)
print('wrote', os.path.normpath(OUT))
