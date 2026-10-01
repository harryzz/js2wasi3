// T62 survey: CommonJS stubs for node built-ins js2wasi3 lacks. Every own key of node 24's module:
// functions throw "T62 stub" when called; JSON-able values are copied.
import { createRequire } from "node:module";
import fs from "node:fs";
const require = createRequire(import.meta.url);
const [out, ...mods] = process.argv.slice(2);
fs.mkdirSync(out, { recursive: true });
for (const mod of mods) {
  const m = require(mod);
  let s = `"use strict";\nfunction stub(n) { return function () { throw new Error("T62 stub: ${mod}." + n); }; }\n`;
  for (const k of Object.keys(m).sort()) {
    if (!/^[A-Za-z_$][\w$]*$/.test(k)) continue;
    const v = m[k];
    if (typeof v === "function") s += `exports.${k} = stub(${JSON.stringify(k)});\n`;
    else { try { const j = JSON.stringify(v); if (j !== undefined && j.length < 20000) s += `exports.${k} = ${j};\n`; else s += `exports.${k} = {};\n`; } catch { s += `exports.${k} = {};\n`; } }
  }
  fs.writeFileSync(`${out}/${mod.replace("/", "_")}.js`, s);
}
// Working stand-ins where zigbee2mqtt calls them on the way to the TCP socket.
fs.appendFileSync(`${out}/child_process.js`, `// as if the command doesn't exist (zigbee2mqtt asks git for its commit hash)
exports.exec = function exec(cmd, opts, cb) { if (typeof opts === "function") cb = opts; const e = Object.assign(new Error("spawn /bin/sh ENOENT"), { code: "ENOENT" }); setImmediate(() => cb && cb(e, "", "")); return {}; };
`);
fs.appendFileSync(`${out}/url.js`, `exports.urlToHttpOptions = function urlToHttpOptions(u) { return { protocol: u.protocol, hostname: u.hostname, hash: u.hash, search: u.search, pathname: u.pathname, path: u.pathname + u.search, href: u.href, port: u.port === "" ? undefined : Number(u.port) }; };
`);
