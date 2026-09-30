// node's `os` (T55): what doesn't depend on the machine (x.env sets TMPDIR and HOME), and the
// shapes of the rest; `tty.isatty` is false under the test harness's pipes, as in node.
import os, { EOL, tmpdir, homedir } from "node:os";
import tty from "tty";

console.log(os.platform(), os.type(), os.endianness(), JSON.stringify(EOL), os.devNull);
console.log(os.constants.signals.SIGTERM, os.constants.signals.SIGKILL, os.constants.errno.ENOENT, os.constants.priority.PRIORITY_LOW);
console.log(tmpdir(), homedir(), typeof os.hostname(), typeof os.release(), typeof os.version(), typeof os.machine());
console.log(Array.isArray(os.cpus()), os.cpus().length > 0, typeof os.cpus()[0].model, os.availableParallelism() >= 1);
console.log(typeof os.totalmem(), os.freemem() <= os.totalmem(), os.loadavg().length, typeof os.uptime(), typeof os.networkInterfaces());
console.log(typeof os.userInfo().username, typeof os.userInfo().homedir, Object.keys(os).sort().join(","));
console.log(tty.isatty(1), tty.isatty(2), typeof tty.WriteStream);
