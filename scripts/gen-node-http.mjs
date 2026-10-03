// Generate js2wasi3's HTTP server built-ins (T71) from node's own lib, as the pinned node (TOOLS.md) has it
// built in (`process.binding("natives")`), like `gen-node-streams.mjs` (T60).
//
//   node scripts/gen-node-http.mjs      (writes crates/frontend/node/http/…)
//
// Copied: `_http_common`, `_http_incoming`, `_http_outgoing`, `_http_server`, `internal/http`.
// Rewritten: `primordials` → `require("node:internal/primordials")`, `require('x')` → `require("node:x")`,
// node's internals mapped to js2wasi3's modules (`INTERNALS`): `internalBinding('http_parser')` is
// `node/http_parser.js` (a JS llhttp), the rest `node/http_support.js` and `node/http_errors.js`; then
// `PATCHES` (on node's text; each must match once). Rerun after a node update and check the diff.
import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dirname, "..");
const OUT = path.join(ROOT, "crates/frontend/node/http");
const natives = process.binding("natives");

const SUPPORT = "internal/http_support";
const INTERNALS = {
  "internal/async_hooks": SUPPORT,
  "internal/perf/observe": SUPPORT,
  "internal/util/debuglog": SUPPORT,
  "internal/freelist": SUPPORT,
  "internal/options": SUPPORT,
  "internal/assert": SUPPORT,
  "internal/timers": SUPPORT,
  "internal/constants": SUPPORT,
  "diagnostics_channel": SUPPORT,
  "timers": SUPPORT,
  "internal/util": SUPPORT,
  "internal/errors": "internal/http_errors",
  "internal/abort_controller": "internal/abort",
  "internal/util/types": "internal/util",
};
const BINDINGS = {
  http_parser: "internal/http_parser",
  trace_events: SUPPORT,
};

// [file, from, to]: js2wasi3's changes to node's code.
const PATCHES = [
  // `net.Server` is a class in js2wasi3 (T66): `Server` extends it instead of calling it.
  [
    "_http_server",
    "function Server(options, requestListener) {\n  if (!(this instanceof Server)) return new Server(options, requestListener);\n",
    "// js2wasi3: a class over net.Server's class (callable only with `new`).\n" +
      "class Server extends net.Server {\nconstructor(options, requestListener) {\n",
  ],
  [
    "_http_server",
    "  storeHTTPOptions.call(this, options);\n\n  net.Server.call(\n    this,\n    {",
    "  super({",
  ],
  [
    "_http_server",
    "      highWaterMark: options.highWaterMark });\n",
    "      highWaterMark: options.highWaterMark });\n  storeHTTPOptions.call(this, options);\n",
  ],
  [
    "_http_server",
    "  this[kUniqueHeaders] = parseUniqueHeadersOption(options.uniqueHeaders);\n}\n",
    "  this[kUniqueHeaders] = parseUniqueHeadersOption(options.uniqueHeaders);\n}\n}\n",
  ],
  [
    "_http_server",
    "  await promisify(this.close).call(this);",
    "  await new Promise((resolve, reject) => this.close((err) => (err ? reject(err) : resolve())));",
  ],
  // js2wasi3's socket handles are WASI handles (numbers): the socket itself starts and stops reading.
  [
    "_http_server",
    "\n  if (this._handle && !this._handle.reading) {\n    this._handle.reading = true;\n    this._handle.readStart();\n  }\n",
    "\n",
  ],
  [
    "_http_server",
    "function onSocketPause() {\n  if (this._handle?.reading) {\n    this._handle.reading = false;\n    this._handle.readStop();\n  }\n}",
    "function onSocketPause() {}",
  ],
  // `internal/assert` is a function in node.
  ["_http_server", 'const assert = require("node:internal/http_support");', 'const { assert } = require("node:internal/http_support");'],
  ["_http_outgoing", 'const assert = require("node:internal/http_support");', 'const { assert } = require("node:internal/http_support");'],
  ["_http_common", 'const FreeList = require("node:internal/http_support");', 'const { FreeList } = require("node:internal/http_support");'],
  ["_http_server", 'const dc = require("node:internal/http_support");', 'const dc = require("node:internal/http_support"); // diagnostics_channel'],
];

const NAMES = ["_http_common", "_http_incoming", "_http_outgoing", "_http_server", "internal/http"];
const file = (name) => (name === "internal/http" ? "internal_http.js" : `${name}.js`);

// Primordials the shim (`node/stream/primordials.js`, shared with the streams) lacks: local definitions
// (none: the global itself).
const LOCAL = {
  NumberIsFinite: "Number.isFinite",
  ObjectHasOwn: "Object.hasOwn",
  ObjectValues: "Object.values",
  Array: null,
  Date: null,
  decodeURIComponent: null,
};

fs.mkdirSync(OUT, { recursive: true });
for (const name of NAMES) {
  let s = natives[name];
  s = s.replace(/^const \{([^]*?)\} = primordials;$/m, (_, list) => {
    const names = list.split(",").map((x) => x.trim()).filter(Boolean);
    const kept = names.filter((n) => !(n in LOCAL));
    const local = names.filter((n) => LOCAL[n]).map((n) => `const ${n} = ${LOCAL[n]}; // js2wasi3\n`);
    return `const {\n${kept.map((n) => `  ${n},\n`).join("")}} = require("node:internal/primordials");\n${local.join("")}`;
  });
  s = s.replace(/require\('([^']+)'\)/g, (_, spec) => `require("node:${INTERNALS[spec] ?? spec}")`);
  s = s.replace(/internalBinding\('([^']+)'\)/g, (_, b) => {
    if (!BINDINGS[b]) throw new Error(`${name}: internalBinding('${b}')`);
    return `require("node:${BINDINGS[b]}")`;
  });
  for (const [f, from, to] of PATCHES) {
    if (f !== name) continue;
    const before = s;
    s = s.replace(from, to);
    if (s === before) throw new Error(`patch for ${name} didn't match: ${from}`);
  }
  if (/\binternalBinding\b/.test(s)) throw new Error(`${name}: internalBinding left`);
  if (/\.stack\b/.test(s)) throw new Error(`${name}: reads \`stack\``);
  if (/\bprimordials\b/.test(s.replace(/"node:internal\/primordials"/g, ""))) throw new Error(`${name}: primordials left`);
  const header = `// node ${process.version}'s lib/${name}.js (MIT; generated by scripts/gen-node-http.mjs).\n`;
  fs.writeFileSync(path.join(OUT, file(name)), header + s);
}
// Every primordial the files take must be in the shim.
const shim = fs.readFileSync(path.join(ROOT, "crates/frontend/node/stream/primordials.js"), "utf8");
for (const name of NAMES) {
  const m = fs.readFileSync(path.join(OUT, file(name)), "utf8").match(/const \{([^}]*)\} = require\("node:internal\/primordials"\)/);
  for (const p of m ? m[1].split(",").map((x) => x.trim().split(":")[0].trim()).filter(Boolean) : []) {
    if (!new RegExp(`\\b${p}\\b`).test(shim)) throw new Error(`${name}: primordial ${p} missing from primordials.js`);
  }
}
for (const name of NAMES) console.log(`    ("${name}", include_str!("../node/http/${file(name)}")),`);
