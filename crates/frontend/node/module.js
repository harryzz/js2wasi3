// node's `module` (T65): loads with node 24's shape. js2wasi3 links every module at build time, so
// `createRequire(filename)` returns a `require` whose calls fail with `MODULE_NOT_FOUND` (node's message for
// names that aren't built-ins); `Module` objects, `builtinModules`, `isBuiltin`, `wrap` and
// `_nodeModulePaths` work; loading and compiling throw.
"use strict";

const path = require("path");
const data = require("node:internal/builtin_data");
const { unavailableError, fn, fns } = require("node:internal/unavailable");

const WHY = "modules are linked at build time";

function Module(id = "", parent) {
  this.id = id;
  this.path = path.dirname(id);
  this.exports = {};
  this.filename = null;
  this.loaded = false;
  this.children = [];
  if (parent !== undefined) this.parent = parent;
}
fns(Module.prototype, "Module.prototype", ["load", "require", "_compile"], WHY);
// node's `constructor` is an accessor, so `inspect` shows a `Module` as a plain object.
Object.defineProperty(Module.prototype, "constructor", { get: () => Module, enumerable: false, configurable: false });

const builtinModules = data.builtinModules.slice();
const builtins = new Set(builtinModules);

function isBuiltin(name) {
  if (typeof name !== "string") return false;
  if (name.startsWith("node:")) return builtins.has(name) || builtins.has(name.slice(5));
  return builtins.has(name);
}

const NM = "node_modules";

// The `node_modules` directories a module in `from` searches, innermost first (POSIX).
function _nodeModulePaths(from) {
  from = path.resolve(from);
  if (from === "/") return ["/node_modules"];
  const paths = [];
  for (let i = from.length - 1, p = 0, last = from.length; i >= 0; --i) {
    const code = from.charCodeAt(i);
    if (code === 47) {
      if (p !== NM.length) paths.push(from.slice(0, last) + "/node_modules");
      last = i;
      p = 0;
    } else if (p !== -1) {
      if (NM.charCodeAt(NM.length - 1 - p) === code) ++p;
      else p = -1;
    }
  }
  paths.push("/node_modules");
  return paths;
}

const wrapper = ["(function (exports, require, module, __filename, __dirname) { ", "\n});"];

function wrap(script) {
  return wrapper[0] + script + wrapper[1];
}

function notFound(request, filename) {
  const e = isBuiltin(request)
    ? unavailableError(`require('${request}') through createRequire()`, WHY)
    : new Error(`Cannot find module '${request}'\nRequire stack:\n- ${filename}`);
  e.code = "MODULE_NOT_FOUND";
  e.requireStack = [filename];
  return e;
}

function createRequire(filename) {
  let file;
  if (typeof filename === "object" && filename !== null && typeof filename.href === "string") {
    file = require("url").fileURLToPath(filename);
  } else if (typeof filename === "string" && filename.startsWith("file:")) {
    file = require("url").fileURLToPath(filename);
  } else if (typeof filename === "string" && path.isAbsolute(filename)) {
    file = filename.endsWith("/") ? path.join(filename, "noop.js") : filename;
  } else {
    const e = new TypeError(`The argument 'filename' must be a file URL object, file URL string, or absolute path string. Received ${$$inspectValue(filename)}`);
    e.code = "ERR_INVALID_ARG_VALUE";
    throw e;
  }
  const req = function require(id) {
    throw notFound(id, file);
  };
  req.resolve = function resolve(request) {
    throw notFound(request, file);
  };
  req.resolve.paths = function paths(request) {
    return isBuiltin(request) ? null : _nodeModulePaths(path.dirname(file));
  };
  req.main = undefined;
  req.extensions = Module._extensions;
  req.cache = Module._cache;
  return req;
}

Object.assign(Module, {
  _cache: { __proto__: null },
  _pathCache: { __proto__: null },
  _extensions: { __proto__: null },
  globalPaths: [],
  _debug: () => {},
  isBuiltin,
  _findPath: fn("Module", "_findPath", WHY),
  _nodeModulePaths,
  _resolveLookupPaths: fn("Module", "_resolveLookupPaths", WHY),
  _load: fn("Module", "_load", WHY),
  _resolveFilename: fn("Module", "_resolveFilename", WHY),
  createRequire,
  _initPaths: () => {},
  _preloadModules: fn("Module", "_preloadModules", WHY),
  syncBuiltinESMExports: () => {},
  Module,
  registerHooks: fn("Module", "registerHooks", WHY),
  builtinModules,
  runMain: fn("Module", "runMain", WHY),
  register: fn("Module", "register", WHY),
  constants: { __proto__: null, compileCacheStatus: { __proto__: null, FAILED: 0, ENABLED: 1, ALREADY_ENABLED: 2, DISABLED: 3 } },
  enableCompileCache: fn("Module", "enableCompileCache", WHY),
  findPackageJSON: fn("Module", "findPackageJSON", WHY),
  flushCompileCache: () => {},
  getCompileCacheDir: () => undefined,
  stripTypeScriptTypes: fn("Module", "stripTypeScriptTypes", WHY),
  findSourceMap: () => undefined,
  SourceMap: class SourceMap {
    constructor() {
      throw unavailableError("module.SourceMap", WHY);
    }
  },
  getSourceMapsSupport: () => ({ __proto__: null, enabled: false, nodeModules: false, generatedCode: false }),
  setSourceMapsSupport: fn("Module", "setSourceMapsSupport", WHY),
});
// Not enumerable in node.
Object.defineProperties(Module, {
  _stat: { value: fn("Module", "_stat", WHY), writable: true, configurable: true },
  _readPackage: { value: fn("Module", "_readPackage", WHY), writable: true, configurable: true },
  wrap: { value: wrap, writable: true },
  wrapper: { value: wrapper, writable: true },
});
fns(Module._extensions, "Module._extensions", [".js", ".json", ".node"], WHY);

module.exports = Module;
