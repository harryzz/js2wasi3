// The `process` global and module (T55): arguments and environment (x.args, x.env), constants,
// clocks, stdout/stderr writes, listeners, warnings, exit codes and the 'exit' event.
import proc, { argv, env, platform } from "node:process";
import proc2 from "process";

console.log(proc === process, proc2 === process, argv === process.argv, env === process.env, platform);
console.log(process.argv.slice(2), process.argv.length, process.argv[1].endsWith("/node/process.js"));
console.log(process.env.GREETING, process.env.EMPTY === "", process.env.MISSING, "GREETING" in process.env);
console.log(process.platform, process.version, process.versions.node, process.release.name, process.title, process.argv0);
console.log(typeof process.cwd(), typeof process.execPath, Array.isArray(process.execArgv), process.exitCode);

// Clocks: shapes only (the values differ run to run).
const t = process.hrtime();
const d = process.hrtime(t);
console.log(t.length, Number.isInteger(t[0]), d[0] >= 0 && d[1] >= 0 && d[1] < 1e9, process.uptime() >= 0);
const a = performance.now();
const b = performance.now();
console.log(b >= a, a >= 0, performance.timeOrigin > 1.7e12);

// Writes without a newline.
process.stdout.write("a");
process.stdout.write("b\n", () => console.log("write callback"));
process.stderr.write("to stderr\n");

// Listeners.
function onFoo(x) {
  console.log("foo", x, this === process);
}
process.on("foo", onFoo);
process.once("foo", (x) => console.log("foo once", x));
console.log(process.emit("foo", 1), process.listenerCount("foo"));
console.log(process.emit("foo", 2), process.listenerCount("foo"), process.listeners("foo")[0] === onFoo);
process.off("foo", onFoo);
console.log(process.emit("foo", 3), process.addListener === process.on, process.removeListener === process.off);

// Warnings: a 'warning' event on a later tick (node with --no-warnings prints nothing).
process.on("warning", (w) => console.log("warning:", w.name, w.message, w.code));
process.emitWarning("careful", "CustomWarning", "W001");
process.emitWarning("plain");
console.log("after emitWarning");

// Exit codes: validated like node's.
for (const code of ["x", 1.5, {}, "2"]) {
  try {
    process.exitCode = code;
    console.log("exitCode", process.exitCode);
  } catch (e) {
    console.log(e.name, e.code, e.message);
  }
}
process.exitCode = 0;
process.on("exit", (code) => console.log("exit", code, process.exitCode));
setTimeout(() => console.log("timer"), 1);
