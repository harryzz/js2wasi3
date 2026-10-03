#!/usr/bin/env python3
"""T64: port node 24.21.0's url and querystring modules to js2wasi3 built-in modules.

Usage: port-url.py <dir with node's lib files> (lib_url.js, lib_querystring.js,
lib_internal_querystring.js, lib_internal_url.js: `lib/<path>` with `/` as `_`, from
https://raw.githubusercontent.com/nodejs/node/v24.21.0/). Writes crates/frontend/node/{url.js,
querystring.js, internal_querystring.js, internal_url.js}. Primordials become local functions,
internal requires become the modules below or local definitions; every other change is a
replacement listed here (each must match, or the script fails).
"""
import os
import re
import sys

SRC = sys.argv[1]
OUT = os.path.join(os.path.dirname(__file__), '..', '..', 'crates', 'frontend', 'node')

PRIMORDIALS = {
    'ArrayIsArray': 'const ArrayIsArray = (a) => Array.isArray(a);',
    'ArrayPrototypeJoin': 'const ArrayPrototypeJoin = (a, s) => a.join(s);',
    'ArrayPrototypeMap': 'const ArrayPrototypeMap = (a, f) => a.map(f);',
    'ArrayPrototypePush': 'const ArrayPrototypePush = (a, ...xs) => a.push(...xs);',
    'MathAbs': 'const MathAbs = (x) => Math.abs(x);',
    'NumberIsFinite': 'const NumberIsFinite = (x) => Number.isFinite(x);',
    'NumberPrototypeToString': 'const NumberPrototypeToString = (n, r) => n.toString(r);',
    'ObjectAssign': 'const ObjectAssign = (t, ...s) => Object.assign(t, ...s);',
    'ObjectDefineProperties': 'const ObjectDefineProperties = (o, p) => Object.defineProperties(o, p);',
    'ObjectKeys': 'const ObjectKeys = (o) => Object.keys(o);',
    'ReflectGetOwnPropertyDescriptor': 'const ReflectGetOwnPropertyDescriptor = (o, k) => Reflect.getOwnPropertyDescriptor(o, k);',
    'ReflectOwnKeys': 'const ReflectOwnKeys = (o) => Reflect.ownKeys(o);',
    'SafeMap': 'const SafeMap = Map;',
    'SafeSet': 'const SafeSet = Set;',
    'StringPrototypeAt': 'const StringPrototypeAt = (s, i) => s.at(i);',
    'StringPrototypeCharAt': 'const StringPrototypeCharAt = (s, i) => s.charAt(i);',
    'StringPrototypeCharCodeAt': 'const StringPrototypeCharCodeAt = (s, i) => s.charCodeAt(i);',
    'StringPrototypeCodePointAt': 'const StringPrototypeCodePointAt = (s, i) => s.codePointAt(i);',
    'StringPrototypeIncludes': 'const StringPrototypeIncludes = (s, x) => s.includes(x);',
    'StringPrototypeIndexOf': 'const StringPrototypeIndexOf = (s, x, i) => s.indexOf(x, i);',
    'StringPrototypeReplaceAll': 'const StringPrototypeReplaceAll = (s, a, b) => s.replaceAll(a, b);',
    'StringPrototypeSlice': 'const StringPrototypeSlice = (s, a, b) => s.slice(a, b);',
    'StringPrototypeStartsWith': 'const StringPrototypeStartsWith = (s, x) => s.startsWith(x);',
    'StringPrototypeToUpperCase': 'const StringPrototypeToUpperCase = (s) => s.toUpperCase();',
    'StringPrototypeToWellFormed': 'const StringPrototypeToWellFormed = (s) => s.toWellFormed();',
    'SymbolIterator': 'const SymbolIterator = Symbol.iterator;',
    'SymbolToStringTag': 'const SymbolToStringTag = Symbol.toStringTag;',
    'TypedArrayPrototypeGetBuffer': 'const TypedArrayPrototypeGetBuffer = (a) => a.buffer;',
    'TypedArrayPrototypeGetByteLength': 'const TypedArrayPrototypeGetByteLength = (a) => a.byteLength;',
    'TypedArrayPrototypeGetByteOffset': 'const TypedArrayPrototypeGetByteOffset = (a) => a.byteOffset;',
}
# The globals themselves.
GLOBALS = {'Array', 'Boolean', 'Int8Array', 'Number', 'String', 'Symbol', 'decodeURIComponent'}
# Replaced by an edit below.
DROPPED = {'IteratorPrototype', 'ObjectSetPrototypeOf'}

# node's errors used by these modules: `ERR_X(args)` (called without `new`).
ERRORS = r'''
function codeError(Base, code, message) {
  const e = new Base(message);
  e.code = code;
  return e;
}
const typeNames = (expected) => {
  const types = [], instances = [];
  for (const e of Array.isArray(expected) ? expected : [expected]) {
    if (e[0] === e[0].toUpperCase() && e !== 'Object') instances.push(e);
    else types.push(e === 'Object' ? 'object' : e);
  }
  let msg = '';
  if (types.length > 0) msg += 'of type ' + (types.length > 1 ? types.slice(0, -1).join(', ') + ' or ' + types[types.length - 1] : types[0]);
  if (instances.length > 0) msg += (msg !== '' ? ' or ' : '') + 'an instance of ' + instances.join(' or ');
  return msg;
};
const ERR_INVALID_ARG_TYPE = (name, expected, actual) => $$errInvalidArgType(name, typeNames(expected), actual);
const ERR_INVALID_ARG_VALUE = (name, value, reason = 'is invalid') =>
  codeError(TypeError, 'ERR_INVALID_ARG_VALUE', `The ${name.includes('.') ? 'property' : 'argument'} '${name}' ${reason}. Received ${$$inspectValue(value)}`);
const ERR_ARG_NOT_ITERABLE = (name) => codeError(TypeError, 'ERR_ARG_NOT_ITERABLE', `${name} must be iterable`);
const ERR_INVALID_FILE_URL_HOST = (platform) => codeError(TypeError, 'ERR_INVALID_FILE_URL_HOST', `File URL host must be "localhost" or empty on ${platform}`);
const ERR_INVALID_FILE_URL_PATH = (msg) => codeError(TypeError, 'ERR_INVALID_FILE_URL_PATH', `File URL path ${msg}`);
const ERR_INVALID_THIS = (type) => codeError(TypeError, 'ERR_INVALID_THIS', `Value of "this" must be of type ${type}`);
const ERR_INVALID_TUPLE = (name, value) => codeError(TypeError, 'ERR_INVALID_TUPLE', `${name} must be an iterable ${value} tuple`);
const ERR_INVALID_URI = () => codeError(URIError, 'ERR_INVALID_URI', 'URI malformed');
const ERR_INVALID_URL = (input, base = null) => {
  const e = codeError(TypeError, 'ERR_INVALID_URL', 'Invalid URL');
  e.input = input;
  if (base != null) e.base = base;
  return e;
};
const ERR_INVALID_URL_SCHEME = (expected) => codeError(TypeError, 'ERR_INVALID_URL_SCHEME', `The URL must be of scheme ${expected}`);
const ERR_MISSING_ARGS = (...names) => codeError(TypeError, 'ERR_MISSING_ARGS',
  `The ${names.map((n) => `"${n}"`).join(names.length > 2 ? ', ' : ' and ')} argument${names.length > 1 ? 's' : ''} must be specified`);
const validateString = (value, name) => {
  if (typeof value !== 'string') throw ERR_INVALID_ARG_TYPE(name, 'string', value);
};
const validateObject = (value, name) => {
  if (value === null || typeof value !== 'object') throw ERR_INVALID_ARG_TYPE(name, 'Object', value);
};
const validateFunction = (value, name) => {
  if (typeof value !== 'function') throw ERR_INVALID_ARG_TYPE(name, 'function', value);
};
'''

CHARS = r'''
const CHAR_SPACE = 32, CHAR_TAB = 9, CHAR_CARRIAGE_RETURN = 13, CHAR_LINE_FEED = 10, CHAR_NO_BREAK_SPACE = 160,
  CHAR_ZERO_WIDTH_NOBREAK_SPACE = 65279, CHAR_HASH = 35, CHAR_FORWARD_SLASH = 47, CHAR_LEFT_SQUARE_BRACKET = 91,
  CHAR_RIGHT_SQUARE_BRACKET = 93, CHAR_LEFT_ANGLE_BRACKET = 60, CHAR_RIGHT_ANGLE_BRACKET = 62,
  CHAR_LEFT_CURLY_BRACKET = 123, CHAR_RIGHT_CURLY_BRACKET = 125, CHAR_QUESTION_MARK = 63, CHAR_DOUBLE_QUOTE = 34,
  CHAR_SINGLE_QUOTE = 39, CHAR_PERCENT = 37, CHAR_SEMICOLON = 59, CHAR_BACKWARD_SLASH = 92,
  CHAR_CIRCUMFLEX_ACCENT = 94, CHAR_GRAVE_ACCENT = 96, CHAR_VERTICAL_LINE = 124, CHAR_AT = 64, CHAR_COLON = 58,
  CHAR_LOWERCASE_A = 97, CHAR_LOWERCASE_Z = 122;
'''


def primordials(src, name):
    m = re.search(r"const \{\n((?:  \w+,\n)+)\} = primordials;\n", src)
    if not m:
        sys.exit(f'{name}: no primordials block')
    names = re.findall(r'(\w+),', m.group(1))
    out = []
    for n in names:
        if n in GLOBALS or n in DROPPED:
            continue
        if n not in PRIMORDIALS:
            sys.exit(f'{name}: unknown primordial {n}')
        out.append(PRIMORDIALS[n])
    return src[:m.start()] + '// js2wasi3: primordials as local functions.\n' + '\n'.join(out) + '\n' + src[m.end():]


def edit(src, name, old, new, count=1):
    n = src.count(old)
    if n != count:
        sys.exit(f'{name}: expected {count} of {old!r}, found {n}')
    return src.replace(old, new)


def errors_without_new(src):
    return re.sub(r'new (ERR_\w+)\(', r'\1(', src)


def header(name, task='T64'):
    return (f"// From node 24.21.0's lib/{name} (MIT, Node.js contributors; see the license at the top where\n"
            f"// node has it), adapted by js2wasi3 ({task}) with tasks/T64-files/port-url.py; changes are marked \"js2wasi3\".\n")


def read(name):
    with open(os.path.join(SRC, name)) as f:
        return f.read()


def write(name, src):
    with open(os.path.join(OUT, name), 'w') as f:
        f.write(src)


# --- internal/querystring ---
n = 'internal/querystring.js'
s = primordials(read('lib_internal_querystring.js'), n)
s = edit(s, n, "const { ERR_INVALID_URI } = require('internal/errors').codes;\n",
         "// js2wasi3: node's error.\nconst ERR_INVALID_URI = () => { const e = new URIError('URI malformed'); e.code = 'ERR_INVALID_URI'; return e; };\n")
s = errors_without_new(s)
write('internal_querystring.js', header(n) + s)

# --- querystring ---
n = 'querystring.js'
s = primordials(read('lib_querystring.js'), n)
s = edit(s, n, "const { Buffer } = require('buffer');\n", "")
write('querystring.js', header(n) + s)

# --- internal/url ---
n = 'internal/url.js'
s = primordials(read('lib_internal_url.js'), n)
s = edit(s, n, "const { URLPattern } = internalBinding('url_pattern');\n", "")
s = edit(s, n, "const { inspect } = require('internal/util/inspect');\n",
         "// js2wasi3: the formatter's `inspect` (depth only).\n"
         "const inspect = (v, opts) => $$inspectValue(v, opts);\n"
         "inspect.custom = Symbol.for('nodejs.util.inspect.custom');\n")
s = re.sub(r"const \{\n  getConstructorOf,\n.*?\} = require\('internal/util'\);\n", lambda m: (
    "// js2wasi3: node's internal/util helpers.\n"
    "const getConstructorOf = (obj) => {\n"
    "  while (obj) {\n"
    "    const d = Object.getOwnPropertyDescriptor(obj, 'constructor');\n"
    "    if (d !== undefined && typeof d.value === 'function' && d.value.name !== '') return d.value;\n"
    "    obj = Object.getPrototypeOf(obj);\n"
    "  }\n"
    "  return null;\n"
    "};\n"
    "const removeColors = (s) => s;\n"
    "const kEnumerableProperty = { __proto__: null, enumerable: true };\n"
    "const kEmptyObject = Object.freeze({ __proto__: null });\n"
    "const SideEffectFreeRegExpPrototypeSymbolReplace = (re, s, r) => s.replace(re, r);\n"
    "const isWindows = process.platform === 'win32';\n"), s, count=1, flags=re.S)
s = edit(s, n, "const {\n  platform,\n} = require('internal/process/per_thread');\n", "const platform = process.platform;\n")
s = edit(s, n, "const {\n  markTransferMode,\n} = require('internal/worker/js_transferable');\n", "")
s = edit(s, n, "    markTransferMode(this, false, false);\n", "", count=2)
s = re.sub(r"const \{\n  codes: \{\n.*?\} = require\('internal/errors'\);\n", lambda m: ERRORS, s, count=1, flags=re.S)
s = re.sub(r"const \{\n  CHAR_BACKWARD_SLASH,\n.*?\} = require\('internal/constants'\);\n", lambda m: CHARS, s, count=1, flags=re.S)
s = edit(s, n, "const { Buffer } = require('buffer');\n", "")
s = edit(s, n, "const {\n  validateFunction,\n  validateObject,\n  kValidateObjectAllowObjects,\n} = require('internal/validators');\n",
         "const kValidateObjectAllowObjects = 0;\n")
s = edit(s, n, "const { percentDecode } = require('internal/data_url');\n",
         "// js2wasi3: node's `percentDecode` (internal/data_url).\n"
         "function percentDecode(input) {\n"
         "  const out = [];\n"
         "  for (let i = 0; i < input.length; i++) {\n"
         "    const b = input[i];\n"
         "    const hex = (c) => (c >= 48 && c <= 57) || (c >= 65 && c <= 70) || (c >= 97 && c <= 102);\n"
         "    if (b === 37 && i + 2 < input.length && hex(input[i + 1]) && hex(input[i + 2])) {\n"
         "      out.push(parseInt(String.fromCharCode(input[i + 1], input[i + 2]), 16));\n"
         "      i += 2;\n"
         "    } else {\n"
         "      out.push(b);\n"
         "    }\n"
         "  }\n"
         "  return new Uint8Array(out);\n"
         "}\n")
s = edit(s, n, "const bindingUrl = internalBinding('url');\n",
         "// js2wasi3: the parser node has in C++ (ada).\nconst bindingUrl = require('internal/url_binding');\n")
s = re.sub(r"let blob;\nlet cryptoRandom;\n\nfunction lazyBlob\(\) \{.*?\n\}\n\nfunction lazyCryptoRandom\(\) \{.*?\n\}\n", "", s, count=1, flags=re.S)
s = edit(s, n, "// https://heycam.github.io/webidl/#dfn-iterator-prototype-object\n"
               "delete URLSearchParamsIterator.prototype.constructor;\n"
               "ObjectSetPrototypeOf(URLSearchParamsIterator.prototype, IteratorPrototype);\n",
         "// https://heycam.github.io/webidl/#dfn-iterator-prototype-object\n"
         "// js2wasi3: `[Symbol.iterator]` of %IteratorPrototype% defined here.\n"
         "Object.defineProperty(URLSearchParamsIterator.prototype, SymbolIterator, {\n"
         "  __proto__: null, configurable: true, writable: true, value: function() { return this; },\n"
         "});\n")
# js2wasi3: a plain object for the formatter (node's `{ __proto__: { constructor } }`).
s = edit(s, n, "    const obj = { __proto__: { constructor } };\n", "    const obj = {}; // js2wasi3\n")
s = edit(s, n, "    if (opts.showHidden) {\n      obj[contextForInspect] = this.#context;\n    }\n\n", "")
s = re.sub(r"function installObjectURLMethods\(\) \{.*?\n\}\n\n", "", s, count=1, flags=re.S)
s = edit(s, n, "  installObjectURLMethods,\n", "")
s = edit(s, n, "  URLPattern,\n", "")
s = errors_without_new(s)
write('internal_url.js', header(n) + s)

# --- url ---
n = 'url.js'
s = primordials(read('lib_url.js'), n)
s = edit(s, n, "const { URLPattern } = internalBinding('url_pattern');\n", "")
s = edit(s, n, "const { toASCII } = internalBinding('encoding_binding');\n",
         "// js2wasi3: the parser node has in C++ (ada); its `toASCII` gives '' on failure.\n"
         "const bindingUrl = require('internal/url_binding');\n"
         "const toASCII = (h) => bindingUrl.toASCII(h) ?? '';\n")
s = re.sub(r"const \{\n  ERR_INVALID_ARG_TYPE,\n  ERR_INVALID_URL,\n\} = require\('internal/errors'\).codes;\n", lambda m: ERRORS, s, count=1)
s = edit(s, n, "const {\n  validateString,\n  validateObject,\n} = require('internal/validators');\n", "")
s = edit(s, n, "const { spliceOne } = require('internal/util');\n",
         "const spliceOne = (list, index) => { list.splice(index, 1); }; // js2wasi3\n")
s = edit(s, n, "const { isInsideNodeModules } = internalBinding('util');\n", "const isInsideNodeModules = () => false; // js2wasi3\n")
s = edit(s, n, "const bindingUrl = internalBinding('url');\n", "")
s = re.sub(r"const \{\n  CHAR_SPACE,\n.*?\} = require\('internal/constants'\);\n", lambda m: CHARS, s, count=1, flags=re.S)
s = edit(s, n, "  URLPattern,\n", "")
s = errors_without_new(s)
write('url.js', header(n) + s)
print('ok')
