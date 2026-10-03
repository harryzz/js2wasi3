// node:http's server (T71): node 24's _http_server over node:net and a JS llhttp. The clients are raw
// node:net sockets; each step waits for the last. `Date:` headers are replaced (time-dependent).
import http from "node:http";
import net from "node:net";

const once = (emitter, name) => new Promise((resolve) => emitter.once(name, (...args) => resolve(args)));
const show = (s) => JSON.stringify(s.replace(/Date: [^\r]*\r\n/g, "Date: <date>\r\n"));

const log = [];
const server = http.createServer((req, res) => {
  const body = [];
  req.on("data", (d) => body.push(d));
  req.on("end", () => {
    log.push(`${req.method} ${req.url} ${req.httpVersion} ${JSON.stringify(req.rawHeaders)} body=${JSON.stringify(Buffer.concat(body).toString())} complete=${req.complete}`);
    if (req.url === "/missing") {
      res.statusCode = 404;
      res.end("not here");
    } else if (req.url === "/chunked") {
      res.setHeader("Content-Type", "text/plain");
      res.write("one,");
      res.write("two,");
      res.end("three");
    } else if (req.url === "/head") {
      res.writeHead(201, "Made", { "X-List": ["a", "b"], "Content-Length": 4 });
      res.end(req.method === "HEAD" ? undefined : "body");
    } else if (req.url === "/trailers") {
      res.writeHead(200, { "Transfer-Encoding": "chunked", Trailer: "X-Sum" });
      res.write("data");
      res.addTrailers({ "X-Sum": "42" });
      res.end();
    } else if (req.url === "/empty") {
      res.writeHead(204);
      res.end();
    } else {
      res.setHeader("X-Url", req.url);
      res.end(`hello ${req.method} ${req.url}`);
    }
  });
});
console.log(server instanceof net.Server, server.listening, server.timeout, server.keepAliveTimeout, server.headersTimeout,
  server.requestTimeout, server.maxHeadersCount, server.maxRequestsPerSocket);
server.listen(0, "127.0.0.1");
await once(server, "listening");
const port = server.address().port;

// Sends `text` on a fresh connection, reads to the end.
async function exchange(text, { half = false } = {}) {
  const c = net.connect(port, "127.0.0.1");
  await once(c, "connect");
  const closed = once(c, "close");
  let out = "";
  c.on("data", (d) => (out += d));
  if (Array.isArray(text)) {
    for (const t of text) {
      c.write(t);
      await new Promise((r) => setTimeout(r, 20));
    }
  } else {
    c.write(text);
  }
  if (half) c.end();
  await closed;
  return out;
}

const H = "Host: test\r\n";
console.log(show(await exchange(`GET /a?b=1 HTTP/1.1\r\n${H}Connection: close\r\n\r\n`)));
console.log(show(await exchange(`POST /post HTTP/1.1\r\n${H}Content-Length: 11\r\nConnection: close\r\n\r\nhello world`)));
console.log(show(await exchange(`POST /chunked HTTP/1.1\r\n${H}Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n`)));
// Keep-alive: three pipelined requests, the last closes.
console.log(show(await exchange(`GET /one HTTP/1.1\r\n${H}\r\nGET /missing HTTP/1.1\r\n${H}\r\nHEAD /head HTTP/1.1\r\n${H}Connection: close\r\n\r\n`)));
// In pieces, and HTTP/1.0.
console.log(show(await exchange(["GET /pie", `ces HTTP/1.1\r\n${H}Conn`, "ection: close\r\n\r\n"])));
console.log(show(await exchange("GET /old HTTP/1.0\r\n\r\n")));
console.log(show(await exchange(`GET /head HTTP/1.1\r\n${H}Connection: close\r\n\r\n`)));
console.log(show(await exchange(`GET /trailers HTTP/1.1\r\n${H}Connection: close\r\n\r\n`)));
console.log(show(await exchange(`GET /empty HTTP/1.1\r\n${H}Connection: close\r\n\r\n`)));
// No Host header; a malformed request; a client that half-closes after a keep-alive request.
console.log(show(await exchange("GET /nohost HTTP/1.1\r\n\r\n")));
console.log(show(await exchange("BAD REQUEST\r\n\r\n")));
console.log(show(await exchange(`GET /half HTTP/1.1\r\n${H}\r\n`, { half: true })));
// Expect: 100-continue.
console.log(show(await exchange(`PUT /put HTTP/1.1\r\n${H}Expect: 100-continue\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc`)));
console.log(log.join("\n"));

// clientError: the server answers it itself.
server.once("clientError", (err, socket) => {
  console.log("clientError", err.code, err.reason, err.message, socket.writable);
  socket.end("HTTP/1.1 400 Custom\r\n\r\n");
});
console.log(show(await exchange("GET / HTTP/1.1\r\nBad Header: x\r\n\r\n")));

// An upgrade: the socket and the bytes after the headers go to the listener.
server.on("upgrade", (req, socket, head) => {
  console.log("upgrade", req.method, req.url, req.headers.upgrade, JSON.stringify(head.toString()));
  socket.write("HTTP/1.1 101 Switching Protocols\r\nUpgrade: echo\r\nConnection: Upgrade\r\n\r\n");
  socket.on("data", (d) => socket.write(d.toString().toUpperCase()));
  socket.on("end", () => socket.end());
});
console.log(show(await exchange([`GET /ws HTTP/1.1\r\n${H}Upgrade: echo\r\nConnection: Upgrade\r\n\r\nfirst`, " second"],
  { half: true })));
// CONNECT without a listener: the socket is destroyed.
console.log(show(await exchange(`CONNECT a:1 HTTP/1.1\r\n${H}\r\n`)));

// Errors of the response API.
{
  const c = net.connect(port, "127.0.0.1");
  await once(c, "connect");
  const closed = once(c, "close");
  c.resume();
  const [req, res] = await new Promise((resolve) => {
    server.removeAllListeners("request");
    server.once("request", (...a) => resolve(a));
    c.write(`GET /api HTTP/1.1\r\n${H}Connection: close\r\n\r\n`);
  });
  for (const f of [() => res.writeHead(99), () => res.setHeader("bad name", "x"), () => res.setHeader("x", "a\nb"),
    () => res.setHeader("x", undefined)]) {
    try {
      f();
    } catch (e) {
      console.log(e.name, e.code, e.message);
    }
  }
  console.log(req.headers.host, res.headersSent, res.getHeaderNames(), res.statusCode, res.writableEnded);
  res.end("ok");
  try {
    res.setHeader("x", "y");
  } catch (e) {
    console.log(e.name, e.code, e.message);
  }
  console.log(res.headersSent, res.writableEnded, res.finished);
  await closed;
}

server.close();
await once(server, "close");
console.log("closed", server.listening);
