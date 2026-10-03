// node's `child_process` (T65): WASI has no processes, so every spawn fails as node's does when the program
// doesn't exist: `spawn` returns a `ChildProcess` that emits `error` (`spawn <file> ENOENT`) then `close`,
// `execFile` calls back with that error, and the sync forms throw it (or return it, `spawnSync`).
// T70: a shell command (`exec`, `execSync`, `shell: true`) runs as on a system whose `/bin/sh` (dash) finds no
// program: it starts, writes `/bin/sh: 1: <program>: not found` to its stderr and exits with 127 after a turn of
// the event loop, so `exec` calls back with node's `Command failed: …` error (zigbee2mqtt's `git rev-parse`).
// (Shell built-ins such as `echo` are "not found" too.)
"use strict";

const EventEmitter = require("events");
const { Readable, Writable } = require("stream");
const { Buffer } = require("buffer");

const UV_ENOENT = -2;
// The exit status and the stderr line of a shell command whose program isn't found.
const SHELL_NOT_FOUND = 127;

function shellNotFound(shell, command) {
  const program = command.trim().split(/\s+/)[0] || command;
  return `${shell === true ? "/bin/sh" : shell}: 1: ${program}: not found\n`;
}

function argValue(name, reason, value) {
  const e = new TypeError(`The argument '${name}' ${reason}. Received ${$$inspectValue(value)}`);
  e.code = "ERR_INVALID_ARG_VALUE";
  return e;
}

function enoent(syscall, file, args) {
  const e = new Error(`${syscall} ${file} ENOENT`);
  e.errno = UV_ENOENT;
  e.code = "ENOENT";
  e.syscall = `${syscall} ${file}`;
  e.path = file;
  e.spawnargs = args;
  return e;
}

// node's `normalizeSpawnArguments`: the program, its arguments (without it) and the options.
function normalize(file, args, options) {
  if (typeof file !== "string") throw $$errInvalidArgType("file", "of type string", file);
  if (file.length === 0) throw argValue("file", "cannot be empty", file);
  if (Array.isArray(args)) {
    args = args.slice();
  } else if (args == null) {
    args = [];
  } else if (typeof args !== "object") {
    throw $$errInvalidArgType("args", "of type object", args);
  } else {
    options = args;
    args = [];
  }
  if (options === undefined) options = {};
  else if (options === null || typeof options !== "object") throw $$errInvalidArgType("options", "of type object", options);
  let command = null;
  if (options.shell) {
    const line = args.length > 0 ? `${file} ${args.join(" ")}` : file;
    file = typeof options.shell === "string" ? options.shell : "/bin/sh";
    args = ["-c", line];
    // Only `/bin/sh` exists (T70); another shell is a missing program like any other.
    if (file === "/bin/sh" || file === "sh") command = line;
  }
  return { file, args, options, command };
}

function pipes(stdio) {
  const kinds = typeof stdio === "string" ? [stdio, stdio, stdio] : Array.isArray(stdio) ? stdio : [];
  return [0, 1, 2].map((i) => kinds[i] === undefined || kinds[i] === null || kinds[i] === "pipe" || kinds[i] === "overlapped");
}

class ChildProcess extends EventEmitter {
  constructor() {
    super();
    this._closesNeeded = 1;
    this._closesGot = 0;
    this.connected = false;
    this.signalCode = null;
    this.exitCode = null;
    this.killed = false;
    this.spawnfile = null;
    this._handle = null;
  }

  // `options.file` and `options.args` (the program first), as node's `spawn` passes them.
  spawn(options) {
    const file = options.file;
    const args = options.args.slice(1);
    this.spawnfile = file;
    this.spawnargs = [file, ...args];
    this.pid = undefined;
    const [inPipe, outPipe, errPipe] = pipes(options.stdio);
    this.stdin = inPipe
      ? new Writable({ write(chunk, encoding, cb) { cb(Object.assign(new Error("write EPIPE"), { errno: -32, code: "EPIPE", syscall: "write" })); } })
      : null;
    this.stdout = outPipe ? new Readable({ read() {} }) : null;
    this.stderr = errPipe ? new Readable({ read() {} }) : null;
    this.stdio = [this.stdin, this.stdout, this.stderr];
    if (options.command != null) {
      // The shell starts, finds no program and exits.
      this.pid = 1000;
      process.nextTick(() => {
        this.emit("spawn");
        setImmediate(() => {
          if (this.stderr !== null) this.stderr.push(shellNotFound(options.shell, options.command));
          for (const s of [this.stdout, this.stderr]) if (s !== null) s.push(null);
          this.exitCode = SHELL_NOT_FOUND;
          this.emit("exit", this.exitCode, this.signalCode);
          // node's `flushStdio`: pipes nobody reads are resumed, so they end.
          for (const s of [this.stdout, this.stderr]) if (s !== null && !s.readableFlowing) s.resume();
          setImmediate(() => {
            this.emit("close", this.exitCode, this.signalCode);
            for (const s of this.stdio) if (s !== null) s.destroy();
          });
        });
      });
      return 0;
    }
    const err = enoent("spawn", file, args);
    process.nextTick(() => {
      this.exitCode = UV_ENOENT;
      this.emit("error", err);
      setImmediate(() => {
        for (const s of [this.stdout, this.stderr]) if (s !== null) s.push(null);
        this.emit("close", this.exitCode, this.signalCode);
        for (const s of this.stdio) if (s !== null) s.destroy();
      });
    });
    return err;
  }

  kill() {
    return false;
  }

  ref() {}

  unref() {}
}

function spawn(file, args, options) {
  const n = normalize(file, args, options);
  const child = new ChildProcess();
  child.spawn({ ...n.options, file: n.file, args: [n.file, ...n.args], command: n.command });
  return child;
}

function execFile(file, args, options, callback) {
  if (typeof args === "function") {
    callback = args;
    args = [];
    options = {};
  } else if (typeof options === "function") {
    callback = options;
    if (!Array.isArray(args) && args != null) {
      options = args;
      args = [];
    } else {
      options = {};
    }
  } else if (!Array.isArray(args) && args != null && typeof args === "object") {
    callback = options;
    options = args;
    args = [];
  }
  options = { ...options };
  const child = spawn(file, args, options);
  const encoding = options.encoding === undefined ? "utf8" : options.encoding;
  const list = Array.isArray(args) ? args : [];
  const cmd = list.length > 0 ? `${file} ${list.join(" ")}` : file;
  const text = (s) => (encoding === "buffer" || !Buffer.isEncoding(encoding) ? Buffer.from(s) : s);
  child.on("error", (e) => {
    e.cmd = cmd;
    if (typeof callback === "function") callback(e, text(""), text(""));
  });
  if (child.pid !== undefined) {
    // A shell that ran (T70). node's `exithandler`: the shell's stderr, then a `Command failed` error on `close`.
    let stderr = "";
    child.stderr.on("data", (d) => { stderr += d; });
    child.on("close", (code, signal) => {
      if (typeof callback !== "function") return;
      const e = new Error(`Command failed: ${cmd}\n${stderr}`);
      e.code = code;
      e.killed = false;
      e.signal = signal;
      e.cmd = cmd;
      callback(e, text(""), text(stderr));
    });
  }
  return child;
}

function exec(command, options, callback) {
  if (typeof options === "function") {
    callback = options;
    options = undefined;
  }
  options = { ...options };
  options.shell = typeof options.shell === "string" ? options.shell : true;
  return execFile(command, options, callback);
}

function syncResult(file, args, options) {
  const n = normalize(file, args, options);
  if (n.command != null) {
    // T70: the shell runs, finds no program and exits with 127 (no `error`).
    const stderr = Buffer.from(shellNotFound(n.options.shell, n.command));
    const stdout = Buffer.alloc(0);
    const enc = n.options.encoding;
    const out = (b) => (enc && enc !== "buffer" && Buffer.isEncoding(enc) ? b.toString(enc) : b);
    return { status: SHELL_NOT_FOUND, signal: null, output: [null, out(stdout), out(stderr)], pid: 1000, stdout: out(stdout), stderr: out(stderr), command: n.command };
  }
  return {
    error: enoent("spawnSync", n.file, n.args),
    status: null,
    signal: null,
    output: null,
    pid: 0,
    stdout: undefined,
    stderr: undefined,
  };
}

function spawnSync(file, args, options) {
  const r = syncResult(file, args, options);
  delete r.command;
  return r;
}

function syncError(ret) {
  if (ret.error === undefined) {
    // A shell command that exited with 127: node's `checkExecSyncError`.
    const { command, ...rest } = ret;
    const err = new Error(`Command failed: ${command}\n${String(ret.stderr)}`);
    Object.assign(err, rest);
    return err;
  }
  const err = ret.error;
  Object.assign(err, ret);
  return err;
}

function execFileSync(file, args, options) {
  throw syncError(syncResult(file, args, options));
}

function execSync(command, options) {
  const ret = syncResult(command, [], { ...options, shell: options && typeof options.shell === "string" ? options.shell : true });
  // node: without `stdio`, the child's stderr goes to the parent's too.
  if (ret.error === undefined && (!options || options.stdio === undefined)) process.stderr.write(ret.stderr);
  throw syncError(ret);
}

function fork(modulePath, args, options) {
  if (typeof modulePath !== "string" && !(modulePath instanceof URL)) {
    throw $$errInvalidArgType("modulePath", "of type string or an instance of URL", modulePath);
  }
  if (!Array.isArray(args)) {
    options = args;
    args = [];
  }
  return spawn(process.execPath, [...process.execArgv, String(modulePath), ...args], options);
}

function _forkChild() {
  throw new Error("child_process._forkChild() isn't available in js2wasi3 (WASI has no processes)");
}

module.exports = {
  _forkChild,
  ChildProcess,
  exec,
  execFile,
  execFileSync,
  execSync,
  fork,
  spawn,
  spawnSync,
};
