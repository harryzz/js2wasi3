// `process` and `performance` (T55): objects made on first use (like `console`, T53), in commands.
// `env`, `argv` and `cwd()` come from `wasi:cli/environment` (`__intrinsic_environment(k)`: 0 the
// variables, 1 the arguments, each string followed by a NUL; 2 the initial directory, "" if none);
// the clocks from `monotonic_now` (ns) and `date_now`. `process` is not an EventEmitter: it has
// its own listeners (`on`, `once`, `emit`, …). `process.exit` emits `'exit'` and ends the program
// through `__intrinsic_process_exit(code)` (no JS runs after it; the exit status is 0 or 1);
// otherwise the event loop calls `$$processAtExit(status)` when the program ends. Warnings are
// emitted as `'warning'` events but not printed, as node does with `--no-warnings`.
let $$processObject = undefined;
let $$processExitCode = undefined;
let $$processExiting = false;
let $$processStart = -1;
let $$processListeners = undefined;
let $$performanceObject = undefined;

function $$processOrigin() {
  if ($$processStart < 0) $$processStart = __intrinsic_monotonic_now();
  return $$processStart;
}

// `process.cwd()`: WASI's initial directory; "/" without one.
function $$processCwd() {
  const cwd = __intrinsic_environment(2);
  return cwd === "" ? "/" : cwd;
}

// node's validation of an exit code: an integer, or a string of one (converted); or undefined/null.
function $$processCode(code) {
  if (code === undefined || code === null) return code;
  if (typeof code === "string" && code !== "" && Number.isInteger(+code)) return +code;
  if (typeof code !== "number") throw $$errInvalidArgType("code", "of type number", code);
  if (!Number.isInteger(code)) throw $$errOutOfRange("code", "an integer", code);
  return code;
}

function $$processEmit(name, args) {
  const list = $$processListeners;
  if (list === undefined) return false;
  const found = [];
  for (let i = 0; i < list.length; i++) {
    if (list[i][0] === name) found.push(list[i]);
  }
  for (let i = 0; i < found.length; i++) {
    const entry = found[i];
    if (entry[2]) {
      const k = list.indexOf(entry);
      if (k >= 0) list.splice(k, 1);
    }
    entry[1].apply($$processObject, args);
  }
  return found.length > 0;
}

// The event loop calls this once when the program ends (`status` 1 after an uncaught exception):
// the `'exit'` listeners run with the exit code, which `process.exitCode` decides.
function $$processAtExit(status) {
  if ($$processObject === undefined || $$processExiting) return undefined;
  $$processExiting = true;
  if (status !== 0) $$processExitCode = 1;
  $$processEmit("exit", [$$processExitCode === undefined || $$processExitCode === null ? 0 : $$processExitCode]);
  const code = $$processExitCode === undefined || $$processExitCode === null ? 0 : $$processExitCode;
  if (code !== 0) __intrinsic_process_exit(code);
  return undefined;
}

function $$processWriter(fd) {
  return {
    fd: fd,
    write: function write(chunk, encoding, callback) {
      if (typeof encoding === "function") callback = encoding;
      if (typeof chunk !== "string") {
        throw $$errInvalidArgType("chunk", "of type string or an instance of Buffer, TypedArray, or DataView", chunk);
      }
      if (fd === 1) __intrinsic_write_stdout(chunk);
      else __intrinsic_write_stderr(chunk);
      if (typeof callback === "function") process.nextTick(callback);
      return true;
    },
  };
}

function $$Process() {
  if ($$processObject !== undefined) return $$processObject;
  $$processOrigin();
  const p = {};
  $$processObject = p;
  $$processListeners = [];
  const vars = __intrinsic_environment(0).split("\0");
  const env = {};
  for (let i = 0; i + 1 < vars.length; i += 2) env[vars[i]] = vars[i + 1];
  const args = __intrinsic_environment(1).split("\0");
  args.pop();
  const argv = [args.length > 0 ? args[0] : "node", __intrinsic_entry_path()];
  for (let i = 1; i < args.length; i++) argv.push(args[i]);
  p.version = "v24.21.0";
  p.versions = {
    node: "24.21.0", acorn: "8.18.0", ada: "4.0.0", amaro: "1.1.11", ares: "1.34.8", brotli: "1.2.0",
    cldr: "48.0", icu: "78.3", llhttp: "9.4.3", merve: "1.2.2", modules: "137", napi: "10", nbytes: "0.1.4",
    ncrypto: "0.0.1", nghttp2: "1.70.0", nghttp3: "", ngtcp2: "", openssl: "3.5.8", simdjson: "4.6.7",
    simdutf: "6.4.0", sqlite: "3.53.4", tz: "2026c", undici: "7.29.1", unicode: "17.0", uv: "1.52.1",
    uvwasi: "0.0.23", v8: "13.6.233.17-node.53", zlib: "1.3.2.1-motley-8002e91", zstd: "1.5.7",
  };
  p.arch = "wasm32";
  p.platform = "linux";
  p.release = {
    name: "node",
    lts: "Krypton",
    sourceUrl: "https://nodejs.org/download/release/v24.21.0/node-v24.21.0.tar.gz",
    headersUrl: "https://nodejs.org/download/release/v24.21.0/node-v24.21.0-headers.tar.gz",
  };
  p.title = "node";
  p.argv = argv;
  p.argv0 = "node";
  p.execArgv = [];
  p.execPath = argv[0];
  p.env = env;
  p.stdout = $$processWriter(1);
  p.stderr = $$processWriter(2);
  Object.defineProperty(p, "exitCode", {
    get: function get() {
      return $$processExitCode;
    },
    set: function set(code) {
      $$processExitCode = $$processCode(code);
    },
    enumerable: true,
    configurable: false,
  });
  p.exit = function exit(code) {
    if (code !== undefined) $$processExitCode = $$processCode(code);
    if (!$$processExiting) {
      $$processExiting = true;
      $$processEmit("exit", [$$processExitCode === undefined || $$processExitCode === null ? 0 : $$processExitCode]);
    }
    __intrinsic_process_exit($$processExitCode === undefined || $$processExitCode === null ? 0 : $$processExitCode);
  };
  p.cwd = function cwd() {
    return $$processCwd();
  };
  p.chdir = function chdir(directory) {
    throw new Error("process.chdir() isn't available in js2wasi3 (WASI has no current directory to change)");
  };
  p.nextTick = process.nextTick;
  const hrtime = function hrtime(time) {
    const t = __intrinsic_monotonic_now();
    let sec = Math.floor(t / 1e9);
    let ns = t - sec * 1e9;
    if (time !== undefined) {
      if (!Array.isArray(time)) throw $$errInvalidArgType("time", "an instance of Array", time);
      if (time.length !== 2) {
        const e = new RangeError('The value of "time" is out of range. It must be 2. Received ' + time.length);
        e.code = "ERR_OUT_OF_RANGE";
        throw e;
      }
      sec -= time[0];
      ns -= time[1];
      if (ns < 0) {
        sec--;
        ns += 1e9;
      }
    }
    return [sec, ns];
  };
  hrtime.bigint = function bigint() {
    if (__intrinsic_bigint_mode()) {
      return __intrinsic_bigint_from_number(__intrinsic_monotonic_now());
    }
    throw new Error("process.hrtime.bigint() needs BigInt: js2wasi3 builds BigInt only into programs that write a BigInt literal or name `BigInt`");
  };
  p.hrtime = hrtime;
  p.uptime = function uptime() {
    return (__intrinsic_monotonic_now() - $$processStart) / 1e9;
  };
  p.emitWarning = function emitWarning(warning, type, code) {
    let w = warning;
    if (typeof w === "string") {
      w = new Error(w);
      w.name = typeof type === "string" ? type : type !== null && typeof type === "object" && typeof type.type === "string" ? type.type : "Warning";
      if (typeof code === "string") w.code = code;
    } else if (!(w instanceof Error)) {
      throw $$errInvalidArgType("warning", "of type string or an instance of Error", w);
    }
    process.nextTick(() => {
      $$processEmit("warning", [w]);
    });
  };
  const on = function on(name, listener) {
    if (typeof listener !== "function") throw $$errInvalidArgType("listener", "of type function", listener);
    $$processListeners.push([name, listener, false]);
    return p;
  };
  p.on = on;
  p.addListener = on;
  p.once = function once(name, listener) {
    if (typeof listener !== "function") throw $$errInvalidArgType("listener", "of type function", listener);
    $$processListeners.push([name, listener, true]);
    return p;
  };
  const off = function off(name, listener) {
    const list = $$processListeners;
    for (let i = list.length - 1; i >= 0; i--) {
      if (list[i][0] === name && list[i][1] === listener) {
        list.splice(i, 1);
        break;
      }
    }
    return p;
  };
  p.off = off;
  p.removeListener = off;
  p.removeAllListeners = function removeAllListeners(name) {
    const list = $$processListeners;
    for (let i = list.length - 1; i >= 0; i--) {
      if (name === undefined || list[i][0] === name) list.splice(i, 1);
    }
    return p;
  };
  p.emit = function emit(name, ...args) {
    return $$processEmit(name, args);
  };
  p.listeners = function listeners(name) {
    const out = [];
    const list = $$processListeners;
    for (let i = 0; i < list.length; i++) {
      if (list[i][0] === name) out.push(list[i][1]);
    }
    return out;
  };
  p.listenerCount = function listenerCount(name) {
    return p.listeners(name).length;
  };
  return p;
}

function $$Performance() {
  if ($$performanceObject !== undefined) return $$performanceObject;
  const origin = $$processOrigin();
  const o = {};
  $$performanceObject = o;
  const wall = __intrinsic_date_now();
  o.timeOrigin = wall - (__intrinsic_monotonic_now() - origin) / 1e6;
  o.now = function now() {
    return (__intrinsic_monotonic_now() - origin) / 1e6;
  };
  return o;
}
