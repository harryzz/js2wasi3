// node:net (T66): TCP over wasi:sockets. A server and its clients in one program; each step waits
// for the last, so the output doesn't depend on the order the host delivers events in.
import net from "node:net";
import { createServer, isIP } from "node:net";

const once = (emitter, name) => new Promise((resolve) => emitter.once(name, (...args) => resolve(args)));

console.log(Object.keys(net).sort().join(" "));
console.log(isIP("127.0.0.1"), isIP("::1"), isIP("fe80::1%eth0"), isIP("1.2.3"), net.isIPv4("10.0.0.1"), net.isIPv6("::ffff:1.2.3.4"));

const s0 = new net.Socket();
console.log(s0.readyState, s0.pending, s0.connecting, s0.bytesRead, s0.bytesWritten, s0.allowHalfOpen, s0.timeout,
  s0.remoteAddress, s0 instanceof net.Stream);
for (const f of [() => net.connect(70000), () => net.connect({}), () => net.connect({ port: true }), () => net.connect(-1)]) {
  try {
    f();
  } catch (e) {
    console.log(e.name, e.code, e.message);
  }
}

// A port nothing listens on: listen, then close.
const probe = net.createServer();
await once(probe.listen(0, "127.0.0.1"), "listening");
const freePort = probe.address().port;
probe.close();
await once(probe, "close");
console.log("closed listening", probe.listening, probe.address());

// A refused connection.
{
  const s = net.connect(freePort, "127.0.0.1");
  const events = [];
  s.on("connectionAttempt", (address, port, family) => events.push(`attempt ${address} ${port === freePort} ${family}`));
  s.on("connectionAttemptFailed", (address, port, family, e) => events.push(`attemptFailed ${e.code}`));
  console.log("state", s.readyState, s.connecting, s.pending);
  const [e] = await once(s, "error");
  const [hadError] = await once(s, "close");
  console.log(events.join(", "));
  console.log(e.message.replace(String(freePort), "PORT"), e.code, e.errno, e.syscall, e.address, e.port === freePort,
    Object.keys(e).join(" "));
  console.log("close", hadError, s.readyState, s.destroyed);
}

// An echo server: one client sends, half-closes, reads back.
{
  const log = [];
  let serverData = "";
  let serverClosed;
  const serverClose = new Promise((resolve) => (serverClosed = resolve));
  const server = createServer((c) => {
    log.push(`connection ${c.remoteAddress} ${c.remoteFamily} ${c.readyState} ${c.localPort === server.address().port}`);
    c.on("data", (d) => (serverData += d));
    c.on("end", () => log.push(`server end ${serverData}`));
    c.on("close", (h) => serverClosed(log.push(`server socket close ${h}`)));
    c.pipe(c);
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const a = server.address();
  console.log("listening", server.listening, Object.keys(a).join(" "), a.address, a.family, typeof a.port);
  const s = net.createConnection({ port: a.port, host: "127.0.0.1" });
  s.setNoDelay(true);
  s.setKeepAlive(true, 15000);
  s.write("hello ");
  await once(s, "connect");
  console.log("connect", s.readyState, s.remoteAddress, s.remotePort === a.port, s.remoteFamily, s.localAddress);
  s.end("world");
  let got = "";
  s.setEncoding("utf8");
  s.on("data", (d) => (got += d));
  await once(s, "end");
  console.log("end", JSON.stringify(got), s.readyState);
  const [hadError] = await once(s, "close");
  console.log("close", hadError, s.bytesRead, s.bytesWritten);
  await serverClose;
  await new Promise((resolve) => server.getConnections((e, n) => resolve(console.log("connections", e, n))));
  await new Promise((resolve) => server.close(resolve));
  console.log(log.join("\n"));
}

// destroy() while the peer keeps the connection open; a second listen on a port in use; timeouts.
{
  const server = net.createServer((c) => c.write("hi"));
  await once(server.listen(0, "127.0.0.1"), "listening");
  const port = server.address().port;
  const other = net.createServer();
  other.listen(port, "127.0.0.1");
  const [e] = await once(other, "error");
  console.log(e.message.replace(String(port), "PORT"), e.code, e.syscall, other.listening);
  const s = net.connect(port, "127.0.0.1");
  const [d] = await once(s, "data");
  console.log("data", String(d));
  s.setTimeout(20);
  await once(s, "timeout");
  console.log("timeout", s.timeout);
  s.destroy();
  console.log("destroyed", s.destroyed, s.readyState);
  console.log("close", ...(await once(s, "close")));
  server.close((err) => console.log("server closed", err));
  server.close((err) => console.log("again", err.code, err.message));
}
