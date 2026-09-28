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

// Async module evaluation (T41 slice 3): a command whose dependency has top-level `await`. The
// module linker makes each module's top-level code a function (`bodies`, in evaluation order, the
// entry last; an async function for a module with top-level `await`) and gives each one's
// requested modules (`requests`, indices into `bodies`); the generated code awaits this. It's the
// spec's Evaluate (16.2.1.5.3): InnerModuleEvaluation (Tarjan's DFS over the cycles),
// ExecuteAsyncModule, AsyncModuleExecutionFulfilled/Rejected and GatherAvailableAncestors, so the
// modules that don't wait for an async one run while it's suspended, and the ticks match node's.
function $$EvaluateModules(bodies, tla, requests) {
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
  }
  let nextOrder = 1;
  const entry = n - 1;
  let resolveTop;
  let rejectTop;
  const top = new Promise((resolve, reject) => {
    resolveTop = resolve;
    rejectTop = reject;
  });
  const stack = [];

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
    if (m === entry) resolveTop(undefined);
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
        if (p === entry) resolveTop(undefined);
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
    if (m === entry) rejectTop(e);
  }

  try {
    inner(entry, 0);
  } catch (e) {
    for (let k = 0; k < stack.length; k++) {
      status[stack[k]] = 3;
      failed[stack[k]] = true;
      errors[stack[k]] = e;
    }
    rejectTop(e);
    return top;
  }
  if (asyncOrder[entry] === 0) resolveTop(undefined);
  return top;
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
