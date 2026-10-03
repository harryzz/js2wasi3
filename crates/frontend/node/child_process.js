// node's `child_process` (T65): WASI has no processes, so every spawn fails as node's does when the program
// doesn't exist: `spawn` returns a `ChildProcess` that emits `error` (`spawn <file> ENOENT`) then `close`,
// `exec`/`execFile` call back with that error, and the sync forms throw it (or return it, `spawnSync`).
// A shell command runs `/bin/sh -c` (or `options.shell`), which is just as absent.
"use strict";

const EventEmitter = require("events");
const { Readable, Writable } = require("stream");
const { Buffer } = require("buffer");

const UV_ENOENT = -2;

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
  if (options.shell) {
    const command = args.length > 0 ? `${file} ${args.join(" ")}` : file;
    file = typeof options.shell === "string" ? options.shell : "/bin/sh";
    args = ["-c", command];
  }
  return { file, args, options };
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
  child.spawn({ ...n.options, file: n.file, args: [n.file, ...n.args] });
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
  child.on("error", (e) => {
    const list = Array.isArray(args) ? args : [];
    e.cmd = list.length > 0 ? `${file} ${list.join(" ")}` : file;
    if (typeof callback === "function") {
      const empty = encoding === "buffer" || !Buffer.isEncoding(encoding) ? Buffer.alloc(0) : "";
      callback(e, empty, empty);
    }
  });
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
  return syncResult(file, args, options);
}

function syncError(ret) {
  const err = ret.error;
  Object.assign(err, ret);
  return err;
}

function execFileSync(file, args, options) {
  throw syncError(syncResult(file, args, options));
}

function execSync(command, options) {
  throw syncError(syncResult(command, [], { ...options, shell: options && typeof options.shell === "string" ? options.shell : true }));
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
