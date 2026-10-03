// T71 probe: zigbee2mqtt's staticFileServer (srvx/node + srvx/static) on node:http, raw net clients.
const http = require("http");
const net = require("net");
const path = require("path");
const { createStaticFileServer } = require("../dist/util/staticFileServer.js");
const once = (e, n) => new Promise((r) => e.once(n, (...a) => r(a)));
const mask = (s) => s.replace(/(Date|Last-Modified): [^\r]*\r\n/gi, "$1: <t>\r\n").replace(/ETag: W\/"[^"]*"/gi, 'ETag: W/"<e>"');
async function main() {
  const handler = createStaticFileServer(path.join(__dirname, "www"), (m) => console.log("logError", m));
  const server = http.createServer(handler);
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const port = server.address().port;
  const ask = async (req) => {
    const c = net.connect(port, "127.0.0.1");
    await once(c, "connect");
    const closed = once(c, "close");
    let out = "";
    c.on("data", (d) => (out += d.toString("latin1")));
    c.write(req.replace(/\n/g, "\r\n") + "Host: t\r\nConnection: close\r\n\r\n");
    await closed;
    return out;
  };
  for (const r of ["GET / HTTP/1.1\n", "GET / HTTP/1.1\nAccept-Encoding: gzip, br\n", "GET /app.js HTTP/1.1\n",
    "HEAD /app.js HTTP/1.1\n", "GET /missing HTTP/1.1\n", "GET /sub/ HTTP/1.1\n", "GET /sub HTTP/1.1\n",
    "GET /app.js HTTP/1.1\nRange: bytes=2-5\n", "POST /app.js HTTP/1.1\nContent-Length: 0\n", "GET /%E0%A4%A HTTP/1.1\n",
    "GET /../probe.js HTTP/1.1\n"]) {
    console.log(JSON.stringify(r), JSON.stringify(mask(await ask(r))));
  }
  const etag = /ETag: (W\/"[^"]*")/i.exec(await ask("GET /app.js HTTP/1.1\n"))[1];
  console.log(JSON.stringify(mask(await ask(`GET /app.js HTTP/1.1\nIf-None-Match: ${etag}\n`))));
  server.close();
}
main();
