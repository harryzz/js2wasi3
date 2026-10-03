// T70: shell commands run as on a system whose /bin/sh finds no program: the shell starts, writes
// `/bin/sh: 1: <program>: not found` and exits with 127 after a turn of the event loop (node with dash as
// /bin/sh and no such program). Programs run directly stay missing (`spawn <file> ENOENT`, child-process.js).
import cp from "node:child_process";
import net from "node:net";
import { once } from "node:events";

const P = "nosuchprog-t70";

// exec: node's `Command failed` error, after a setImmediate scheduled before it.
await new Promise((done) => {
  setImmediate(() => console.log("immediate"));
  cp.exec(`${P} rev-parse --short=8 HEAD`, (e, stdout, stderr) => {
    console.log("exec", JSON.stringify([e.message, e.code, e.killed, e.signal, e.cmd, stdout, stderr]));
    done();
  });
});
await new Promise((done) => cp.exec(P, { encoding: "buffer" }, (e, stdout, stderr) => {
  console.log("exec buffer", Buffer.isBuffer(stdout), Buffer.isBuffer(stderr), String(stderr).trim());
  done();
}));

// spawn with `shell: true`: spawn, stderr, exit, close.
const c = cp.spawn(`${P} --version`, { shell: true });
console.log("spawn", typeof c.pid, c.spawnfile, JSON.stringify(c.spawnargs));
const events = [];
c.on("spawn", () => events.push("spawn"));
c.stderr.on("data", (d) => events.push(`stderr ${JSON.stringify(String(d))}`));
c.on("exit", (code, signal) => events.push(`exit ${code} ${signal}`));
const [code, signal] = await once(c, "close");
console.log(events, "close", code, signal);

// The sync forms.
try {
  cp.execSync(`${P} status`, { stdio: "pipe" });
} catch (e) {
  console.log("execSync", JSON.stringify([e.message, e.status, e.signal, String(e.stdout), String(e.stderr), e.output.length]));
}
const r = cp.spawnSync(`${P} status`, { shell: true, encoding: "utf8" });
console.log("spawnSync", JSON.stringify([r.status, r.signal, r.stdout, r.stderr, r.error]));
try {
  cp.execSync(`${P} quiet`, { stdio: ["pipe", "pipe", "pipe"] });
} catch (e) {
  console.log("execSync piped", e.status);
}

// A socket destroyed before an exec closes before the exec's callback (zigbee2mqtt's restart, F300).
const server = net.createServer((s) => s.on("error", () => {}));
server.listen(0, "127.0.0.1");
await once(server, "listening");
const sock = net.connect(server.address().port, "127.0.0.1");
await once(sock, "connect");
sock.once("close", () => console.log("socket close"));
sock.destroy();
await new Promise((done) => cp.exec(`${P} rev-parse HEAD`, () => { console.log("exec callback"); done(); }));
server.close();
