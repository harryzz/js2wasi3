const path = require("node:path"); const pkg = process.argv[2];
const Ajv = require(path.join(pkg, "node_modules/ajv")).default;
const schema = require(path.join(pkg, "dist/util/settings.schema.json"));
const a = new Ajv({ allErrors: true }).addKeyword({ keyword: "requiresRestart", validate: (s) => !s }).compile(schema);
const b = require(process.argv[3] + "/restart.js");
for (const x of [{}, { mqtt: { server: 1 } }, { serial: { port: "x" }, advanced: { log_level: "nope" } }]) {
  a(x); b(x); const k = (v) => JSON.stringify((v.errors || []).map((e) => [e.instancePath, e.keyword]));
  console.log(k(a) === k(b), k(a).slice(0, 120));
}
