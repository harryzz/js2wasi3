// T62: generate zigbee2mqtt's four settings validators with ajv's standalone mode (no `new Function`
// at run time, F191). The `requiresRestart` keyword's validate function `(s) => !s` is emitted as code.
const path = require("node:path"), fs = require("node:fs");
const pkg = process.argv[2], out = process.argv[3];
const Ajv = require(path.join(pkg, "node_modules/ajv")).default;
const standalone = require(path.join(pkg, "node_modules/ajv/dist/standalone")).default;
const schema = require(path.join(pkg, "dist/util/settings.schema.json"));
const code = { source: true, lines: false };
// As a code keyword (standalone mode can't emit a validate function): fails where the schema value is true.
const restart = { keyword: "requiresRestart", schemaType: "boolean", code(cxt) { if (cxt.schema) cxt.fail(); } };
const mk = (kw, s) => { const ajv = new Ajv({ allErrors: true, code }).addKeyword(kw); return standalone(ajv, ajv.compile(s)); };
const mods = {
  setting: mk("requiresRestart", schema),
  restart: mk(restart, schema),
  device: mk(restart, schema.definitions.device),
  group: mk(restart, schema.definitions.group),
};
fs.mkdirSync(out, { recursive: true });
for (const [k, v] of Object.entries(mods)) fs.writeFileSync(`${out}/${k}.js`, v);
console.log(Object.entries(mods).map(([k, v]) => `${k}: ${v.length} B`).join(", "));
