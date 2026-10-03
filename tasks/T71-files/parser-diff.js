// T71: js2wasi3's JS HTTPParser (crates/frontend/node/http_parser.js) against node's llhttp binding.
//   node --expose-internals --no-warnings tasks/T71-files/parser-diff.js [--pos] [--splits]
// Each case runs in one piece (and with --splits at every split point into two pieces); the callbacks,
// their arguments and execute/finish results must match (error code; with --pos also reason and bytesParsed).
"use strict";
const path = require("path");
const { internalBinding } = require("internal/test/binding");
const Real = internalBinding("http_parser");
const Mine = require(path.join(__dirname, "../../crates/frontend/node/http_parser.js"));
const POS = process.argv.includes("--pos");
const SPLITS = process.argv.includes("--splits");

function run(B, chunks, o) {
  const p = new B.HTTPParser();
  const log = [];
  p[0] = () => log.push(["begin"]);
  p[1] = (h, u) => log.push(["headers", h, u]);
  p[2] = (...a) => {
    log.push(["hc", ...a]);
    return o.hcRet ?? 0;
  };
  p[3] = (b) => log.push(["body", b.toString("latin1")]);
  p[4] = () => log.push(["complete"]);
  const list = o.list ? new B.ConnectionsList() : undefined;
  p.initialize(B.HTTPParser.REQUEST, {}, o.max || 0, 0, list);
  // Bodies may come in pieces: join adjacent ones.
  const push = (e) => {
    const last = log[log.length - 1];
    if (e[0] === "body" && last && last[0] === "body") last[1] += e[1];
    else log.push(e);
  };
  p[3] = (b) => push(["body", b.toString("latin1")]);
  let total = 0;
  for (const c of chunks) {
    const r = p.execute(Buffer.from(c, "latin1"));
    if (r instanceof Error) {
      log.push(["ERR", r.code, ...(POS ? [r.reason, r.bytesParsed + total] : [])]);
      break;
    }
    total += r;
    if (r !== c.length) {
      log.push(["stop", total]);
      break;
    }
  }
  if (o.finish) {
    const r = p.finish();
    log.push(r instanceof Error ? ["FERR", r.code] : ["fret", r]);
  }
  if (list) log.push(["list", list.all().length, list.active().length, list.idle().length]);
  // A flush at a chunk boundary is the same as one at the end: join kOnHeaders runs.
  return JSON.stringify(log);
}

const H = "GET / HTTP/1.1\r\nHost: a\r\n\r\n";
const cases = [
  [H, {}], [H + H, {}], [H, { finish: true, list: true }], ["GET / HTTP/1.1\r\nHost: a", { finish: true, list: true }],
  ["POST /x?y=1 HTTP/1.1\r\nHost: a\r\nContent-Length: 5\r\n\r\nhello" + H, {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6;ext=1\r\n world\r\n0\r\nT: 1\r\n\r\n" + H, {}],
  ["GET / HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: x\r\n\r\n\x81\x05hello", {}],
  ["GET / HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nContent-Length: 2\r\n\r\nabZZ", { hcRet: 2 }],
  ["GET / HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nContent-Length: 2\r\n\r\nabZZ", {}],
  ["GET / HTTP/1.1\r\nContent-Length: 2\r\n\r\nab" + H, { hcRet: 1 }],
  ["CONNECT a:1 HTTP/1.1\r\n\r\nXYZ", {}], ["OPTIONS * HTTP/1.1\r\n\r\n", {}], ["M-SEARCH * HTTP/1.1\r\n\r\n", {}],
  ["GET http://x/y?z#f HTTP/1.1\r\n\r\n", {}], ["GET / HTTP/1.0\r\n\r\n", {}], ["GET / HTTP/1.0\r\nConnection: keep-alive\r\n\r\n" + H, {}],
  ["GET / HTTP/1.1\r\nConnection: close\r\n\r\n" + H, {}], ["GET / HTTP/1.1\r\nConnection: close\r\n\r\n\r\n", { finish: true }],
  ["GET /\r\n\r\n", {}], ["GET / RTSP/1.0\r\n\r\n", {}], ["SOURCE / ICE/1.0\r\n\r\n", {}], ["\r\n\n" + H, {}],
  ["GET / HTTP/1.1\r\n" + Array.from({ length: 70 }, (_, i) => `H${i}: v${i}\r\n`).join("") + "\r\n", {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n1\r\na\r\n0\r\n" + Array.from({ length: 40 }, (_, i) => `T${i}: v\r\n`).join("") + "\r\n", {}],
  ["GET / HTTP/1.1\r\nA:\tb\t \r\nB:\r\nC:  c d  \r\n\r\n", {}],
  ["GET / HTTP/1.1\r\nA: " + "x".repeat(100) + "\r\n\r\n", { max: 50 }],
  ["GET /" + "x".repeat(100) + " HTTP/1.1\r\n\r\n", { max: 50 }],
  ["GET / HTTP/1.1\r\n" + "x".repeat(100) + ": y\r\n\r\n", { max: 50 }],
  ["GET / HTTP/1.1\r\nA: " + "x".repeat(17000) + "\r\n\r\n", {}],
  // errors
  ["get / HTTP/1.1\r\n\r\n", {}], ["GETX / HTTP/1.1\r\n\r\n", {}], ["GEX / HTTP/1.1\r\n\r\n", {}], ["GET\r\n", {}],
  ["DESCRIBE / HTTP/1.1\r\n\r\n", {}], ["GET / ICE/1.1\r\n\r\n", {}], ["GET / HTTP/1.2\r\n\r\n", {}], ["GET / HTTP/1.1 \r\n", {}],
  ["GET / HTTP/1.1\nHost: a\n\n", {}], ["GET / HTTP/1.1\rX", {}], ["GET /a b HTTP/1.1\r\n\r\n", {}], ["GET 1 HTTP/1.1\r\n\r\n", {}],
  ["GET a HTTP/1.1\r\n\r\n", {}], ["GET /\x01 HTTP/1.1\r\n\r\n", {}], ["GET /\xe9 HTTP/1.1\r\n\r\n", {}], ["GET /?\x7f HTTP/1.1\r\n\r\n", {}],
  ["GET / HTTP/1.1\r\nA : b\r\n\r\n", {}], ["GET / HTTP/1.1\r\n: b\r\n\r\n", {}], ["GET / HTTP/1.1\r\nA\x01: b\r\n\r\n", {}],
  ["GET / HTTP/1.1\r\nA: \x01\r\n\r\n", {}], ["GET / HTTP/1.1\r\nA: \xe9\x7f\r\n\r\n", {}], ["GET / HTTP/1.1\r\n A: b\r\n\r\n", {}],
  ["GET / HTTP/1.1\r\nA: b\r\n c\r\n\r\n", {}], ["GET / HTTP/1.1\r\nA: b\rc\r\n\r\n", {}], ["GET / HTTP/1.1\r\nA: b\n\r\n", {}],
  ["GET / HTTP/1.1\r\nA: b\r\n\n", {}], ["GET / HTTP/1.1\r\nHost: a\r\n\rX", {}], ["GET / HTTP/1.1\r\nHost: a\r\nX\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\n", {}], ["POST / HTTP/1.1\r\nContent-Length: 1x\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nContent-Length: 1\r\nTransfer-Encoding: chunked\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: 1\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nContent-Length: 99999999999999999999999\r\n\r\n", {}], ["POST / HTTP/1.1\r\nContent-Length: 5 5\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nContent-Length:\r\n\r\n", {}], ["POST / HTTP/1.1\r\nContent-Length: +1\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nContent-Length:  05 \r\n\r\nhello", {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: gzip\r\n\r\n", {}], ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked, gzip\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: gzip, chunked\r\n\r\n0\r\n\r\n", {}], ["POST / HTTP/1.1\r\nTransfer-Encoding: CHUNKED \r\n\r\n0\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\nTransfer-Encoding: chunked\r\n\r\n", {}], ["POST / HTTP/1.1\r\nTransfer-Encoding:\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nZ\r\n", {}], ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabcX", {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\n0\r\n\r\n", {}], ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\nabc\r\n0\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3 \r\nabc\r\n0\r\n\r\n", {}], ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffffff\r\n", {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0000000000000000003\r\nabc\r\n0\r\n\r\n", {}],
  ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX\r\n\r\n", {}], ["POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\n", {}],
  ["POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nab", { finish: true }], ["GE", { finish: true }], [" GET / HTTP/1.1\r\n\r\n", {}],
  ["GET / HTTP/1.1\r\nTransfer-Encoding: chunkedx\r\n\r\n", {}],
  ["GET / HTTP/1.1\r\nUpgrade: x\r\nConnection: upgrade\r\nTransfer-Encoding: chunked\r\n\r\n1\r\na\r\n0\r\n\r\nZZ", {}],
];

let fail = 0;
let n = 0;
for (const [input, o] of cases) {
  const splits = [[input]];
  if (SPLITS) for (let k = 1; k < input.length; k += input.length > 400 ? 97 : 1) splits.push([input.slice(0, k), input.slice(k)]);
  for (const chunks of splits) {
    n++;
    const a = run(Real, chunks, o);
    const b = run(Mine, chunks, o);
    if (a !== b) {
      fail++;
      if (fail <= 15) {
        console.log("DIFF", JSON.stringify(chunks).slice(0, 160), JSON.stringify(o));
        console.log("  node:", a.slice(0, 400));
        console.log("  mine:", b.slice(0, 400));
      }
    }
  }
}
console.log(`${n - fail}/${n} equal`);
process.exitCode = fail ? 1 : 0;
