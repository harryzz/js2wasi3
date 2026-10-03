// T67 (F282): `dgram` over `wasi:sockets` UDP: two sockets in one program, each step awaited.
import dgram from "node:dgram";
import { EventEmitter } from "node:events";
import { once } from "node:events";

// Ports are the host's choice: masked in messages.
const show = (e) => console.log(e.constructor.name, e.code, e.message.replace(/:\d+$/, ":PORT"), e.errno, e.syscall);

async function main() {
  try { dgram.createSocket("udp5"); } catch (e) { show(e); }
  try { dgram.createSocket({}); } catch (e) { show(e); }

  const a = dgram.createSocket("udp4");
  const b = dgram.createSocket({ type: "udp4" });
  console.log(a instanceof dgram.Socket, a instanceof EventEmitter);
  try { a.address(); } catch (e) { show(e); }
  try { a.remoteAddress(); } catch (e) { show(e); }

  // bind: 'listening', then address().
  const events = [];
  b.on("listening", () => events.push("listening"));
  b.bind(0, "127.0.0.1", () => events.push("bind callback"));
  await once(b, "listening");
  await new Promise((r) => setImmediate(r));
  console.log(events);
  const bAddr = b.address();
  console.log(bAddr.address, bAddr.family, typeof bAddr.port, bAddr.port > 0);

  // send from an unbound socket: it binds itself first.
  a.on("listening", () => console.log("a listening", a.address().address, a.address().family));
  const got = once(b, "message");
  a.send("hello", bAddr.port, "127.0.0.1", (err, bytes) => console.log("sent", err, bytes));
  const [msg, rinfo] = await got;
  console.log(Buffer.isBuffer(msg), msg.toString(), rinfo.address, rinfo.family, rinfo.port === a.address().port, rinfo.size);

  // Buffer, offset/length, an array of chunks.
  const many = [];
  const three = new Promise((r) => b.on("message", (m) => { many.push(m.toString()); if (many.length === 3) r(); }));
  a.send(Buffer.from("buffer"), bAddr.port, "127.0.0.1");
  a.send(Buffer.from("xxsliceyy"), 2, 5, bAddr.port, "127.0.0.1");
  a.send(["ar", Buffer.from("ray")], bAddr.port, "127.0.0.1");
  await three;
  b.removeAllListeners("message");
  console.log(many.sort());

  // An empty datagram.
  const empty = once(b, "message");
  a.send(Buffer.alloc(0), bAddr.port, "127.0.0.1");
  const [m0, r0] = await empty;
  console.log(m0.length, r0.size);

  // Reply to the sender's rinfo.
  const reply = once(a, "message");
  b.once("message", (m, r) => b.send(m.toString().toUpperCase(), r.port, r.address));
  a.send("ping", bAddr.port, "127.0.0.1");
  console.log((await reply)[0].toString());

  // connect: send without an address; remoteAddress().
  const c = dgram.createSocket("udp4");
  c.connect(bAddr.port, "127.0.0.1", () => console.log("connect callback"));
  await once(c, "connect");
  const ra = c.remoteAddress();
  console.log(ra.address, ra.family, ra.port === bAddr.port);
  try { c.connect(bAddr.port, "127.0.0.1"); } catch (e) { show(e); }
  const fromC = once(b, "message");
  c.send("connected", (err) => console.log("connected send", err));
  console.log((await fromC)[0].toString());
  c.disconnect();
  try { c.remoteAddress(); } catch (e) { show(e); }
  try { c.disconnect(); } catch (e) { show(e); }

  // Bad arguments.
  try { a.send("x"); } catch (e) { show(e); }
  try { a.send("x", 70000, "127.0.0.1"); } catch (e) { show(e); }
  try { a.send(42, bAddr.port, "127.0.0.1"); } catch (e) { show(e); }
  try { a.bind(0); } catch (e) { show(e); }

  // EADDRINUSE as an 'error' event.
  const d = dgram.createSocket("udp4");
  d.bind(bAddr.port, "127.0.0.1");
  const [err] = await once(d, "error");
  show(err);
  console.log(err.address, err.port === bAddr.port);
  d.close();

  // A send that is too large: its callback gets the error.
  await new Promise((r) => a.send(Buffer.alloc(70000), bAddr.port, "127.0.0.1", (e) => { show(e); r(); }));

  // close: 'close', its callback, then sends throw.
  const order = [];
  c.on("close", () => order.push("close event"));
  c.close(() => order.push("close callback"));
  await once(c, "close");
  console.log(order);
  try { c.send("x", bAddr.port, "127.0.0.1"); } catch (e) { show(e); }
  try { c.close(); } catch (e) { show(e); }
  try { c.address(); } catch (e) { show(e); }

  // udp6 on ::1.
  const s6 = dgram.createSocket("udp6");
  s6.bind(0, "::1");
  await once(s6, "listening");
  const r6 = dgram.createSocket("udp6");
  const got6 = once(s6, "message");
  r6.send("six", s6.address().port, "::1");
  const [m6, ri6] = await got6;
  console.log(m6.toString(), ri6.address, ri6.family, s6.address().address, s6.address().family);
  s6.close();
  r6.close();

  // Closing with a receive pending ends the program.
  a.close();
  b.close();
  console.log("done");
}
main();
