// node's `os` (T55): what a WASI program can know. Values node reads from the system are
// placeholders for a Linux-like host (`platform()` is "linux", as `process.platform`, so packages
// take their POSIX paths): one CPU, no network interfaces, the memory sizes of a small machine.
// `tmpdir()` and `homedir()` follow node's rules for `TMPDIR`/`HOME` from the environment.
// `constants` are node 24's on Linux. `getPriority`/`setPriority` throw.
"use strict";

function notAvailable(what) {
  return function () {
    throw new Error("os." + what + "() isn't available in js2wasi3 (WASI has no such information)");
  };
}

function tmpdir() {
  const env = process.env;
  let path = env.TMPDIR || env.TMP || env.TEMP || "/tmp";
  if (path.length > 1 && path.endsWith("/")) path = path.slice(0, -1);
  return path;
}

function homedir() {
  return process.env.HOME || "/";
}

function userInfo(options) {
  const home = homedir();
  const info = { __proto__: null, uid: -1, gid: -1, username: process.env.USER || process.env.LOGNAME || "", homedir: home, shell: null };
  return info;
}

const signals = {"SIGHUP":1,"SIGINT":2,"SIGQUIT":3,"SIGILL":4,"SIGTRAP":5,"SIGABRT":6,"SIGIOT":6,"SIGBUS":7,"SIGFPE":8,"SIGKILL":9,"SIGUSR1":10,"SIGSEGV":11,"SIGUSR2":12,"SIGPIPE":13,"SIGALRM":14,"SIGTERM":15,"SIGCHLD":17,"SIGSTKFLT":16,"SIGCONT":18,"SIGSTOP":19,"SIGTSTP":20,"SIGTTIN":21,"SIGTTOU":22,"SIGURG":23,"SIGXCPU":24,"SIGXFSZ":25,"SIGVTALRM":26,"SIGPROF":27,"SIGWINCH":28,"SIGIO":29,"SIGPOLL":29,"SIGPWR":30,"SIGSYS":31};
const errno = {"E2BIG":7,"EACCES":13,"EADDRINUSE":98,"EADDRNOTAVAIL":99,"EAFNOSUPPORT":97,"EAGAIN":11,"EALREADY":114,"EBADF":9,"EBADMSG":74,"EBUSY":16,"ECANCELED":125,"ECHILD":10,"ECONNABORTED":103,"ECONNREFUSED":111,"ECONNRESET":104,"EDEADLK":35,"EDESTADDRREQ":89,"EDOM":33,"EDQUOT":122,"EEXIST":17,"EFAULT":14,"EFBIG":27,"EHOSTUNREACH":113,"EIDRM":43,"EILSEQ":84,"EINPROGRESS":115,"EINTR":4,"EINVAL":22,"EIO":5,"EISCONN":106,"EISDIR":21,"ELOOP":40,"EMFILE":24,"EMLINK":31,"EMSGSIZE":90,"EMULTIHOP":72,"ENAMETOOLONG":36,"ENETDOWN":100,"ENETRESET":102,"ENETUNREACH":101,"ENFILE":23,"ENOBUFS":105,"ENODATA":61,"ENODEV":19,"ENOENT":2,"ENOEXEC":8,"ENOLCK":37,"ENOLINK":67,"ENOMEM":12,"ENOMSG":42,"ENOPROTOOPT":92,"ENOSPC":28,"ENOSR":63,"ENOSTR":60,"ENOSYS":38,"ENOTCONN":107,"ENOTDIR":20,"ENOTEMPTY":39,"ENOTSOCK":88,"ENOTSUP":95,"ENOTTY":25,"ENXIO":6,"EOPNOTSUPP":95,"EOVERFLOW":75,"EPERM":1,"EPIPE":32,"EPROTO":71,"EPROTONOSUPPORT":93,"EPROTOTYPE":91,"ERANGE":34,"EROFS":30,"ESPIPE":29,"ESRCH":3,"ESTALE":116,"ETIME":62,"ETIMEDOUT":110,"ETXTBSY":26,"EWOULDBLOCK":11,"EXDEV":18};
const priority = {"PRIORITY_LOW":19,"PRIORITY_BELOW_NORMAL":10,"PRIORITY_NORMAL":0,"PRIORITY_ABOVE_NORMAL":-7,"PRIORITY_HIGH":-14,"PRIORITY_HIGHEST":-20};
const dlopen = {"RTLD_LAZY":1,"RTLD_NOW":2,"RTLD_GLOBAL":256,"RTLD_LOCAL":0,"RTLD_DEEPBIND":8};
const constants = { UV_UDP_REUSEADDR: 4, dlopen, errno, signals, priority };

module.exports = {
  arch: function arch() {
    return process.arch;
  },
  availableParallelism: function availableParallelism() {
    return 1;
  },
  cpus: function cpus() {
    return [{ model: "wasm32", speed: 0, times: { user: 0, nice: 0, sys: 0, idle: 0, irq: 0 } }];
  },
  endianness: function endianness() {
    return "LE";
  },
  freemem: function freemem() {
    return 1073741824;
  },
  getPriority: notAvailable("getPriority"),
  homedir,
  hostname: function hostname() {
    return "localhost";
  },
  loadavg: function loadavg() {
    return [0, 0, 0];
  },
  networkInterfaces: function networkInterfaces() {
    return {};
  },
  platform: function platform() {
    return process.platform;
  },
  release: function release() {
    return "0.0.0";
  },
  setPriority: notAvailable("setPriority"),
  tmpdir,
  totalmem: function totalmem() {
    return 4294967296;
  },
  type: function type() {
    return "Linux";
  },
  userInfo,
  uptime: function uptime() {
    return Math.floor(process.uptime());
  },
  version: function version() {
    return "#1 SMP js2wasi3 (WASI 0.3)";
  },
  machine: function machine() {
    return "wasm32";
  },
  constants,
  EOL: "\n",
  devNull: "/dev/null",
};
