// Generate js2wasi3's `node:stream` built-ins (T60) from node's own lib, as the pinned node
// (TOOLS.md) has it built in: `process.binding("natives")`.
//
//   node scripts/gen-node-streams.mjs      (writes crates/frontend/node/stream/…)
//
// Copied: `stream`, `stream/promises`, `_stream_*`, `internal/streams/*` (not `lazy_transform`),
// `internal/fs/streams` (`fs.createReadStream`, … over `node/fs.js`).
// Rewritten: `const { … } = primordials;` → `require("node:internal/primordials")`, `require('x')` →
// `require("node:x")`, node's internals mapped to js2wasi3's support modules (`INTERNALS`; hand-written
// or from readable-stream 4.7.0, MIT: `node/stream/{primordials,errors,validators,util,util/inspect}.js`,
// `node/abort.js`), after `PATCHES` (on node's text; each must match once). Rerun after a node update and check the diff.
import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dirname, "..");
const OUT = path.join(ROOT, "crates/frontend/node/stream");
const natives = process.binding("natives");

// node's internals → the built-in that provides what the streams use of them.
const INTERNALS = {
  "internal/abort_controller": "internal/abort",
  "internal/event_target": "internal/abort",
  "internal/events/abort_listener": "internal/util",
  "internal/util/types": "internal/util",
  "internal/blob": "internal/util",
  "internal/options": "internal/util",
  "internal/util/debuglog": "internal/util",
  "internal/webstreams/adapters": "internal/util",
  "internal/buffer": "buffer",
  // internal/fs/streams (fs.createReadStream, …)
  "internal/fs/promises": "internal/fs_support",
  "internal/fs/utils": "internal/fs_support",
  "internal/url": "internal/fs_support",
};

// [file, from, to]: js2wasi3's changes to node's code.
const PATCHES = [
  // No `AsyncResource` (async_hooks): callbacks run as they are.
  [
    "internal/streams/end-of-stream",
    /function bindAsyncResource\(fn, type\) \{[^}]*\{[^}]*\};\n\}/,
    "function bindAsyncResource(fn, type) {\n  return fn; // js2wasi3: no async_hooks\n}",
  ],
  // `Buffer[Symbol.species]` (no typed-array species in js2wasi3, F248): a Buffer over the same memory.
  [
    "internal/streams/readable",
    "const FastBuffer = Buffer[SymbolSpecies];",
    "// js2wasi3: no `Buffer[Symbol.species]` (F248).\n" +
      "function FastBuffer(buffer, byteOffset, length) {\n" +
      "  return buffer === undefined ? Buffer.alloc(0) : Buffer.from(buffer, byteOffset, length);\n}",
  ],
  [
    "stream",
    /new internalBuffer\.FastBuffer\(chunk\.buffer,\s*chunk\.byteOffset,\s*chunk\.byteLength\)/,
    "internalBuffer.Buffer.from(chunk.buffer, chunk.byteOffset, chunk.byteLength)",
  ],
  // No Windows handles, node's O_* constants are Linux's.
  ["internal/fs/streams", "const binding = internalBinding('fs');", "const binding = {}; // js2wasi3: no windowsHandle"],
  [
    "internal/fs/streams",
    "const { O_RDONLY, O_WRONLY } = internalBinding('constants').fs;",
    "const O_RDONLY = 0;\nconst O_WRONLY = 1;",
  ],
  // `internal/assert`: only duplexPair's encapsulation checks.
  [
    "internal/streams/duplexpair",
    "const assert = require('internal/assert');",
    "const assert = (ok) => {\n  if (!ok) throw new Error('assertion failed');\n};",
  ],
];

const names = Object.keys(natives).filter(
  (k) =>
    k === "stream" ||
    k === "stream/promises" ||
    /^_stream_(duplex|passthrough|readable|transform|writable)$/.test(k) ||
    (/^internal\/streams\/[a-z_-]+$/.test(k) && k !== "internal/streams/lazy_transform") ||
    k === "internal/fs/streams",
);

const file = (name) =>
  name === "stream"
    ? "stream.js"
    : name === "stream/promises"
      ? "promises.js"
      : name === "internal/fs/streams"
        ? "fs_streams.js"
      : name.startsWith("internal/streams/")
        ? `streams/${name.slice("internal/streams/".length)}.js`
        : `${name}.js`;

fs.rmSync(path.join(OUT, "streams"), { recursive: true, force: true });
fs.mkdirSync(path.join(OUT, "streams"), { recursive: true });
const entries = [];
for (const name of names.sort()) {
  let s = natives[name];
  for (const [f, from, to] of PATCHES) {
    if (f !== name) continue;
    const before = s;
    s = s.replace(from, to);
    if (s === before) throw new Error(`patch for ${name} didn't match: ${from}`);
  }
  // `err.stack;` (a V8 leak workaround): reading `stack` traps in js2wasi3 (F31).
  s = s.replace(/^(\s*)(err|er)\.stack; \/\/ eslint-disable-line no-unused-expressions$/gm, "$1// js2wasi3: no read of the stack property (F31)");
  s = s.replace(/^(const \{[^]*?\}) = primordials;$/m, '$1 = require("node:internal/primordials");');
  s = s.replace(/require\('([^']+)'\)/g, (_, spec) => `require("node:${INTERNALS[spec] ?? spec}")`);
  if (/\binternalBinding\b/.test(s)) throw new Error(`${name}: internalBinding left`);
  if (/\.stack\b/.test(s)) throw new Error(`${name}: reads \`stack\``);
  if (/\bprimordials\b/.test(s.replace(/"node:internal\/primordials"/g, ""))) {
    throw new Error(`${name}: primordials left`);
  }
  const header = `// node ${process.version}'s lib/${name}.js (MIT; generated by scripts/gen-node-streams.mjs).\n`;
  fs.writeFileSync(path.join(OUT, file(name)), header + s);
  entries.push([name, file(name)]);
}
// Every primordial the files take must be in the shim.
const shim = fs.readFileSync(path.join(OUT, "primordials.js"), "utf8");
for (const [name, f] of entries) {
  const m = fs.readFileSync(path.join(OUT, f), "utf8").match(/const \{([^}]*)\} = require\("node:internal\/primordials"\)/);
  for (const p of m ? m[1].split(",").map((x) => x.trim().split(":")[0].trim()).filter(Boolean) : []) {
    if (!new RegExp(`\\b${p}\\b`).test(shim)) throw new Error(`${name}: primordial ${p} missing from primordials.js`);
  }
}
for (const [name, f] of entries) console.log(`    ("${name}", include_str!("../node/stream/${f}")),`);
