// CommonJS modules (T54). The module linker (`frontend::link`) makes each CommonJS file a function
// `cjs$k(exports, require, module, __filename, __dirname)` and writes their table first:
// [function, filename, dirname, requests, module, parent], `requests` being the module's static
// `require` specifiers as [specifier, index] (-1: not found; [specifier, -1, code, message]: another
// resolution error, like a package's `exports` without it), `module` its module object once it's
// required (undefined before) and `parent` the index of the module that first required it.
// A module runs on its first `require` (node's Module._load): a cycle sees the partial
// `module.exports`; one that throws leaves the cache (node deletes it), so a later `require` runs
// it again.
let $$cjsMainModule = undefined;
let $$cjsTable = undefined;
let $$cjsProto = undefined;
// What a computed `require` can load (slice 4): [absolute path, index] (or an ES module's
// [path, -3, evaluation index, namespace, filename]).
let $$cjsKnownFiles = undefined;

function $$cjsKnown(files) {
  $$cjsKnownFiles = files;
}

// The entry: node's main module (`require.main`, `module.id` is ".").
function $$cjsMain(table, k) {
  $$cjsTable = table;
  $$cjsLoad(table, k, -1, true);
}

// A CommonJS module an ES module imports: node's ESM loader runs it (no `module.parent`); its
// named exports are then read from `module.exports` (`$$cjsNamed`).
function $$cjsImport(table, k) {
  $$cjsTable = table;
  return $$cjsLoad(table, k, -2, false);
}

function $$cjsNamed(exports, name) {
  return exports === undefined || exports === null ? undefined : exports[name];
}

// `parent`: the requiring module's index, -1 for the main module, -2 for an ES module's import.
function $$cjsLoad(table, k, parent, main) {
  const rec = table[k];
  const cached = rec[4];
  const parentModule = parent < 0 ? undefined : table[parent][4];
  if (cached !== undefined) {
    if (parentModule !== undefined && !parentModule.children.includes(cached)) parentModule.children.push(cached);
    return cached.exports;
  }
  const filename = rec[1];
  const dirname = rec[2];
  const module = Object.create($$cjsModulePrototype());
  module.id = main ? "." : filename;
  module.path = dirname;
  module.exports = {};
  module.filename = filename;
  module.loaded = false;
  module.children = [];
  module.paths = $$cjsPaths(dirname);
  if (main) $$cjsMainModule = module;
  if (parentModule !== undefined) parentModule.children.push(module);
  rec[4] = module;
  rec[5] = parent;
  const require = $$cjsRequireFunction(table, k);
  const exports = module.exports;
  // No local assigned in the `try` is read in the `catch` (wasmtime 49, F224).
  try {
    rec[0].call(exports, exports, require, module, filename, dirname);
  } catch (e) {
    rec[4] = undefined;
    if (parentModule !== undefined) {
      const i = parentModule.children.indexOf(module);
      if (i >= 0) parentModule.children.splice(i, 1);
    }
    throw e;
  }
  module.loaded = true;
  return module.exports;
}

// `Module.prototype`: node's deprecated `parent` getter (the module that first required this one;
// `null` for the main module). node's module objects are `Module`s, printed `Module {…}`.
function $$cjsModulePrototype() {
  if ($$cjsProto === undefined) {
    const Module = function Module(id, parent) {};
    $$cjsProto = Module.prototype;
    Object.defineProperty($$cjsProto, "parent", {
      get: function () {
        const table = $$cjsTable;
        for (let i = 0; i < table.length; i++) {
          if (table[i][4] !== this) continue;
          const p = table[i][5];
          return p === -1 ? null : p < 0 ? undefined : table[p][4];
        }
        return undefined;
      },
      configurable: true,
    });
  }
  return $$cjsProto;
}

// node's Module._nodeModulePaths: `<dir>/node_modules` for the directory and each one above it
// (skipping `node_modules` directories), then `/node_modules`.
function $$cjsPaths(dir) {
  const out = [];
  const parts = dir.split("/");
  for (let i = parts.length; i > 1; i--) {
    if (parts[i - 1] === "node_modules") continue;
    out.push(parts.slice(0, i).join("/") + "/node_modules");
  }
  out.push("/node_modules");
  return out;
}

// Module `k`'s `require`: its static specifiers are in the table.
function $$cjsRequireFunction(table, k) {
  const requests = table[k][3];
  const find = (id) => {
    $$cjsCheckId(id);
    for (let i = 0; i < requests.length; i++) {
      const r = requests[i];
      if (r[0] !== id) continue;
      if (r[1] >= 0 || r[1] === -3) return r;
      // A package's `exports` doesn't have it (node's code and message).
      if (r.length > 2) {
        const e = new Error(r[3]);
        e.code = r[2];
        throw e;
      }
      throw $$cjsNotFound(table, k, id);
    }
    // A specifier known only at run time: LOAD_AS_FILE / index files among the known files.
    const known = $$cjsKnownFiles;
    if (known !== undefined && (id.startsWith("./") || id.startsWith("../") || id.startsWith("/"))) {
      const base = $$normalizePath(id.startsWith("/") ? id : table[k][2] + "/" + id);
      const suffixes = id.endsWith("/") ? ["/index.js", "/index.json"] : ["", ".js", ".json", "/index.js", "/index.json"];
      for (let s = 0; s < suffixes.length; s++) {
        const path = base + suffixes[s];
        for (let i = 0; i < known.length; i++) {
          if (known[i][0] === path) return known[i];
        }
      }
    }
    // Not a static specifier: only modules in the build can be found.
    const e = $$cjsNotFound(table, k, id);
    e.message += "\n(js2wasi3: a computed require finds only modules known at build time; name others with --include)";
    throw e;
  };
  const require = function require(id) {
    const r = find(id);
    return r[1] === -3 ? $$cjsRequireEsm(table, k, r) : $$cjsLoad(table, r[1], k, false);
  };
  require.resolve = function resolve(request) {
    const r = find(request);
    if (r[1] === -3) return r[4];
    // A node built-in's filename is its name (T55): `require.resolve("node:path")` keeps `node:`.
    const filename = table[r[1]][1];
    return request === "node:" + filename ? request : filename;
  };
  require.main = $$cjsMainModule;
  return require;
}

// `require` of an ES module (node 24): evaluated synchronously (`$$moduleRequire`), its namespace
// returned, with `__esModule: true` when it has a default export (a namespace of its own, made
// once), or its `"module.exports"` export. `r`: [specifier, -3, evaluation index, namespace, filename].
function $$cjsRequireEsm(table, k, r) {
  if (!$$moduleRequire(r[2])) {
    const e = new Error("require() cannot be used on an ESM graph with top-level await. Use import() instead. To see where the top-level await comes from, use --experimental-print-required-tla.\nRequired module: " + r[4] + "\nRequire stack:\n- " + $$cjsStack(table, k).join("\n- "));
    e.code = "ERR_REQUIRE_ASYNC_MODULE";
    throw e;
  }
  const ns = r[3];
  if ("module.exports" in ns) return ns["module.exports"];
  if (!("default" in ns) || "__esModule" in ns) return ns;
  if (r.length > 5) return r[5];
  const keys = Object.keys(ns);
  const outKeys = [];
  const getters = [];
  let added = false;
  for (let i = 0; i <= keys.length; i++) {
    if (!added && (i === keys.length || keys[i] > "__esModule")) {
      outKeys.push("__esModule");
      getters.push(() => true);
      added = true;
    }
    if (i < keys.length) {
      const key = keys[i];
      outKeys.push(key);
      getters.push(() => ns[key]);
    }
  }
  const out = $$ModuleNamespace(outKeys, getters);
  r.push(out);
  return out;
}

// The require stack: the requiring module, the one that first required it, … up to the main module.
function $$cjsStack(table, k) {
  const stack = [];
  for (let i = k; i >= 0; i = table[i][5]) stack.push(table[i][1]);
  return stack;
}

function $$cjsCheckId(id) {
  if (typeof id !== "string") throw $$errInvalidArgType("id", "of type string", id);
  if (id === "") {
    const e = new TypeError("The argument 'id' must be a non-empty string. Received ''");
    e.code = "ERR_INVALID_ARG_VALUE";
    throw e;
  }
}

// node's MODULE_NOT_FOUND error: the message lists the require stack (the requiring module, the
// one that first required it, … up to the main module).
function $$cjsNotFound(table, k, id) {
  const stack = $$cjsStack(table, k);
  const e = new Error("Cannot find module '" + id + "'\nRequire stack:\n- " + stack.join("\n- "));
  e.code = "MODULE_NOT_FOUND";
  e.requireStack = stack;
  return e;
}
