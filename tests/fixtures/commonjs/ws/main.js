// ws 8.22.0 (MIT, vendored in node_modules/ws: zigbee2mqtt's WebSocket server, T71) on node:http's server:
// `noServer` + `handleUpgrade` as zigbee2mqtt's frontend does it, and a raw node:net client that does the
// handshake and sends masked frames. Each step waits for the last.
const http = require("http");
const net = require("net");
const WebSocket = require("ws");

const once = (emitter, name) => new Promise((resolve) => emitter.once(name, (...args) => resolve(args)));

// A client frame (masked, as clients must): text 0x1, close 0x8.
function frame(opcode, payload) {
  const mask = Buffer.from([1, 2, 3, 4]);
  const head = payload.length < 126 ? Buffer.from([0x80 | opcode, 0x80 | payload.length])
    : Buffer.from([0x80 | opcode, 0x80 | 126, payload.length >> 8, payload.length & 255]);
  const body = Buffer.from(payload);
  for (let i = 0; i < body.length; i++) body[i] ^= mask[i & 3];
  return Buffer.concat([head, mask, body]);
}

// Server frames (unmasked) from a buffer: [opcode, payload] pairs.
function frames(buf) {
  const out = [];
  let at = 0;
  while (at + 2 <= buf.length) {
    let len = buf[at + 1] & 127;
    let start = at + 2;
    if (len === 126) {
      len = buf.readUInt16BE(at + 2);
      start += 2;
    }
    if (start + len > buf.length) break;
    out.push([buf[at] & 15, buf.subarray(start, start + len)]);
    at = start + len;
  }
  return [out, buf.subarray(at)];
}

async function main() {
  const log = [];
  const wss = new WebSocket.WebSocketServer({ noServer: true });
  wss.on("connection", (ws, req) => {
    log.push(`connection ${req.url} ${ws.protocol === "" ? "(no protocol)" : ws.protocol} ${ws.readyState}`);
    ws.on("message", (data, isBinary) => {
      log.push(`message ${JSON.stringify(data.toString())} ${isBinary}`);
      if (data.toString() === "bye") ws.close(4401, "Unauthorized");
      else ws.send(JSON.stringify({ echo: data.toString(), n: data.length }));
    });
    ws.on("close", (code, reason) => log.push(`close ${code} ${JSON.stringify(reason.toString())}`));
  });
  const server = http.createServer((req, res) => res.end("plain"));
  server.on("upgrade", (req, socket, head) => {
    log.push(`upgrade ${req.url} ${req.headers["sec-websocket-version"]} head=${head.length}`);
    if (req.url !== "/api") {
      socket.write("HTTP/1.1 404 Not Found\r\n\r\n");
      socket.destroy();
      return;
    }
    wss.handleUpgrade(req, socket, head, (ws) => wss.emit("connection", ws, req));
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const port = server.address().port;

  const c = net.connect(port, "127.0.0.1");
  await once(c, "connect");
  const closed = once(c, "close");
  let pending = Buffer.alloc(0);
  const got = [];
  let wake = null;
  c.on("data", (d) => {
    pending = Buffer.concat([pending, d]);
    if (got.length === 0) {
      const end = pending.indexOf("\r\n\r\n");
      if (end < 0) return;
      got.push(pending.subarray(0, end).toString());
      pending = pending.subarray(end + 4);
    }
    const [fs, rest] = frames(pending);
    pending = rest;
    for (const [op, p] of fs) got.push(op === 8 ? `close ${p.readUInt16BE(0)} ${p.subarray(2)}` : `${op} ${p}`);
    if (wake) wake();
  });
  const next = async (n) => {
    while (got.length < n) await new Promise((r) => (wake = r));
    return got[n - 1];
  };
  c.write("GET /api HTTP/1.1\r\nHost: test\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n" +
    "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n");
  console.log(JSON.stringify((await next(1)).replace(/\r\n/g, " | ")));
  c.write(frame(1, "hello"));
  console.log(await next(2));
  c.write(frame(1, "x".repeat(300)));
  console.log((await next(3)).slice(0, 40));
  c.write(frame(1, "bye"));
  console.log(await next(4));
  c.write(frame(8, Buffer.from([0x11, 0x31])));
  c.end();
  await closed;
  await new Promise((r) => setTimeout(r, 50));
  console.log(log.join("\n"));
  console.log("clients", wss.clients.size);

  // A wrong path, and a plain request.
  const d = net.connect(port, "127.0.0.1");
  await once(d, "connect");
  let out = "";
  d.on("data", (x) => (out += x));
  const dClosed = once(d, "close");
  d.write("GET /other HTTP/1.1\r\nHost: test\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\n");
  await dClosed;
  console.log(JSON.stringify(out));
  wss.close();
  server.close();
  await once(server, "close");
  console.log("done");
}
main();
