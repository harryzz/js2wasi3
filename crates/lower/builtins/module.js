// Module namespace objects (T41): `import * as ns`, `export * as ns from`. The module linker
// (`frontend::link`) makes one per module before any module runs, with the exported names in
// code-unit order and a getter reading each binding (so it's live, and throws in its TDZ). The
// object has a null prototype and class `CLASS_MODULE`, for which the runtime shows the getters
// as data properties (descriptors, printing) and rejects writes with node's messages.
function $$ModuleNamespace(keys, getters) {
  const ns = __intrinsic_module_new();
  for (let i = 0; i < keys.length; i++) {
    Object.defineProperty(ns, keys[i], { get: getters[i], enumerable: true });
  }
  Object.defineProperty(ns, Symbol.toStringTag, { value: "Module" });
  Object.preventExtensions(ns);
  return ns;
}

// Module evaluation (T41 slices 3–4): a program with a dependency that has top-level `await`, or
// with `import()`. The module linker makes each module's top-level code a function (`bodies`, in
// evaluation order, the entry last among the statically imported ones; an async function for a
// module with top-level `await`) and gives each one's requested modules (`requests`, indices into
// `bodies`); the generated code calls this first. It's the spec's Evaluate (16.2.1.5.3):
// InnerModuleEvaluation (Tarjan's DFS over the cycles), ExecuteAsyncModule,
// AsyncModuleExecutionFulfilled/Rejected and GatherAvailableAncestors, so the modules that don't wait
// for an async one run while it's suspended, and the ticks match node's.
// `import()` (`$$DynamicImport`) looks its specifier up in `table`: [absolute path, module, is JSON,
// namespace], or [path, -1, is JSON, null, error class, message, code] for one that fails to parse
// or link (node rejects then too); a package's key is the importer's directory, `\n` and the
// specifier. The specifier is resolved against the importer's `paths` entry
// ([directory, file]), and the module evaluated the same way (a job later: node's loader takes a
// few more).
// `entry`: the entry's index. `sync` (a library's `init`): the entry's evaluation throws instead
// of rejecting.
let $$moduleImport = undefined;

function $$EvaluateModules(bodies, tla, requests, paths, table, entry, sync) {
  const n = bodies.length;
  // Status: 0 new, 1 evaluating, 2 evaluating-async, 3 evaluated.
  const status = [];
  const dfsIndex = [];
  const dfsAncestor = [];
  const pending = [];
  const parents = [];
  const cycleRoot = [];
  const failed = [];
  const errors = [];
  // [[AsyncEvaluationOrder]]: 0 unset, -1 done.
  const asyncOrder = [];
  // [[TopLevelCapability]]: [promise, resolve, reject] or undefined.
  const caps = [];
  for (let i = 0; i < n; i++) {
    status.push(0);
    dfsIndex.push(0);
    dfsAncestor.push(0);
    pending.push(0);
    parents.push([]);
    cycleRoot.push(i);
    failed.push(false);
    errors.push(undefined);
    asyncOrder.push(0);
    caps.push(undefined);
  }
  let nextOrder = 1;
  let stack = [];

  function inner(m, index) {
    if (status[m] === 2 || status[m] === 3) {
      if (failed[m]) throw errors[m];
      return index;
    }
    if (status[m] === 1) return index;
    status[m] = 1;
    dfsIndex[m] = index;
    dfsAncestor[m] = index;
    pending[m] = 0;
    index++;
    stack.push(m);
    const reqs = requests[m];
    for (let k = 0; k < reqs.length; k++) {
      let r = reqs[k];
      index = inner(r, index);
      if (status[r] === 1) {
        if (dfsAncestor[r] < dfsAncestor[m]) dfsAncestor[m] = dfsAncestor[r];
      } else {
        r = cycleRoot[r];
        if (failed[r]) throw errors[r];
      }
      if (asyncOrder[r] > 0) {
        pending[m]++;
        parents[r].push(m);
      }
    }
    if (pending[m] > 0 || tla[m]) {
      asyncOrder[m] = nextOrder++;
      if (pending[m] === 0) executeAsync(m);
    } else {
      bodies[m]();
    }
    if (dfsAncestor[m] === dfsIndex[m]) {
      let done = false;
      while (!done) {
        const c = stack.pop();
        status[c] = asyncOrder[c] > 0 ? 2 : 3;
        cycleRoot[c] = m;
        done = c === m;
      }
    }
    return index;
  }

  async function executeAsync(m) {
    try {
      await bodies[m]();
    } catch (e) {
      rejected(m, e);
      return;
    }
    fulfilled(m);
  }

  function gather(m, list) {
    const ps = parents[m];
    for (let k = 0; k < ps.length; k++) {
      const p = ps[k];
      if (!list.includes(p) && !failed[cycleRoot[p]]) {
        pending[p]--;
        if (pending[p] === 0) {
          list.push(p);
          if (!tla[p]) gather(p, list);
        }
      }
    }
  }

  function fulfilled(m) {
    if (status[m] === 3) return;
    asyncOrder[m] = -1;
    status[m] = 3;
    if (caps[m] !== undefined) caps[m][1](undefined);
    const list = [];
    gather(m, list);
    list.sort((a, b) => asyncOrder[a] - asyncOrder[b]);
    for (let k = 0; k < list.length; k++) {
      const p = list[k];
      if (status[p] === 3) continue;
      if (tla[p]) {
        executeAsync(p);
      } else {
        try {
          bodies[p]();
        } catch (e) {
          rejected(p, e);
          continue;
        }
        asyncOrder[p] = -1;
        status[p] = 3;
        if (caps[p] !== undefined) caps[p][1](undefined);
      }
    }
  }

  function rejected(m, e) {
    if (status[m] === 3) return;
    failed[m] = true;
    errors[m] = e;
    status[m] = 3;
    asyncOrder[m] = -1;
    const ps = parents[m];
    for (let k = 0; k < ps.length; k++) rejected(ps[k], e);
    if (caps[m] !== undefined) caps[m][2](e);
  }

  // Evaluate(): a promise for module `m`'s evaluation (its cycle's, once it has run).
  function evaluate(m) {
    if (status[m] === 2 || status[m] === 3) m = cycleRoot[m];
    if (caps[m] !== undefined) return caps[m][0];
    let resolve;
    let reject;
    const promise = new Promise((res, rej) => {
      resolve = res;
      reject = rej;
    });
    caps[m] = [promise, resolve, reject];
    stack = [];
    try {
      inner(m, 0);
    } catch (e) {
      for (let k = 0; k < stack.length; k++) {
        status[stack[k]] = 3;
        failed[stack[k]] = true;
        errors[stack[k]] = e;
      }
      reject(e);
      return promise;
    }
    if (asyncOrder[m] <= 0) resolve(undefined);
    return promise;
  }

  // import(): the specifier and options are checked as the spec's EvaluateImportCall does, the
  // module found in the table, evaluated, and the promise resolved with its namespace.
  $$moduleImport = (k, specifier, options) => {
    let resolve;
    let reject;
    const promise = new Promise((res, rej) => {
      resolve = res;
      reject = rej;
    });
    let spec;
    let type;
    try {
      spec = `${specifier}`;
      if (options !== undefined) {
        if (options === null || (typeof options !== "object" && typeof options !== "function")) {
          throw new TypeError("The second argument to import() must be an object");
        }
        const attributes = options.with;
        if (attributes !== undefined) {
          if (attributes === null || (typeof attributes !== "object" && typeof attributes !== "function")) {
            throw new TypeError("The 'with' option of import() must be an object");
          }
          const keys = Object.keys(attributes);
          const values = [];
          for (let i = 0; i < keys.length; i++) {
            const v = attributes[keys[i]];
            if (typeof v !== "string") throw new TypeError("Import attribute value must be a string");
            values.push(v);
          }
          for (let i = 0; i < keys.length; i++) {
            if (keys[i] === "type") type = values[i];
          }
          for (let i = 0; i < keys.length; i++) {
            if (keys[i] !== "type") {
              throw $$moduleError(TypeError, "ERR_IMPORT_ATTRIBUTE_UNSUPPORTED",
                `Import attribute "${keys[i]}" with value "${values[i]}" is not supported in ${$$fileUrl(paths[k][1])}`);
            }
          }
        }
      }
    } catch (e) {
      reject(e);
      return promise;
    }
    const [dir, from] = paths[k];
    let entry;
    if (spec.startsWith("./") || spec.startsWith("../") || spec.startsWith("/")) {
      const key = $$normalizePath(spec.startsWith("/") ? spec : dir + spec);
      for (let i = 0; i < table.length; i++) {
        if (table[i][0] === key) entry = table[i];
      }
      if (entry === undefined) {
        reject($$moduleError(Error, "ERR_MODULE_NOT_FOUND", `Cannot find module '${key}' imported from ${from}`));
        return promise;
      }
    } else if (spec.startsWith("node:")) {
      reject($$moduleError(Error, "ERR_UNKNOWN_BUILTIN_MODULE", `No such built-in module: ${spec} (node's built-in modules aren't available in js2wasi3)`));
      return promise;
    } else {
      // A package (T41 slice 5): resolved at build time from this directory.
      const key = dir + "\n" + spec;
      for (let i = 0; i < table.length; i++) {
        if (table[i][0] === key) entry = table[i];
      }
      if (entry === undefined) {
        const name = spec.startsWith("@") ? spec.split("/").slice(0, 2).join("/") : spec.split("/")[0];
        reject($$moduleError(Error, "ERR_MODULE_NOT_FOUND", `Cannot find package '${name}' imported from ${from}`));
        return promise;
      }
    }
    const url = entry[0].includes("\n") ? spec : $$fileUrl(entry[0]);
    let problem;
    if (type !== undefined && type !== "json") {
      problem = $$moduleError(TypeError, "ERR_IMPORT_ATTRIBUTE_UNSUPPORTED", `Import attribute "type" with value "${type}" is not supported in ${url}`);
    } else if (entry[2] && type === undefined) {
      problem = $$moduleError(TypeError, "ERR_IMPORT_ATTRIBUTE_MISSING", `Module "${url}" needs an import attribute of "type: json"`);
    } else if (!entry[2] && type === "json") {
      problem = $$moduleError(TypeError, "ERR_IMPORT_ATTRIBUTE_TYPE_INCOMPATIBLE", `Module "${url}" is not of type "json"`);
    }
    if (problem !== undefined) {
      reject(problem);
      return promise;
    }
    // The host loads the module in a later job (as engines do; node's loader takes a few).
    (async () => {
      await undefined;
      if (entry[1] < 0) {
        // It (or a module it imports) doesn't parse or link: the same error every time.
        if (entry[7] === undefined) {
          const e = entry[4] === "SyntaxError" ? new SyntaxError(entry[5]) : new Error(entry[5]);
          if (entry[6] !== "") e.code = entry[6];
          entry.push(e);
        }
        reject(entry[7]);
        return;
      }
      try {
        await evaluate(entry[1]);
      } catch (e) {
        reject(e);
        return;
      }
      resolve(entry[3]);
    })();
    return promise;
  };

  if (sync) {
    stack = [];
    caps[entry] = [undefined, () => {}, () => {}];
    try {
      inner(entry, 0);
    } catch (e) {
      for (let k = 0; k < stack.length; k++) {
        status[stack[k]] = 3;
        failed[stack[k]] = true;
        errors[stack[k]] = e;
      }
      throw e;
    }
    return undefined;
  }
  return evaluate(entry);
}

// A node-style error with a `code`.
function $$moduleError(C, code, message) {
  const e = new C(message);
  e.code = code;
  return e;
}

// `/a/./b/../c` → `/a/c` (as the module linker's `normalize`).
function $$normalizePath(path) {
  const parts = path.split("/");
  const out = [];
  for (let i = 0; i < parts.length; i++) {
    const p = parts[i];
    if (p === ".") continue;
    if (p === ".." && out.length > 0 && out[out.length - 1] !== ".." && out[out.length - 1] !== "") {
      out.pop();
      continue;
    }
    out.push(p);
  }
  return out.join("/");
}

// node's `url.pathToFileURL(path)` for an absolute path.
function $$fileUrl(path) {
  return "file://" + $$urlEncode(path, "\"#%<>?`{}^|[]\\~");
}

// `import(x, o)` (T41 slice 4): the linker writes `$$DynamicImport(<importer>, x, o)`.
function $$DynamicImport(k, specifier, options) {
  return $$moduleImport(k, specifier, options);
}

// `import.meta` (T41 slice 3): one null-prototype object per module, made before any module runs,
// with node's properties. The paths are the source file's at build time (node's values when the
// program runs where it was built).
function $$ImportMeta(url, filename, dirname, main) {
  const meta = Object.create(null);
  meta.dirname = dirname;
  meta.filename = filename;
  meta.main = main;
  meta.resolve = function resolve(specifier) {
    return $$importMetaResolve(url, filename, `${specifier}`);
  };
  meta.url = url;
  return meta;
}

// `import.meta.resolve(specifier)`: a relative or absolute path against the module's URL, a URL
// as it is; a bare specifier would be a package, which isn't there at run time.
function $$importMetaResolve(url, filename, spec) {
  const relative = spec.startsWith("./") || spec.startsWith("../") || spec.startsWith("/") ||
    spec === "." || spec === "..";
  if (!relative) {
    let k = 0;
    for (; k < spec.length; k++) {
      const c = spec[k];
      const letter = (c >= "a" && c <= "z") || (c >= "A" && c <= "Z");
      if (!letter && (k === 0 || !((c >= "0" && c <= "9") || c === "+" || c === "." || c === "-"))) break;
    }
    if (k > 0 && spec[k] === ":") return spec;
    const e = new Error(`Cannot find package '${spec}' imported from ${filename}`);
    e.code = "ERR_MODULE_NOT_FOUND";
    throw e;
  }
  let path = spec;
  let rest = "";
  for (let i = 0; i < spec.length; i++) {
    if (spec[i] === "?" || spec[i] === "#") {
      path = spec.slice(0, i);
      rest = spec.slice(i);
      break;
    }
  }
  const base = url.slice("file://".length);
  const segments = (path.startsWith("/") ? path : base.slice(0, base.lastIndexOf("/") + 1) + path).split("/");
  const out = [];
  for (let i = 1; i < segments.length; i++) {
    const s = segments[i];
    const last = i === segments.length - 1;
    if (s === "..") {
      out.pop();
      if (last) out.push("");
    } else if (s === ".") {
      if (last) out.push("");
    } else {
      out.push(s);
    }
  }
  return "file:///" + $$urlEncode(out.join("/"), "\"<>?^`{} ") + $$urlEncode(rest, "\"<>`' ");
}

// Percent-encode the characters in `set`, controls and non-ASCII (as UTF-8).
function $$urlEncode(s, set) {
  let out = "";
  for (let i = 0; i < s.length; i++) {
    const c = s.codePointAt(i);
    if (c > 0xffff) i++;
    if (c > 0x20 && c < 0x7f && !set.includes(s[i])) {
      out += s[i];
      continue;
    }
    const bytes = c < 0x80 ? [c]
      : c < 0x800 ? [0xc0 | (c >> 6), 0x80 | (c & 63)]
      : c < 0x10000 ? [0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63)]
      : [0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63)];
    for (const b of bytes) out += "%" + (b < 16 ? "0" : "") + b.toString(16).toUpperCase();
  }
  return out;
}
