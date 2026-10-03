const { internalBinding } = require("internal/test/binding");
const { HTTPParser, ConnectionsList } = internalBinding("http_parser");
function run(input, opts = {}) {
  const p = new HTTPParser();
  const log = [];
  p[0] = () => log.push(["begin"]);
  p[1] = (h, u) => log.push(["headers", h, u]);
  p[2] = (...a) => { log.push(["hc", ...a]); return opts.hcRet ?? 0; };
  p[3] = (b) => log.push(["body", b.toString("latin1")]);
  p[4] = () => log.push(["complete"]);
  p.initialize(HTTPParser.REQUEST, {}, opts.max || 0, opts.lenient || 0);
  const chunks = Array.isArray(input) ? input : [input];
  for (const c of chunks) {
    const r = p.execute(Buffer.from(c, "latin1"));
    log.push(r instanceof Error ? ["ERR", r.code, r.reason, r.bytesParsed] : ["ret", r]);
  }
  if (opts.finish) { const r = p.finish(); log.push(r instanceof Error ? ["FERR", r.code, r.reason, r.bytesParsed] : ["fret", r]); }
  return log;
}
module.exports = run;
if (require.main === module) {
  for (const t of process.argv.slice(2)) console.log(JSON.stringify(t), JSON.stringify(run(JSON.parse('"' + t + '"'))));
}
