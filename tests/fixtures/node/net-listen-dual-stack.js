// T148: `listen(port)` without a host takes IPv4 and IPv6 clients (node's `::` is dual-stack; a WASI IPv6 socket is not).
import net from "node:net";
const connect = (host, port) => new Promise((resolve) => {
  const c = net.connect(port, host, () => c.write("hi"));
  c.on("data", (d) => { resolve(host + " " + d); c.end(); });
  c.on("error", (e) => resolve(host + " error " + e.code));
});
const server = net.createServer((s) => s.on("data", (d) => s.write("echo " + d)));
server.listen(0, async () => {
  const { port } = server.address();
  console.log(await connect("127.0.0.1", port));
  console.log(await connect("::1", port));
  server.close(() => console.log("closed"));
});
