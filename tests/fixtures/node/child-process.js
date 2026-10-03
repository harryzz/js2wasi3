// `child_process` (T65): a program that doesn't exist fails with ENOENT through every API. A command runs
// through a shell, so `shell` names a missing one (node would run /bin/sh; js2wasi3 has no processes).
import cp from "node:child_process";
import EventEmitter from "node:events";

console.log(Object.keys(cp).sort().join(" "));
const show = (e) => [e.name, e.message, e.errno, e.code, e.syscall, e.path, JSON.stringify(e.spawnargs)].join(" | ");

const child = cp.spawn("/no/such/program", ["a", "b"]);
console.log(child instanceof cp.ChildProcess, child instanceof EventEmitter, child.pid, child.exitCode, child.killed,
  child.spawnfile, child.spawnargs, typeof child.stdout, child.stdio.length, child.kill());
child.on("error", (e) => console.log("spawn error", show(e), child.exitCode));
child.on("exit", () => console.log("exit (never)"));
child.on("close", (code, signal) => {
  console.log("spawn close", code, signal);
  cp.execFile("/no/such/tool", ["--version"], (e, stdout, stderr) => {
    console.log("execFile", show(e), e.cmd, JSON.stringify(stdout), JSON.stringify(stderr));
    cp.exec("git rev-parse --short HEAD", { shell: "/no/sh" }, (e2, stdout2) => {
      console.log("exec", show(e2), e2.cmd, JSON.stringify(stdout2));
    });
  });
});
const ignored = cp.spawn("/no/such/x", { stdio: "ignore" });
console.log(ignored.stdout, ignored.stdin);
ignored.on("error", (e) => console.log("ignored", e.code));

const r = cp.spawnSync("/no/such/program", ["q"]);
console.log(Object.keys(r).join(), show(r.error), r.status, r.signal, r.output, r.pid, r.stdout);
for (const f of [() => cp.execFileSync("/no/such/program"), () => cp.execSync("echo hi", { shell: "/no/sh" })]) {
  try { f(); } catch (e) { console.log("sync", show(e), e.status, e.pid, e.error === e); }
}
for (const f of [() => cp.spawn(""), () => cp.spawn(5), () => cp.spawn("x", "y")]) {
  try { f(); } catch (e) { console.log(e.name, e.code, e.message); }
}
