#!/usr/bin/env node
// T101: the Node.js compatibility page (docs/compatibility.md, docs/compat/compatibility.json, site/).
//
// Usage:
//   node scripts/compat.mjs inventory <all.json>   derive docs/compat/inventory.json from node's API docs
//   node scripts/compat.mjs probe                  run the presence probe (node + js2wasi3/wasmtime) into
//                                                  docs/compat/probe.json (needs target/release/js2wasi3)
//   node scripts/compat.mjs render                 write the page, the dataset and the site from the files above
//   node scripts/compat.mjs bench <results.tsv> [machine]   import a scripts/bench.sh run (docs/compat/benchmarks.json)
//   node scripts/compat.mjs                        probe, then render
//   checks: `summary` (status counts), `find <regex>` (rows by path), `refusals` (the stubs found), `debug` (samples)
//
// The inventory comes from node v24.21.0's https://nodejs.org/docs/v24.21.0/api/all.json (downloaded 2026-10-10);
// only the derived rows are kept in the repo. Each row is one documented API (method, property, class, static
// method, constructor, event). Statuses, and how each is known (the "basis"):
//   supported   present in js2wasi3 and named in a passing fixture        (test-backed)
//   present     present in js2wasi3, no fixture names it                    (probe)
//   partial     present, with a known gap                                   (declared: docs/compat/partial.json)
//   refused     present, but throws "… isn't available in js2wasi3 (why)"   (stub in crates/frontend/node)
//   missing     node has it, js2wasi3 doesn't                               (probe)
//   no module   the module isn't one js2wasi3 ships: importing it is a build error (registry)
//   not probed  events, instance-only properties, names not reachable from a module or global
//   n/a         chapters that aren't APIs of a running program (C++ embedding, TypeScript, …)
import fs from "node:fs";
import path from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const COMPAT = path.join(ROOT, "docs/compat");
const NODE_VERSION = "v24.21.0";

// Chapter (all.json module name) → the module it documents (`require("node:<spec>")`), the names its docs use
// for the module object, and other roots (`fsPromises` → `require("node:fs").promises`).
const CHAPTERS = {
  assert: { spec: "assert", vars: ["assert"] },
  asynchronous_context_tracking: { spec: "async_hooks", vars: ["async_hooks"] },
  async_hooks: { spec: "async_hooks", vars: ["async_hooks"] },
  buffer: { spec: "buffer", vars: ["buffer"] },
  child_process: { spec: "child_process", vars: ["child_process"] },
  cluster: { spec: "cluster", vars: ["cluster"] },
  console: { spec: "console", vars: ["console"] },
  crypto: { spec: "crypto", vars: ["crypto"] },
  diagnostics_channel: { spec: "diagnostics_channel", vars: ["diagnostics_channel", "dc"] },
  dns: { spec: "dns", vars: ["dns"], roots: { dnsPromises: ["dns", "promises"] } },
  domain: { spec: "domain", vars: ["domain"] },
  Events: { spec: "events", vars: ["events"] },
  fs: { spec: "fs", vars: ["fs"], roots: { fsPromises: ["fs", "promises"] } },
  http: { spec: "http", vars: ["http"] },
  "http/2": { spec: "http2", vars: ["http2"] },
  https: { spec: "https", vars: ["https"] },
  inspector: { spec: "inspector", vars: ["inspector"] },
  iterable_streams: { spec: "stream/iter", vars: [] },
  module: { spec: "module", vars: ["module"] },
  "modules:_`node:module`_api": { spec: "module", vars: ["module"] },
  net: { spec: "net", vars: ["net"] },
  os: { spec: "os", vars: ["os"] },
  path: { spec: "path", vars: ["path"] },
  performance_measurement_apis: { spec: "perf_hooks", vars: ["perf_hooks"] },
  punycode: { spec: "punycode", vars: ["punycode"] },
  querystring: { spec: "querystring", vars: ["querystring"] },
  readline: { spec: "readline", vars: ["readline"], roots: { readlinePromises: ["readline/promises"] } },
  repl: { spec: "repl", vars: ["repl"] },
  single_executable_applications: { spec: "sea", vars: ["sea"] },
  sqlite: { spec: "sqlite", vars: ["sqlite"] },
  stream: { spec: "stream", vars: ["stream"] },
  string_decoder: { spec: "string_decoder", vars: ["string_decoder"] },
  test_runner: { spec: "test", vars: ["test"] },
  timers: { spec: "timers", vars: ["timers"], roots: { timersPromises: ["timers/promises"] } },
  "tls_(ssl)": { spec: "tls", vars: ["tls"] },
  trace_events: { spec: "trace_events", vars: ["trace_events"] },
  tty: { spec: "tty", vars: ["tty"] },
  dgram: { spec: "dgram", vars: ["dgram"] },
  url: { spec: "url", vars: ["url"] },
  util: { spec: "util", vars: ["util"] },
  v8: { spec: "v8", vars: ["v8"] },
  vm: { spec: "vm", vars: ["vm"] },
  "webassembly_system_interface_(wasi)": { spec: "wasi", vars: ["wasi"] },
  web_crypto_api: { spec: null, vars: [], roots: { subtle: ["crypto", "webcrypto", "subtle"] } },
  web_streams_api: { spec: "stream/web", vars: [] },
  worker_threads: { spec: "worker_threads", vars: ["worker_threads"] },
  zlib: { spec: "zlib", vars: ["zlib"] },
};
// Chapters that document no API a running program calls.
const NOT_APPLICABLE = {
  "c++_embedder_api": "C++ embedding API",
  environment_variables: "node's own environment variables",
  "modules:_typescript": "TypeScript type stripping (a node CLI feature)",
  permissions: "node's permission model (CLI flags)",
};
const KINDS = { methods: "method", properties: "property", classes: "class", classMethods: "static method",
  ctors: "constructor", events: "event" };

function inventory(file) {
  const all = JSON.parse(fs.readFileSync(file, "utf8"));
  const rows = [];
  const add = (chapter, title, kind, node, cls, owner) => {
    const text = node.textRaw.replace(/^(Class|Static method|Event):\s*/, "");
    rows.push({
      chapter, title, kind, text: text.replace(/`/g, ""),
      cls: cls ? cls.replace(/`/g, "") : undefined,
      owner,
      stability: node.stability ?? null,
    });
  };
  // Every API node under a chapter: sections (`modules`, `miscs`) are only walked into.
  // `owner`: the object a section documents when its names lack a prefix (`stdout` under `process`).
  const walk = (chapter, title, node, cls, owner) => {
    for (const [key, kind] of Object.entries(KINDS)) {
      for (const c of node[key] ?? []) {
        add(chapter, title, kind, c, cls, owner);
        walk(chapter, title, c, kind === "class" ? c.textRaw.replace(/^Class:\s*/, "") : cls, owner);
      }
    }
    for (const c of [...(node.modules ?? []), ...(node.miscs ?? [])]) walk(chapter, title, c, cls, owner);
  };
  for (const m of all.modules) walk(m.name, m.textRaw, m, undefined);
  // Globals and their chapter (`globals.md`: `classes`, `methods`, `globals` at the top level).
  const g = { textRaw: "Globals", classes: all.classes, methods: all.methods, globals: all.globals };
  for (const c of all.globals ?? []) {
    add("globals", "Globals", "global", c);
    walk("globals", "Globals", c, undefined, c.name.toLowerCase());
  }
  for (const [key, kind] of Object.entries(KINDS)) {
    for (const c of g[key] ?? []) {
      add("globals", "Globals", kind, c);
      walk("globals", "Globals", c, kind === "class" ? c.textRaw.replace(/^Class:\s*/, "") : undefined);
    }
  }
  const out = { node: NODE_VERSION, source: `https://nodejs.org/docs/${NODE_VERSION}/api/all.json`,
    derived: "2026-10-10", rows };
  fs.mkdirSync(COMPAT, { recursive: true });
  fs.writeFileSync(path.join(COMPAT, "inventory.json"), JSON.stringify(out, null, 0).replace(/\},\{/g, "},\n{") + "\n");
  console.log(`${rows.length} rows in ${new Set(rows.map((r) => r.chapter)).size} chapters`);
}

// The modules js2wasi3 ships (`crates/frontend/src/node.rs` `MODULES`, without `internal/` and `_…` ones).
function shipped() {
  const src = fs.readFileSync(path.join(ROOT, "crates/frontend/src/node.rs"), "utf8");
  const body = src.slice(src.indexOf("const MODULES"), src.indexOf("];", src.indexOf("const MODULES")));
  return [...body.matchAll(/^\s*\("([a-z0-9_/]+)",/gm)].map((m) => m[1]).filter((n) => !n.startsWith("internal") && !n.startsWith("_"));
}

// A row's name as a dotted path: `fsPromises.access(path[, mode])` → ["fsPromises", "access"];
// `readable[Symbol.asyncIterator]()` → ["readable", "@@asyncIterator"]; null when it isn't one (`[index]`).
function dotted(text) {
  const t = text.replace(/^new\s+/, "").trim();
  const m = t.match(/^[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*|\[Symbol\.[A-Za-z]+\])*/);
  if (!m) return null;
  return m[0].replace(/\[Symbol\.([A-Za-z]+)\]/g, ".@@$1").split(".");
}

// Candidate paths for a dotted name in a chapter: [root, ...keys], root "" = globalThis, else a module spec.
function candidates(ch, names) {
  const [first, ...rest] = names;
  const c = CHAPTERS[ch] ?? {};
  const out = [];
  if (c.roots?.[first]) {
    const [spec, ...pre] = c.roots[first];
    out.push(spec === "crypto" && ch === "web_crypto_api" ? ["", "crypto", "subtle", ...rest] : [spec, ...pre, ...rest]);
  }
  if (c.spec && c.vars.includes(first)) out.push([c.spec, ...rest]);
  out.push(["", first, ...rest]);
  if (c.spec && !c.vars.includes(first)) out.push([c.spec, first, ...rest]);
  return out;
}

// Each row's candidate checks (a path whose last key is tested with `in`), or a reason it isn't probed.
function checks(row) {
  if (NOT_APPLICABLE[row.chapter]) return { na: NOT_APPLICABLE[row.chapter] };
  if (row.kind === "event") return { skip: "events aren't probed" };
  if (row.kind === "global") return { paths: [["", "process"]] };
  let names = dotted(row.text);
  if (!names) return { skip: "not a name" };
  if (row.owner && !row.cls && names.length === 1 && names[0] !== row.owner) names = [row.owner, ...names];
  if (row.cls && row.kind !== "class" && row.kind !== "static method" && row.kind !== "constructor") {
    // A member of a class: `buf.toString()` → Buffer.prototype.toString.
    const cls = dotted(row.cls);
    if (!cls) return { skip: "class not reachable by name" };
    const member = names[names.length - 1];
    const proto = candidates(row.chapter, cls).map((p) => [...p, "prototype", member]);
    // `console.log()` under `Class: Console` is called on the `console` instance: that path first.
    const own = names.length > 1 && (CHAPTERS[row.chapter]?.vars.includes(names[0]) || names[0] === row.owner)
      ? candidates(row.chapter, names) : [];
    return { paths: [...own, ...proto], instance: true };
  }
  if (row.kind === "constructor") {
    const cls = dotted(row.cls ?? row.text);
    return cls ? { paths: candidates(row.chapter, cls) } : { skip: "class not reachable by name" };
  }
  return { paths: candidates(row.chapter, names) };
}

// A path as a member expression that names every key (for the never-run branch of the probe).
const IDENT = /^[A-Za-z_$][\w$]*$/;
const RESERVED = new Set(["default", "delete", "new", "class", "function", "import", "export", "in", "of", "void", "typeof"]);
function named(p) {
  const [root, ...keys] = p;
  let e = root === "" ? (IDENT.test(keys[0]) && !RESERVED.has(keys[0]) ? keys.shift() : `globalThis[${JSON.stringify(keys.shift())}]`)
    : `M[${JSON.stringify(root)}]`;
  for (const k of keys) e += k.startsWith("@@") ? `[Symbol.${k.slice(2)}]` : IDENT.test(k) ? `.${k}` : `[${JSON.stringify(k)}]`;
  return `  void ${e};`;
}

// The probe program: every check, evaluated with `in` (no getters run on the tested key), printed as one JSON
// array (0 absent, 1 present, 2 module not loaded, 3 threw on the way).
function probeProgram(specs, list, drop = new Set()) {
  const req = specs.map((s) => `try { M[${JSON.stringify(s)}] = require(${JSON.stringify("node:" + s)}); } catch {}`);
  // Globals through their names (js2wasi3 links a node global where it's named; `"URL" in globalThis` is
  // false there, F387).
  const globals = [...new Set(list.filter((p) => p[0] === "").map((p) => p[1]))];
  const getter = (k) => (IDENT.test(k) && !RESERVED.has(k)
    ? `G[${JSON.stringify(k)}] = () => (typeof ${k} === "undefined" ? undefined : ${k});`
    : `G[${JSON.stringify(k)}] = () => globalThis[${JSON.stringify(k)}];`);
  return `"use strict";
const M = {};
const G = {};
${req.join("\n")}
${globals.map(getter).join("\n")}
const C = ${JSON.stringify(list)};
// js2wasi3 compiles in only the built-ins a program names: every path also appears as a plain member
// expression in a branch that never runs, so each name is in the program.
if (M.__never === 1) {
${list.map((p, i) => (drop.has(i) ? "  // (a build error)" : named(p))).join("\n")}
}
const out = [];
for (const p of C) {
  let v = 0;
  try {
    if (p[0] === "" && p.length === 2) v = G[p[1]]() !== undefined ? 1 : 0;
    else if (p[0] === "" ? false : M[p[0]] === undefined) v = 2;
    else {
      let o = p[0] === "" ? G[p[1]]() : M[p[0]];
      for (let i = p[0] === "" ? 2 : 1; i < p.length - 1; i++) o = o[p[i]];
      const k = p[p.length - 1];
      const key = k.startsWith("@@") ? Symbol[k.slice(2)] : k;
      v = o !== null && (typeof o === "object" || typeof o === "function") && key in o ? 1 : 0;
    }
  } catch {
    v = 3;
  }
  out.push(v);
}
console.log(JSON.stringify(out));
`;
}

function probe() {
  const inv = JSON.parse(fs.readFileSync(path.join(COMPAT, "inventory.json"), "utf8"));
  const list = [];
  const index = new Map();
  for (const row of inv.rows) {
    for (const p of checks(row).paths ?? []) {
      const k = JSON.stringify(p);
      if (!index.has(k)) {
        index.set(k, list.length);
        list.push(p);
      }
    }
  }
  const allSpecs = [...new Set(Object.values(CHAPTERS).map((c) => c.spec).filter(Boolean)), ...Object.values(CHAPTERS)
    .flatMap((c) => Object.values(c.roots ?? {}).map((r) => r[0])).filter((s) => s !== "crypto")];
  const specs = [...new Set(allSpecs)];
  const ours = new Set(shipped());
  const tmp = fs.mkdtempSync(path.join(process.env.TMPDIR ?? "/tmp", "compat-"));
  fs.writeFileSync(path.join(tmp, "node.js"), probeProgram(specs, list));
  const node = JSON.parse(execFileSync("node", [path.join(tmp, "node.js")], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"], maxBuffer: 1 << 26 }));
  // A name js2wasi3 rejects at build time even where it never runs ("unsupported: …"): that path's status;
  // drop its line from the never-run branch and build again.
  const bin = path.join(ROOT, "target/release/js2wasi3");
  const ourSpecs = specs.filter((s) => ours.has(s));
  const drop = new Set();
  const buildErrors = {};
  for (;;) {
    const src = probeProgram(ourSpecs, list, drop);
    fs.writeFileSync(path.join(tmp, "wasm.js"), src);
    const b = spawnSync(bin, ["build", path.join(tmp, "wasm.js"), "-o", path.join(tmp, "wasm.wasm")], { encoding: "utf8" });
    if (b.status === 0) break;
    const first = src.split("\n").findIndex((l) => l.startsWith("if (M.__never === 1) {")) + 2;
    const errs = [...b.stderr.matchAll(/wasm\.js:(\d+):\d+: error: (.*)/g)];
    const before = drop.size;
    for (const [, line, msg] of errs) {
      const i = Number(line) - first;
      if (i >= 0 && i < list.length) {
        drop.add(i);
        buildErrors[JSON.stringify(list[i])] = msg;
      }
    }
    if (drop.size === before) throw new Error(`probe build failed:\n${b.stderr}`);
  }
  const run = spawnSync("wasmtime", ["run", "-Sp3", path.join(tmp, "wasm.wasm")], { encoding: "utf8", maxBuffer: 1 << 26 });
  if (run.status !== 0) throw new Error(`probe run failed: ${run.stderr}`);
  const wasm = JSON.parse(run.stdout);
  const results = {};
  for (const [k, i] of index) results[k] = [node[i], wasm[i]];
  const wasmtime = execFileSync("wasmtime", ["--version"], { encoding: "utf8" }).trim();
  const commit = execFileSync("git", ["rev-parse", "--short", "HEAD"], { cwd: ROOT, encoding: "utf8" }).trim();
  fs.writeFileSync(path.join(COMPAT, "probe.json"), JSON.stringify({ commit, node: process.version, wasmtime,
    date: new Date().toISOString().slice(0, 10), shipped: [...ours].sort(), buildErrors, results }, null, 0).replace(/\],"/g, '],\n"') + "\n");
  fs.rmSync(tmp, { recursive: true, force: true });
  console.log(`${list.length} checks; node present ${node.filter((v) => v === 1).length}, js2wasi3 present ${wasm.filter((v) => v === 1).length}`);
}

// Functions js2wasi3 ships as stubs that throw "… isn't available in js2wasi3 (why)" (`unavailable.js`'s `fn` and
// `fns` in crates/frontend/node/*.js, found in the sources): Map "<module>:<name>" → why.
function refusals() {
  const dir = path.join(ROOT, "crates/frontend/node");
  const out = new Map();
  for (const f of fs.readdirSync(dir).filter((f) => f.endsWith(".js"))) {
    const src = fs.readFileSync(path.join(dir, f), "utf8");
    const spec = f.replace(/\.js$/, "");
    const why = src.match(/const WHY = "([^"]+)"/)?.[1] ?? "";
    const put = (name) => out.set(`${spec}:${name}`, why);
    const strings = (text) => [...text.matchAll(/"([^"]+)"/g)].map((x) => x[1]);
    // A list: a literal `[…]`, or a `const NAME = […]` of this file.
    const list = (expr) => {
      expr = expr.trim();
      if (expr.startsWith("[")) return strings(expr);
      const name = expr.match(/^(\w+)/)?.[1];
      const body = src.match(new RegExp(`const ${name} = \\[([^\\]]*)\\]`, "s"))?.[1];
      return body === undefined ? [] : strings(body);
    };
    // fn("owner", "name", WHY)
    for (const m of src.matchAll(/\bfn\(\s*"[^"]+",\s*"([^"]+)"/g)) put(m[1]);
    // fns(target, "owner", <list>, WHY); `NAMES.map((n) => \`create${n}\`)`
    for (const m of src.matchAll(/\bfns\([^,]+,\s*[^,]+,\s*(\[[^\]]*\]|\w+(?:\.map\(\(\w+\) => `(\w*)\$\{\w+\}`\))?)\s*,\s*WHY\)/g)) {
      const prefix = m[2] ?? "";
      for (const n of list(m[1].replace(/\.map.*/, ""))) put(prefix + n);
    }
    // for (const name of <list>) fns(module.exports, "zlib", [name, `${name}Sync`], WHY)
    for (const m of src.matchAll(/for \(const (\w+) of (\[[^\]]*\]|\w+)\) fns\([^,]+,\s*[^,]+,\s*\[\1, `\$\{\1\}(\w+)`\]/g)) {
      for (const n of list(m[2])) [n, n + m[3]].forEach(put);
    }
    // Classes whose constructor throws: `for (const name of <list>) { … class … throw unavailableError(…) }`
    for (const m of src.matchAll(/for \(const (\w+) of (\[[^\]]*\]|\w+)\) \{[^]{0,200}?throw unavailableError/g)) {
      for (const n of list(m[2])) put(n);
    }
    // `class X { constructor() { throw unavailableError(…) } }` and classes extending such a class.
    const throwing = new Set();
    for (const m of src.matchAll(/class (\w+)[^{]*\{\s*constructor\([^)]*\)\s*\{\s*(?:super\(\);\s*)?throw unavailableError/g)) throwing.add(m[1]);
    for (const m of src.matchAll(/class (\w+) extends (\w+)\s*\{\}/g)) if (throwing.has(m[2])) put(m[1]);
    for (const n of throwing) if (n !== "Unavailable" && n !== "extends") put(n);
    // fs.js: `watch: unsupported("watch")` (throws "fs.watch isn't supported by js2wasi3 yet").
    for (const m of src.matchAll(/\bunsupported\("(\w+)"\)/g)) out.set(`${spec}:${m[1]}`, "not supported yet");
    // dgram.js: methods that throw libuv's ENOSYS (no broadcast or multicast over WASI 0.3).
    for (const m of src.matchAll(/throw errnoException\("ENOSYS", "(\w+)"\)/g)) out.set(`${spec}:${m[1]}`, "ENOSYS: no broadcast or multicast over WASI 0.3");
    // throw unavailableError(`dns.Resolver`, WHY): the last name, when it's literal.
    for (const m of src.matchAll(/unavailableError\(\s*[`"]([^`"]+)[`"]/g)) {
      const last = m[1].split(".").pop();
      if (/^[A-Za-z_]\w*$/.test(last)) put(last);
    }
  }
  // zlib.js keeps its stubs for programs that do not link the helper (T150), but a program naming `zlib` gets the real thing:
  // only Brotli and zstd (T150 slices 2 and 3) still throw.
  for (const k of [...out.keys()]) if (k.startsWith("zlib:") && !/brotli|zstd/i.test(k)) out.delete(k);
  return out;
}

// The fixtures `scripts/diff.sh` checks against node (a `.skip` file skips one): their sources.
function fixtures() {
  const out = [];
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const f = path.join(d, e.name);
      if (e.isDirectory()) { if (e.name !== "node_modules") walk(f); }
      else if (/\.(js|mjs|cjs)$/.test(e.name) && !fs.existsSync(f.replace(/\.(js|mjs|cjs)$/, ".skip"))) {
        out.push({ file: path.relative(ROOT, f), src: fs.readFileSync(f, "utf8") });
      }
    }
  };
  walk(path.join(ROOT, "tests/fixtures"));
  return out;
}

// The first fixture that loads module `spec` (or, for a global, names the global `root`) and names `name`.
function fixtureFor(fx, spec, root, name) {
  const word = (w) => new RegExp(`(?<![\\w$])${w.replace(/[$]/g, "\\$")}(?![\\w$])`);
  const n = word(name);
  const loads = spec === "" ? ((r) => (src) => r.test(src))(word(root))
    : (src) => src.includes(`"node:${spec}"`) || src.includes(`'node:${spec}'`) || src.includes(`"${spec}"`) || src.includes(`'${spec}'`);
  return fx.find((f) => loads(f.src) && n.test(f.src))?.file;
}

// How a path reads on the page: ["fs", "promises", "access"] → fs.promises.access; "" → the global.
const show = (p) => (p[0] === "" ? p.slice(1) : p).map((k) => (k.startsWith("@@") ? `[Symbol.${k.slice(2)}]` : k)).join(".").replace(/\.\[/g, "[");

function classify() {
  const inv = JSON.parse(fs.readFileSync(path.join(COMPAT, "inventory.json"), "utf8"));
  const pr = JSON.parse(fs.readFileSync(path.join(COMPAT, "probe.json"), "utf8"));
  const partial = JSON.parse(fs.readFileSync(path.join(COMPAT, "partial.json"), "utf8"));
  const ours = new Set(pr.shipped);
  const refused = refusals();
  const fx = fixtures();
  const rows = inv.rows.map((row) => {
    const c = checks(row);
    const r = { chapter: row.chapter, kind: row.kind, api: row.text.replace(/\s+Type:.*$/s, "").replace(/\s+/g, " ").trim(), stability: row.stability };
    if (c.na) return { ...r, status: "n/a", basis: "not an API of a running program", note: c.na };
    if (c.skip) return { ...r, status: "not probed", basis: c.skip };
    const res = c.paths.map((p) => [p, pr.results[JSON.stringify(p)] ?? [0, 0]]);
    // Of the paths node has, prefer one js2wasi3 can reach (a shipped module or a global): `console.log` is
    // the global even though `node:console` isn't shipped.
    const hit = res.find(([p, v]) => v[0] === 1 && (p[0] === "" || ours.has(p[0]))) ?? res.find(([, v]) => v[0] === 1);
    if (!hit) {
      const why = res.every(([, v]) => v[0] === 2) ? "node: module not loadable without flags"
        : c.instance ? "instance property (not on the prototype)" : "not reachable by name in node";
      return { ...r, status: "not probed", basis: why };
    }
    const [p, [, w]] = hit;
    r.path = show(p);
    const spec = p[0];
    if (spec !== "" && !ours.has(spec)) return { ...r, status: "no module", basis: "registry: not a module js2wasi3 ships (a build error)" };
    const be = pr.buildErrors?.[JSON.stringify(p)];
    if (be) return { ...r, status: "missing", basis: "build error where named", note: be.replace(/^unsupported: /, "") };
    if (w !== 1) return { ...r, status: "missing", basis: "probe: absent in js2wasi3" };
    const name = p[p.length - 1].replace(/^@@/, "");
    const why = refused.get(`${spec || "globals"}:${name}`) ?? (spec === "" ? undefined : refused.get(`${CHAPTERS[row.chapter]?.spec}:${name}`));
    if (why !== undefined && spec !== "") return { ...r, status: "refused", basis: "stub: throws when called", note: why };
    if (partial[r.path]) return { ...r, status: "partial", basis: "declared (docs/compat/partial.json)", note: partial[r.path] };
    const f = fixtureFor(fx, spec, p[1], name);
    if (f) return { ...r, status: "supported", basis: "test-backed", test: f };
    return { ...r, status: "present", basis: "probe: present, no fixture names it" };
  });
  for (const k of Object.keys(partial)) if (!rows.some((r) => r.path === k)) console.error(`partial.json: no row ${k}`);
  return { inv, pr, rows };
}

// test262 by directory (`tests/test262/baseline.json`): built-ins/<X>, language/<X>, annexB, intl402.
function language() {
  const file = path.join(ROOT, "tests/test262/baseline.json");
  const base = JSON.parse(fs.readFileSync(file, "utf8"));
  const date = execFileSync("git", ["log", "-1", "--format=%cs", "--", file], { cwd: ROOT, encoding: "utf8" }).trim();
  // Only the agreed subset (`SUBSET` in scripts/test262.mjs): `--update` merges runs, so the baseline also keeps
  // results of other directories run once.
  const runner = fs.readFileSync(path.join(ROOT, "scripts/test262.mjs"), "utf8");
  const subset = [...runner.slice(runner.indexOf("const SUBSET = ["), runner.indexOf("];", runner.indexOf("const SUBSET = ["))).matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  const dirs = new Map();
  const total = { pass: 0, run: 0, skip: 0 };
  for (const [test, res] of Object.entries(base)) {
    if (!subset.some((d) => test.startsWith(`${d}/`))) continue;
    const parts = test.split("/");
    const dir = parts[0] === "built-ins" || parts[0] === "language" ? parts.slice(0, 2).join("/") : parts[0];
    const d = dirs.get(dir) ?? { dir, pass: 0, run: 0, skip: 0 };
    dirs.set(dir, d);
    for (const t of [d, total]) {
      if (res === "skip") t.skip++;
      else {
        t.run++;
        if (res === "pass") t.pass++;
      }
    }
  }
  return { date, total, dirs: [...dirs.values()].sort((x, y) => x.dir.localeCompare(y.dir)) };
}

// `node scripts/compat.mjs bench <results.tsv> [machine]`: keep a `scripts/bench.sh` run's results for the page
// (run it where the benchmarks ran: it records that machine's wasmtime and node).
function benchImport(file, machine) {
  const lines = fs.readFileSync(file, "utf8").trim().split("\n").slice(1);
  const min = (xs) => Math.min(...xs);
  const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
  const rows = lines.map((l) => {
    const [program, arg, input, node, wasm, same] = l.split("\t");
    const n = node.split(" ").map(Number), w = wasm.split(" ").map(Number);
    return { program, arg: arg || input, node_ms: min(n), wasm_ms: min(w), node_median_ms: median(n),
      wasm_median_ms: median(w), runs: w.length, same: same === "SAME" };
  });
  const commit = execFileSync("git", ["rev-parse", "--short", "HEAD"], { cwd: ROOT, encoding: "utf8" }).trim();
  const out = { date: new Date().toISOString().slice(0, 10), commit, wasmtime: execFileSync("wasmtime", ["--version"], { encoding: "utf8" }).trim(),
    node: process.version, machine: machine ?? null, how: "scripts/bench.sh (default flags: 16 MiB GC heap reserved at start), minimum of the runs", rows };
  fs.writeFileSync(path.join(COMPAT, "benchmarks.json"), JSON.stringify(out, null, 1) + "\n");
  console.log(`${rows.length} programs`);
}

const STATUSES = ["supported", "present", "partial", "refused", "missing", "no module", "not probed", "n/a"];
const LEGEND = {
  supported: "present in js2wasi3 and named in a passing fixture (each fixture is checked against node)",
  present: "present in js2wasi3; no fixture names it yet",
  partial: "present, with a known gap (see the note)",
  refused: "present, but throws when called: the facility doesn't exist under WASI or isn't built yet",
  missing: "node has it, js2wasi3 doesn't (or rejects it at build time)",
  "no module": "the module isn't one js2wasi3 ships: importing it is a build error",
  "not probed": "events, instance-only properties, and names not reachable from a module or global",
  "n/a": "not an API of a running program",
};

function render() {
  const { inv, pr, rows } = classify();
  const lang = language();
  const benchFile = path.join(COMPAT, "benchmarks.json");
  const bench = fs.existsSync(benchFile) ? JSON.parse(fs.readFileSync(benchFile, "utf8")) : null;
  const chapters = [];
  for (const r of rows) {
    let c = chapters.find((x) => x.chapter === r.chapter);
    if (!c) {
      const t = inv.rows.find((x) => x.chapter === r.chapter).title;
      c = { chapter: r.chapter, title: t, spec: CHAPTERS[r.chapter]?.spec ?? null, counts: {}, rows: [] };
      chapters.push(c);
    }
    c.counts[r.status] = (c.counts[r.status] ?? 0) + 1;
    c.rows.push(r);
  }
  chapters.sort((a, b) => a.title.localeCompare(b.title));
  const totals = {};
  for (const r of rows) totals[r.status] = (totals[r.status] ?? 0) + 1;
  const meta = { generated: pr.date, commit: pr.commit, node: inv.node, source: inv.source, wasmtime: pr.wasmtime,
    shipped: pr.shipped, statuses: STATUSES, legend: LEGEND };
  const data = { meta, totals, chapters, language: lang, benchmarks: bench };
  fs.writeFileSync(path.join(COMPAT, "compatibility.json"), JSON.stringify(data) + "\n");

  // docs/compatibility.md
  const md = [];
  const esc = (x) => String(x ?? "").replace(/\|/g, "\\|").replace(/</g, "&lt;");
  const pct = (a, b) => (b ? `${((100 * a) / b).toFixed(1)}%` : "–");
  md.push("# Node.js compatibility", "");
  md.push(`Generated by \`node scripts/compat.mjs\` on ${pr.date} at ${pr.commit} (do not edit by hand). Also on the web: <https://harryzz.github.io/js2wasi3/>.`, "");
  md.push(`Every API in node ${inv.node}'s documentation ([all.json](${inv.source}), ${inv.rows.length} rows) checked against js2wasi3: a probe program`,
    `tests each name under node and under js2wasi3 on ${pr.wasmtime.split(" (")[0]}; stubs that throw are found in js2wasi3's sources; a row is`,
    "test-backed when a fixture that `scripts/diff.sh` checks against node names it. Presence isn't conformance: a present API",
    "can still differ from node in details. Language conformance is the test262 section below.", "");
  md.push("| Status | Rows | Meaning |", "|---|---|---|");
  for (const s of STATUSES) md.push(`| ${s} | ${totals[s] ?? 0} | ${LEGEND[s]} |`);
  md.push("", "## By module", "", `| Module | ${STATUSES.join(" | ")} |`, `|---|${STATUSES.map(() => "---").join("|")}|`);
  for (const c of chapters) md.push(`| [${esc(c.title)}](#${c.chapter.replace(/[^a-z0-9]+/gi, "-").toLowerCase()}) | ${STATUSES.map((s) => c.counts[s] ?? "").join(" | ")} |`);
  md.push("", "## Language (test262)", "", `test262 subset, \`tests/test262/baseline.json\` of ${lang.date}: **${lang.total.pass} of ${lang.total.run}** tests run pass (${pct(lang.total.pass, lang.total.run)}), ${lang.total.skip} skipped. History: [tests/test262/HISTORY.md](../tests/test262/HISTORY.md); keywords: [docs/keywords.md](keywords.md).`, "");
  md.push("| Directory | Pass | Run | Pass % |", "|---|---|---|---|");
  for (const d of lang.dirs) md.push(`| ${d.dir} | ${d.pass} | ${d.run} | ${pct(d.pass, d.run)} |`);
  md.push("", "## Benchmarks", "");
  if (bench) {
    md.push(`\`scripts/bench.sh\` on ${bench.date} at ${bench.commit}${bench.machine ? ` on ${bench.machine}` : ""}, ${bench.wasmtime.split(" (")[0]}, node ${bench.node}; ${bench.how}. Programs: [tests/bench](../tests/bench/README.md).`, "");
    md.push("| Program | Input | node | js2wasi3 | × node | Output = node |", "|---|---|---|---|---|---|");
    for (const b of bench.rows) md.push(`| ${b.program} | ${b.arg} | ${(b.node_ms / 1000).toFixed(2)} s | ${(b.wasm_ms / 1000).toFixed(2)} s | ${(b.wasm_ms / b.node_ms).toFixed(1)} | ${b.same ? "yes" : "no"} |`);
  } else md.push("Not measured yet: `scripts/bench.sh -r 3`, then `node scripts/compat.mjs bench <out/bench/results.tsv>`.");
  md.push("", "## APIs", "");
  for (const c of chapters) {
    md.push(`### ${c.title}`, "", `<a id="${c.chapter.replace(/[^a-z0-9]+/gi, "-").toLowerCase()}"></a>${c.spec ? `\`node:${c.spec}\`. ` : ""}${STATUSES.filter((s) => c.counts[s]).map((s) => `${s} ${c.counts[s]}`).join(", ")}.`, "");
    md.push("<details><summary>Rows</summary>", "", "| API | Kind | Status | How we know | Note |", "|---|---|---|---|---|");
    for (const r of c.rows) md.push(`| \`${esc(r.api)}\` | ${r.kind} | ${r.status} | ${esc(r.basis)}${r.test ? ` (\`${r.test}\`)` : ""} | ${esc(r.note)} |`);
    md.push("", "</details>", "");
  }
  fs.writeFileSync(path.join(ROOT, "docs/compatibility.md"), md.join("\n"));

  // site/ (GitHub Pages): the page and its data.
  const site = path.join(ROOT, "site");
  fs.mkdirSync(site, { recursive: true });
  fs.writeFileSync(path.join(site, "compatibility.json"), JSON.stringify(data) + "\n");
  fs.writeFileSync(path.join(site, ".nojekyll"), "");
  fs.copyFileSync(path.join(ROOT, "scripts/compat-page.html"), path.join(site, "index.html"));
  console.log(STATUSES.map((s) => `${s} ${totals[s] ?? 0}`).join(", "));
}

const [cmd, ...args] = process.argv.slice(2);
if (cmd === "inventory") inventory(args[0]);
else if (cmd === "probe") probe();
else if (cmd === "render") render();
else if (cmd === "bench") benchImport(args[0], args[1]);
else if (cmd === undefined) {
  probe();
  render();
}
else if (cmd === "summary") {
  const { rows } = classify();
  const t = {};
  for (const r of rows) t[r.status] = (t[r.status] ?? 0) + 1;
  console.log(t);
}
else if (!["debug", "find", "refusals"].includes(cmd)) {
  console.error("usage: node scripts/compat.mjs inventory <all.json> | probe | render");
  process.exit(2);
}
if (cmd === "debug") {
  const { rows } = classify();
  const np = {};
  for (const r of rows.filter((r) => r.status === "not probed")) np[r.basis] = (np[r.basis] ?? 0) + 1;
  console.log(np);
  const by = (st) => rows.filter((r) => r.status === st);
  const sample = (a, n) => a.filter((_, i) => i % Math.max(1, Math.floor(a.length / n)) === 0).slice(0, n);
  console.log("MISSING", sample(by("missing"), 30).map((r) => `${r.chapter} | ${r.api} | ${r.path}`));
  console.log("UNREACHABLE", sample(rows.filter((r) => r.basis === "not reachable by name in node"), 25).map((r) => `${r.chapter} | ${r.kind} | ${r.api}`));
}
if (cmd === "refusals") console.log([...refusals()].map(([k, v]) => `${k} (${v})`).join("\n"));
if (cmd === "find") {
  const { rows } = classify();
  const re = new RegExp(args[0]);
  for (const r of rows.filter((r) => re.test(r.path ?? r.api))) console.log(`${r.status.padEnd(10)} ${r.path ?? r.api} | ${r.basis}${r.note ? " | " + r.note : ""}${r.test ? " | " + r.test : ""}`);
}
