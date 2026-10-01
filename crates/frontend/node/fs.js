// node's `fs` module (T59), over `wasi:filesystem` (`node:internal/wasi_fs`): the sync API. Paths
// resolve against `process.cwd()` and must lie under a preopened directory (`wasmtime run --dir
// host::guest`); the longest preopen that is a prefix of the absolute path is used. Errors are
// node's (`ENOENT: no such file or directory, open 'x'`, with `errno`, `code`, `syscall`, `path`).
// WASI has no permissions, owners or inode numbers: `Stats` has node's shape with `mode` from the
// file type (0o100644, 0o40755, 0o120777) and zeros for the rest.
"use strict";

const { Buffer } = require("buffer");
const pathModule = require("path");
const wasi = require("node:internal/wasi_fs");

// `error-code` tags → [code, errno (Linux), libuv's message].
const ERRORS = {
  access: ["EACCES", -13, "permission denied"],
  already: ["EALREADY", -114, "connection already in progress"],
  "bad-descriptor": ["EBADF", -9, "bad file descriptor"],
  busy: ["EBUSY", -16, "resource busy or locked"],
  deadlock: ["EDEADLK", -35, "resource deadlock avoided"],
  quota: ["EDQUOT", -122, "disk quota exceeded"],
  exist: ["EEXIST", -17, "file already exists"],
  "file-too-large": ["EFBIG", -27, "file too large"],
  "illegal-byte-sequence": ["EILSEQ", -84, "illegal byte sequence"],
  "in-progress": ["EINPROGRESS", -115, "operation in progress"],
  interrupted: ["EINTR", -4, "interrupted system call"],
  invalid: ["EINVAL", -22, "invalid argument"],
  io: ["EIO", -5, "i/o error"],
  "is-directory": ["EISDIR", -21, "illegal operation on a directory"],
  loop: ["ELOOP", -40, "too many symbolic links encountered"],
  "too-many-links": ["EMLINK", -31, "too many links"],
  "message-size": ["EMSGSIZE", -90, "message too long"],
  "name-too-long": ["ENAMETOOLONG", -36, "name too long"],
  "no-device": ["ENODEV", -19, "no such device"],
  "no-entry": ["ENOENT", -2, "no such file or directory"],
  "no-lock": ["ENOLCK", -37, "no locks available"],
  "insufficient-memory": ["ENOMEM", -12, "not enough memory"],
  "insufficient-space": ["ENOSPC", -28, "no space left on device"],
  "not-directory": ["ENOTDIR", -20, "not a directory"],
  "not-empty": ["ENOTEMPTY", -39, "directory not empty"],
  "not-recoverable": ["ENOTRECOVERABLE", -131, "state not recoverable"],
  unsupported: ["ENOTSUP", -95, "operation not supported on socket"],
  "no-tty": ["ENOTTY", -25, "inappropriate ioctl for device"],
  "no-such-device": ["ENXIO", -6, "no such device or address"],
  overflow: ["EOVERFLOW", -75, "value too large for defined data type"],
  "not-permitted": ["EPERM", -1, "operation not permitted"],
  pipe: ["EPIPE", -32, "broken pipe"],
  "read-only": ["EROFS", -30, "read-only file system"],
  "invalid-seek": ["ESPIPE", -29, "invalid seek"],
  "text-file-busy": ["ETXTBSY", -26, "text file is busy"],
  "cross-device": ["EXDEV", -18, "cross-device link not permitted"],
};

function uvError(tag, syscall, path, dest) {
  const [code, errno, desc] = ERRORS[tag] || ["EIO", -5, "i/o error"];
  let message = `${code}: ${desc}, ${syscall}`;
  if (path !== undefined) {
    message += ` '${path}'`;
  }
  if (dest !== undefined) {
    message += ` -> '${dest}'`;
  }
  const e = new Error(message);
  e.errno = errno;
  e.code = code;
  e.syscall = syscall;
  if (path !== undefined) {
    e.path = path;
  }
  if (dest !== undefined) {
    e.dest = dest;
  }
  return e;
}

// Run `f`; a WASI error becomes node's error for `syscall` on `path` (and `dest`).
function call(syscall, path, dest, f) {
  try {
    return f();
  } catch (e) {
    if (e !== null && typeof e === "object" && e.payload && typeof e.payload.tag === "string") {
      throw uvError(e.payload.tag, syscall, path, dest);
    }
    throw e;
  }
}

// A `result<_, error-code>` a future gave: throw its `err`.
function check(r, syscall, path) {
  if (r.tag === "err") {
    throw uvError(r.val.tag, syscall, path);
  }
}

// ---- paths -------------------------------------------------------------------------------

let preopens;

function pathString(p, name) {
  if (typeof p === "string") {
    return p;
  }
  if (p instanceof Uint8Array) {
    return Buffer.from(p).toString();
  }
  if (p !== null && typeof p === "object" && p.protocol === "file:" && typeof p.pathname === "string") {
    return decodeURIComponent(p.pathname);
  }
  const e = new TypeError(
    `The "${name || "path"}" argument must be of type string or an instance of Buffer or URL. Received ${describe(p)}`,
  );
  e.code = "ERR_INVALID_ARG_TYPE";
  throw e;
}

// How node's ERR_INVALID_ARG_TYPE describes a value (without `util`: it's big).
function describe(v) {
  if (v === undefined || v === null) {
    return String(v);
  }
  if (typeof v === "function") {
    return `function ${v.name}`;
  }
  if (typeof v === "object") {
    return v.constructor && v.constructor.name ? `an instance of ${v.constructor.name}` : "[Object: null prototype]";
  }
  return `type ${typeof v} (${typeof v === "string" ? `'${v}'` : String(v)})`;
}

// The preopened directory and the path relative to it for `p` (as given: for errors).
function at(p, syscall) {
  const abs = pathModule.resolve(p);
  if (preopens === undefined) {
    preopens = wasi
      .getDirectories()
      .map((d) => [d[0], pathModule.resolve(d[1])])
      .sort((a, b) => b[1].length - a[1].length);
  }
  for (const [dir, base] of preopens) {
    if (abs === base) {
      return [dir, "."];
    }
    const prefix = base.endsWith("/") ? base : base + "/";
    if (abs.startsWith(prefix)) {
      return [dir, abs.slice(prefix.length)];
    }
  }
  throw uvError("no-entry", syscall, p);
}

// ---- open flags ----------------------------------------------------------------------------

const constants = {
  UV_FS_SYMLINK_DIR: 1,
  UV_FS_SYMLINK_JUNCTION: 2,
  O_RDONLY: 0,
  O_WRONLY: 1,
  O_RDWR: 2,
  UV_DIRENT_UNKNOWN: 0,
  UV_DIRENT_FILE: 1,
  UV_DIRENT_DIR: 2,
  UV_DIRENT_LINK: 3,
  UV_DIRENT_FIFO: 4,
  UV_DIRENT_SOCKET: 5,
  UV_DIRENT_CHAR: 6,
  UV_DIRENT_BLOCK: 7,
  EXTENSIONLESS_FORMAT_JAVASCRIPT: 0,
  EXTENSIONLESS_FORMAT_WASM: 1,
  S_IFMT: 61440,
  S_IFREG: 32768,
  S_IFDIR: 16384,
  S_IFCHR: 8192,
  S_IFBLK: 24576,
  S_IFIFO: 4096,
  S_IFLNK: 40960,
  S_IFSOCK: 49152,
  O_CREAT: 64,
  O_EXCL: 128,
  UV_FS_O_FILEMAP: 0,
  O_NOCTTY: 256,
  O_TRUNC: 512,
  O_APPEND: 1024,
  O_DIRECTORY: 65536,
  O_NOATIME: 262144,
  O_NOFOLLOW: 131072,
  O_SYNC: 1052672,
  O_DSYNC: 4096,
  O_DIRECT: 16384,
  O_NONBLOCK: 2048,
  S_IRWXU: 448,
  S_IRUSR: 256,
  S_IWUSR: 128,
  S_IXUSR: 64,
  S_IRWXG: 56,
  S_IRGRP: 32,
  S_IWGRP: 16,
  S_IXGRP: 8,
  S_IRWXO: 7,
  S_IROTH: 4,
  S_IWOTH: 2,
  S_IXOTH: 1,
  F_OK: 0,
  R_OK: 4,
  W_OK: 2,
  X_OK: 1,
  UV_FS_COPYFILE_EXCL: 1,
  COPYFILE_EXCL: 1,
  UV_FS_COPYFILE_FICLONE: 2,
  COPYFILE_FICLONE: 2,
  UV_FS_COPYFILE_FICLONE_FORCE: 4,
  COPYFILE_FICLONE_FORCE: 4,
};

const FLAGS = {
  r: 0,
  rs: 0,
  sr: 0,
  "r+": 2,
  "rs+": 2,
  "sr+": 2,
  w: 1 | 64 | 512,
  wx: 1 | 64 | 512 | 128,
  xw: 1 | 64 | 512 | 128,
  "w+": 2 | 64 | 512,
  "wx+": 2 | 64 | 512 | 128,
  "xw+": 2 | 64 | 512 | 128,
  a: 1 | 64 | 1024,
  ax: 1 | 64 | 1024 | 128,
  xa: 1 | 64 | 1024 | 128,
  as: 1 | 64 | 1024,
  sa: 1 | 64 | 1024,
  "a+": 2 | 64 | 1024,
  "ax+": 2 | 64 | 1024 | 128,
  "xa+": 2 | 64 | 1024 | 128,
  "as+": 2 | 64 | 1024,
  "sa+": 2 | 64 | 1024,
};

function flagsNumber(flags, fallback) {
  if (flags === undefined || flags === null) {
    return FLAGS[fallback];
  }
  if (typeof flags === "number") {
    return flags;
  }
  const n = FLAGS[flags];
  if (n === undefined) {
    const e = new TypeError(`The argument 'flags' is invalid. Received ${typeof flags === "string" ? `'${flags}'` : String(flags)}`);
    e.code = "ERR_INVALID_ARG_VALUE";
    throw e;
  }
  return n;
}

// Open `p` with node's numeric flags: the descriptor and whether writes append. `shown`: the path
// errors name (`null`: none, `opendirSync`'s).
function openPath(p, flags, syscall, shown = p) {
  shown = shown === null ? undefined : shown;
  const [dir, rel] = at(p, syscall);
  const access = flags & 3;
  const open = {
    create: (flags & 64) !== 0,
    directory: (flags & 65536) !== 0,
    exclusive: (flags & 128) !== 0,
    truncate: (flags & 512) !== 0,
  };
  const desc = { read: access !== 1, write: access !== 0 };
  if (open.truncate) {
    writes++;
  }
  const d = call(syscall, shown, undefined, () =>
    wasi.openAt(dir, { symlinkFollow: (flags & 131072) === 0 }, rel, open, desc),
  );
  return d;
}

// ---- file descriptors ----------------------------------------------------------------------

// node's fds are the OS's; here they index this table from 3 (0-2: stdio, not openable).
const fds = [];

function file(fd, syscall) {
  const f = typeof fd === "number" ? fds[fd] : undefined;
  if (f === undefined) {
    throw uvError("bad-descriptor", syscall);
  }
  return f;
}

function openSync(path, flags, mode) {
  const p = pathString(path);
  const n = flagsNumber(flags, "r");
  const d = openPath(p, n, "open");
  let fd = 3;
  while (fds[fd] !== undefined) {
    fd++;
  }
  fds[fd] = { d, pos: 0, append: (n & 1024) !== 0 };
  return fd;
}

function closeSync(fd) {
  const f = file(fd, "close");
  wasi.drop(f.d);
  fds[fd] = undefined;
}

// Writes and truncations through `fs` so far (T60): a descriptor's cached bytes (`readSync`) are
// stale after one. Writes by other processes aren't seen while a cache is used.
let writes = 0;

// All bytes of descriptor `d` from `offset`.
function readFrom(d, offset) {
  const r = call("read", undefined, undefined, () => wasi.readViaStream(d, offset));
  check(r[1], "read");
  return r[0];
}

// Write `bytes` to descriptor `d` at `offset` (or appending).
function writeTo(d, bytes, offset, append) {
  writes++;
  const r = call("write", undefined, undefined, () =>
    append ? wasi.appendViaStream(d, bytes) : wasi.writeViaStream(d, bytes, offset),
  );
  check(r, "write");
}

function readSync(fd, buffer, offsetOrOptions, length, position) {
  const f = file(fd, "read");
  let offset = offsetOrOptions;
  if (offset !== null && typeof offset === "object") {
    ({ offset = 0, length = buffer.byteLength - offset, position = null } = offset);
  }
  if (offset === undefined) {
    offset = 0;
  }
  if (length === undefined) {
    length = buffer.byteLength - offset;
  }
  const from = position === null || position === undefined || position === -1 ? f.pos : Number(position);
  // WASI reads a descriptor's bytes to its end (`read-via-stream`, T59): kept for the next reads
  // (T60: a read stream's chunks), unless `fs` wrote since (F252).
  let bytes;
  const c = f.cache;
  if (c !== undefined && c.writes === writes && from >= c.from && from <= c.from + c.bytes.length) {
    bytes = c.bytes.subarray(from - c.from);
  } else {
    bytes = readFrom(f.d, from);
    f.cache = { writes, from, bytes };
  }
  const n = Math.min(length, bytes.length);
  const view = new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  view.set(bytes.subarray(0, n), offset);
  if (position === null || position === undefined || position === -1) {
    f.pos += n;
  }
  return n;
}

function writeSync(fd, data, offsetOrPosition, lengthOrEncoding, position) {
  const f = file(fd, "write");
  let bytes;
  let pos;
  if (typeof data === "string") {
    bytes = Buffer.from(data, typeof lengthOrEncoding === "string" ? lengthOrEncoding : "utf8");
    pos = offsetOrPosition;
  } else {
    const offset = offsetOrPosition === undefined || offsetOrPosition === null ? 0 : offsetOrPosition;
    const length = lengthOrEncoding === undefined ? data.byteLength - offset : lengthOrEncoding;
    bytes = new Uint8Array(data.buffer, data.byteOffset + offset, length);
    pos = position;
  }
  const at = pos === null || pos === undefined ? f.pos : pos;
  writeTo(f.d, bytes, at, f.append);
  if (pos === null || pos === undefined) {
    f.pos += bytes.length;
  }
  return bytes.length;
}

function fsyncSync(fd) {
  const f = file(fd, "fsync");
  call("fsync", undefined, undefined, () => wasi.sync(f.d));
}

function fdatasyncSync(fd) {
  const f = file(fd, "fdatasync");
  call("fdatasync", undefined, undefined, () => wasi.syncData(f.d));
}

function ftruncateSync(fd, len) {
  const f = file(fd, "ftruncate");
  writes++;
  call("ftruncate", undefined, undefined, () => wasi.setSize(f.d, len === undefined ? 0 : len));
}

function fstatSync(fd, options) {
  const f = file(fd, "fstat");
  return new Stats(call("fstat", undefined, undefined, () => wasi.stat(f.d)));
}

// ---- whole files ---------------------------------------------------------------------------

function encodingOf(options, fallback) {
  if (typeof options === "string") {
    return options;
  }
  if (options !== null && typeof options === "object" && options.encoding !== undefined) {
    return options.encoding;
  }
  return fallback;
}

function flagOf(options, fallback) {
  if (options !== null && typeof options === "object" && options.flag !== undefined) {
    return options.flag;
  }
  return fallback;
}

function readFileSync(path, options) {
  const encoding = encodingOf(options, null);
  let bytes;
  if (typeof path === "number") {
    const f = file(path, "read");
    bytes = readFrom(f.d, f.pos);
    f.pos += bytes.length;
  } else {
    const p = pathString(path);
    const d = openPath(p, flagsNumber(flagOf(options, "r"), "r"), "open");
    try {
      bytes = readFrom(d, 0);
    } finally {
      wasi.drop(d);
    }
  }
  const buf = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.length);
  return encoding ? buf.toString(encoding) : buf;
}

function toBytes(data, encoding) {
  if (typeof data === "string") {
    return Buffer.from(data, encoding || "utf8");
  }
  if (ArrayBuffer.isView(data)) {
    return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  }
  const e = new TypeError(
    `The "data" argument must be of type string or an instance of Buffer, TypedArray, or DataView. Received ${describe(data)}`,
  );
  e.code = "ERR_INVALID_ARG_TYPE";
  throw e;
}

function writeFileSync(path, data, options) {
  const bytes = toBytes(data, encodingOf(options, "utf8"));
  if (typeof path === "number") {
    const f = file(path, "write");
    writeTo(f.d, bytes, f.pos, f.append);
    f.pos += bytes.length;
    return;
  }
  const p = pathString(path);
  const n = flagsNumber(flagOf(options, "w"), "w");
  const d = openPath(p, n, "open");
  try {
    writeTo(d, bytes, 0, (n & 1024) !== 0);
  } finally {
    wasi.drop(d);
  }
}

function appendFileSync(path, data, options) {
  const flag = flagOf(options, "a");
  writeFileSync(path, data, typeof options === "object" && options !== null ? { ...options, flag } : { encoding: encodingOf(options, "utf8"), flag });
}

function copyFileSync(src, dest, mode) {
  const s = pathString(src, "src");
  const t = pathString(dest, "dest");
  let bytes;
  const d = openPath(s, 0, "copyfile");
  try {
    bytes = readFrom(d, 0);
  } finally {
    wasi.drop(d);
  }
  const flags = 1 | 64 | 512 | ((mode & 1) !== 0 ? 128 : 0);
  let w;
  try {
    w = openPath(t, flags, "copyfile");
  } catch (e) {
    if (e.syscall === "copyfile") {
      e.message = `${e.message.slice(0, e.message.lastIndexOf(" '"))} '${s}' -> '${t}'`;
      e.path = s;
      e.dest = t;
    }
    throw e;
  }
  try {
    writeTo(w, bytes, 0, false);
  } finally {
    wasi.drop(w);
  }
}

// ---- metadata ------------------------------------------------------------------------------

const TYPE_MODE = {
  "regular-file": 0o100644,
  directory: 0o40755,
  "symbolic-link": 0o120777,
  "character-device": 0o20666,
  "block-device": 0o60660,
  fifo: 0o10644,
  socket: 0o140755,
};

function ms(t) {
  return t === undefined ? 0 : t.seconds * 1000 + Math.floor(t.nanoseconds / 1e6);
}

class Stats {
  constructor(st) {
    const atimeMs = ms(st.dataAccessTimestamp);
    const mtimeMs = ms(st.dataModificationTimestamp);
    const ctimeMs = ms(st.statusChangeTimestamp);
    this.dev = 0;
    this.mode = TYPE_MODE[st.type.tag] || 0;
    this.nlink = st.linkCount;
    this.uid = 0;
    this.gid = 0;
    this.rdev = 0;
    this.blksize = 4096;
    this.ino = 0;
    this.size = st.size;
    this.blocks = Math.ceil(st.size / 512);
    this.atimeMs = atimeMs;
    this.mtimeMs = mtimeMs;
    this.ctimeMs = ctimeMs;
    this.birthtimeMs = ctimeMs;
  }
  get atime() {
    return new Date(this.atimeMs);
  }
  get mtime() {
    return new Date(this.mtimeMs);
  }
  get ctime() {
    return new Date(this.ctimeMs);
  }
  get birthtime() {
    return new Date(this.birthtimeMs);
  }
  isFile() {
    return (this.mode & 61440) === 32768;
  }
  isDirectory() {
    return (this.mode & 61440) === 16384;
  }
  isSymbolicLink() {
    return (this.mode & 61440) === 40960;
  }
  isBlockDevice() {
    return (this.mode & 61440) === 24576;
  }
  isCharacterDevice() {
    return (this.mode & 61440) === 8192;
  }
  isFIFO() {
    return (this.mode & 61440) === 4096;
  }
  isSocket() {
    return (this.mode & 61440) === 49152;
  }
}

function statPath(path, options, follow, syscall) {
  const p = pathString(path);
  const [dir, rel] = at(p, syscall);
  try {
    return new Stats(call(syscall, p, undefined, () => wasi.statAt(dir, { symlinkFollow: follow }, rel)));
  } catch (e) {
    if (e.code === "ENOENT" && options && options.throwIfNoEntry === false) {
      return undefined;
    }
    throw e;
  }
}

function statSync(path, options) {
  return statPath(path, options, true, "stat");
}

function lstatSync(path, options) {
  return statPath(path, options, false, "lstat");
}

function existsSync(path) {
  try {
    statPath(path, undefined, true, "stat");
    return true;
  } catch {
    return false;
  }
}

function accessSync(path, mode) {
  statPath(path, undefined, true, "access");
}

function utimesSync(path, atime, mtime) {
  const p = pathString(path);
  const [dir, rel] = at(p, "utime");
  call("utime", p, undefined, () =>
    wasi.setTimesAt(dir, { symlinkFollow: true }, rel, timestamp(atime), timestamp(mtime)),
  );
}

function timestamp(t) {
  const s = t instanceof Date ? t.getTime() / 1000 : Number(t);
  const seconds = Math.floor(s);
  return { tag: "timestamp", val: { seconds, nanoseconds: Math.round((s - seconds) * 1e9) } };
}

function truncateSync(path, len) {
  if (typeof path === "number") {
    return ftruncateSync(path, len);
  }
  const p = pathString(path);
  const d = openPath(p, 1, "open");
  try {
    writes++;
    call("ftruncate", undefined, undefined, () => wasi.setSize(d, len === undefined ? 0 : len));
  } finally {
    wasi.drop(d);
  }
}

// ---- directories ---------------------------------------------------------------------------

const kType = Symbol("type");
const DIRENT = {
  "regular-file": 1,
  directory: 2,
  "symbolic-link": 3,
  fifo: 4,
  socket: 5,
  "character-device": 6,
  "block-device": 7,
};

class Dirent {
  constructor(name, type, parentPath) {
    this.name = name;
    this.parentPath = parentPath;
    this[kType] = type;
  }
  isFile() {
    return this[kType] === 1;
  }
  isDirectory() {
    return this[kType] === 2;
  }
  isSymbolicLink() {
    return this[kType] === 3;
  }
  isFIFO() {
    return this[kType] === 4;
  }
  isSocket() {
    return this[kType] === 5;
  }
  isCharacterDevice() {
    return this[kType] === 6;
  }
  isBlockDevice() {
    return this[kType] === 7;
  }
}

// The entries of directory `p`: `[name, type tag]`s.
function entries(p, syscall, shown = p) {
  const d = openPath(p, 65536, syscall, shown);
  shown = shown === null ? undefined : shown;
  try {
    const r = call(syscall, shown, undefined, () => wasi.readDirectory(d));
    check(r[1], syscall, shown);
    return r[0].map((e) => [e.name, e.type.tag]);
  } finally {
    wasi.drop(d);
  }
}

function readdirSync(path, options) {
  const p = pathString(path);
  const withFileTypes = options !== null && typeof options === "object" && options.withFileTypes;
  const recursive = options !== null && typeof options === "object" && options.recursive;
  const out = [];
  const walk = (dir, prefix) => {
    for (const [name, tag] of entries(dir, "scandir")) {
      const rel = prefix === "" ? name : pathModule.join(prefix, name);
      out.push(withFileTypes ? new Dirent(name, DIRENT[tag] || 0, dir) : rel);
      if (recursive && tag === "directory") {
        walk(pathModule.join(dir, name), rel);
      }
    }
  };
  walk(p, "");
  return out;
}

// `fs.Dir` (T60): its entries are read when it's opened (WASI's `read-directory` is a stream,
// read whole).
const kDir = Symbol("kDir");

function dirClosed() {
  const e = new Error("Directory handle was closed");
  e.code = "ERR_DIR_CLOSED";
  return e;
}

class Dir {
  constructor(path, list) {
    this[kDir] = { path, list, closed: false };
  }
  get path() {
    return this[kDir].path;
  }
  readSync() {
    const d = this[kDir];
    if (d.closed) {
      throw dirClosed();
    }
    return d.list.length === 0 ? null : d.list.shift();
  }
  read(cb) {
    if (typeof cb !== "function") {
      return new Promise((resolve, reject) => this.read((err, e) => (err ? reject(err) : resolve(e))));
    }
    let e;
    try {
      e = this.readSync();
    } catch (err) {
      process.nextTick(cb, err);
      return;
    }
    process.nextTick(cb, null, e);
  }
  closeSync() {
    const d = this[kDir];
    if (d.closed) {
      throw dirClosed();
    }
    d.closed = true;
  }
  close(cb) {
    if (typeof cb !== "function") {
      return new Promise((resolve, reject) => this.close((err) => (err ? reject(err) : resolve())));
    }
    try {
      this.closeSync();
    } catch (err) {
      process.nextTick(cb, err);
      return;
    }
    process.nextTick(cb, null);
  }
  async *entries() {
    try {
      while (true) {
        const e = this.readSync();
        if (e === null) {
          break;
        }
        yield e;
      }
    } finally {
      if (!this[kDir].closed) {
        this.closeSync();
      }
    }
  }
  [Symbol.asyncIterator]() {
    return this.entries();
  }
}

function openDir(path, shown) {
  const p = pathString(path);
  return new Dir(p, entries(p, "opendir", shown).map(([name, tag]) => new Dirent(name, DIRENT[tag] || 0, p)));
}

function opendirSync(path) {
  return openDir(path, null);
}

function opendir(path, options, cb) {
  if (typeof options === "function") {
    cb = options;
  }
  let dir;
  try {
    dir = openDir(path, pathString(path));
  } catch (err) {
    process.nextTick(cb, err);
    return;
  }
  process.nextTick(cb, null, dir);
}

function mkdirSync(path, options) {
  const p = pathString(path);
  const recursive = options !== null && typeof options === "object" && options.recursive;
  if (!recursive) {
    const [dir, rel] = at(p, "mkdir");
    call("mkdir", p, undefined, () => wasi.createDirectoryAt(dir, rel));
    return undefined;
  }
  // Create each missing ancestor: the first one created is the result.
  let first;
  const parts = [];
  let cur = p;
  while (true) {
    const st = statPath(cur, { throwIfNoEntry: false }, true, "mkdir");
    if (st !== undefined) {
      if (!st.isDirectory()) {
        throw uvError(cur === p ? "exist" : "not-directory", "mkdir", p);
      }
      break;
    }
    parts.push(cur);
    const parent = pathModule.dirname(cur);
    if (parent === cur) {
      break;
    }
    cur = parent;
  }
  for (let i = parts.length - 1; i >= 0; i--) {
    const [dir, rel] = at(parts[i], "mkdir");
    call("mkdir", p, undefined, () => wasi.createDirectoryAt(dir, rel));
    if (first === undefined) {
      first = parts[i];
    }
  }
  return first;
}

function rmdirSync(path, options) {
  const p = pathString(path);
  if (options && options.recursive) {
    return rmSync(p, { recursive: true });
  }
  const [dir, rel] = at(p, "rmdir");
  call("rmdir", p, undefined, () => wasi.removeDirectoryAt(dir, rel));
}

function unlinkSync(path) {
  const p = pathString(path);
  const [dir, rel] = at(p, "unlink");
  call("unlink", p, undefined, () => wasi.unlinkFileAt(dir, rel));
}

function rmSync(path, options) {
  const p = pathString(path);
  const recursive = options && options.recursive;
  const force = options && options.force;
  const st = statPath(p, { throwIfNoEntry: false }, false, "lstat");
  if (st === undefined) {
    if (force) {
      return;
    }
    throw uvError("no-entry", "lstat", p);
  }
  if (st.isDirectory()) {
    if (!recursive) {
      const e = new Error(`Path is a directory: rm returned EISDIR (is a directory) ${p}`);
      e.name = "SystemError";
      e.code = "ERR_FS_EISDIR";
      e.errno = 21;
      e.syscall = "rm";
      e.path = p;
      throw e;
    }
    for (const [name] of entries(p, "scandir")) {
      rmSync(pathModule.join(p, name), options);
    }
    rmdirSync(p);
  } else {
    unlinkSync(p);
  }
}

function renameSync(oldPath, newPath) {
  const o = pathString(oldPath, "oldPath");
  const n = pathString(newPath, "newPath");
  const [od, orel] = at(o, "rename");
  const [nd, nrel] = at(n, "rename");
  call("rename", o, n, () => wasi.renameAt(od, orel, nd, nrel));
}

function linkSync(existingPath, newPath) {
  const o = pathString(existingPath, "existingPath");
  const n = pathString(newPath, "newPath");
  const [od, orel] = at(o, "link");
  const [nd, nrel] = at(n, "link");
  call("link", o, n, () => wasi.linkAt(od, {}, orel, nd, nrel));
}

function symlinkSync(target, path, type) {
  const t = pathString(target, "target");
  const p = pathString(path);
  const [dir, rel] = at(p, "symlink");
  call("symlink", t, p, () => wasi.symlinkAt(dir, t, rel));
}

function readlinkSync(path, options) {
  const p = pathString(path);
  const [dir, rel] = at(p, "readlink");
  const target = call("readlink", p, undefined, () => wasi.readlinkAt(dir, rel));
  return encodingOf(options, "utf8") === "buffer" ? Buffer.from(target) : target;
}

function realpathSync(path, options) {
  const p = pathString(path);
  let parts = pathModule.resolve(p).split("/").filter((s) => s !== "");
  let out = "/";
  let links = 0;
  while (parts.length > 0) {
    const next = pathModule.join(out, parts.shift());
    const st = statPath(next, undefined, false, "lstat");
    if (st.isSymbolicLink()) {
      if (++links > 40) {
        throw uvError("loop", "realpath", p);
      }
      const target = readlinkSync(next);
      const resolved = pathModule.resolve(out, target);
      parts = resolved.split("/").filter((s) => s !== "").concat(parts);
      out = "/";
    } else {
      out = next;
    }
  }
  return encodingOf(options, "utf8") === "buffer" ? Buffer.from(out) : out;
}
realpathSync.native = realpathSync;

// ---- permissions (WASI has none) -----------------------------------------------------------

// The file must exist; nothing else happens.
function chmodSync(path, mode) {
  statPath(path, undefined, true, "chmod");
}

function lchmodSync(path, mode) {
  statPath(path, undefined, false, "lchmod");
}

function chownSync(path, uid, gid) {
  statPath(path, undefined, true, "chown");
}

function lchownSync(path, uid, gid) {
  statPath(path, undefined, false, "lchown");
}

function fchmodSync(fd, mode) {
  file(fd, "fchmod");
}

function fchownSync(fd, uid, gid) {
  file(fd, "fchown");
}

function mkdtempSync(prefix, options) {
  const chars = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
  for (;;) {
    let name = pathString(prefix, "prefix");
    for (let i = 0; i < 6; i++) {
      name += chars[Math.floor(Math.random() * chars.length)];
    }
    try {
      mkdirSync(name);
      return name;
    } catch (e) {
      if (e.code !== "EEXIST") {
        throw e;
      }
    }
  }
}

// ---- callbacks and promises ----------------------------------------------------------------
// They run the sync call at once (node: on its thread pool) and deliver the result later: a
// callback in a later turn of the event loop (`setImmediate`), a promise settled with it. node's
// errors of the call itself (`syscall` set) go to the callback; argument errors are thrown.

function callbackify(sync, name) {
  return function (...args) {
    const cb = args.pop();
    if (typeof cb !== "function") {
      const e = new TypeError(`The "cb" argument must be of type function. Received ${describe(cb)}`);
      e.code = "ERR_INVALID_ARG_TYPE";
      throw e;
    }
    let result;
    try {
      result = sync(...args);
    } catch (e) {
      if (e === null || typeof e !== "object" || e.syscall === undefined) {
        throw e;
      }
      setImmediate(cb, e);
      return;
    }
    setImmediate(cb, null, result);
  };
}

function exists(path, cb) {
  const r = existsSync(path);
  setImmediate(cb, r);
}

function read(fd, buffer, offset, length, position, cb) {
  if (typeof offset === "function") {
    cb = offset;
    offset = undefined;
  }
  const f = callbackify((...a) => readSync(...a), "read");
  f(fd, buffer, offset, length, position, (err, n) => (err ? cb(err) : cb(null, n, buffer)));
}

function write(fd, data, ...rest) {
  const cb = rest.pop();
  callbackify((...a) => writeSync(...a), "write")(fd, data, ...rest, (err, n) =>
    err ? cb(err) : cb(null, n, data),
  );
}

// `fs.promises`' `FileHandle`.
class FileHandle {
  constructor(fd) {
    this.fd = fd;
  }
  async readFile(options) {
    return readFileSync(this.fd, options);
  }
  async writeFile(data, options) {
    return writeFileSync(this.fd, data, options);
  }
  async appendFile(data, options) {
    return writeFileSync(this.fd, data, options);
  }
  async read(buffer, offset, length, position) {
    if (buffer === undefined) {
      buffer = Buffer.alloc(16384);
    }
    const bytesRead = readSync(this.fd, buffer, offset, length, position);
    return { bytesRead, buffer };
  }
  async write(data, ...rest) {
    const bytesWritten = writeSync(this.fd, data, ...rest);
    return { bytesWritten, buffer: data };
  }
  async stat(options) {
    return fstatSync(this.fd, options);
  }
  async truncate(len) {
    return ftruncateSync(this.fd, len);
  }
  async sync() {
    return fsyncSync(this.fd);
  }
  async datasync() {
    return fdatasyncSync(this.fd);
  }
  async chmod(mode) {
    return fchmodSync(this.fd, mode);
  }
  async close() {
    if (this.fd !== -1) {
      closeSync(this.fd);
      this.fd = -1;
    }
  }
}

function promisify(sync) {
  return async (...args) => sync(...args);
}

// ---- streams (T60): node 24's lib/internal/fs/streams.js, loaded on first use ----------------

let ReadStream;
let WriteStream;
function lazyLoadStreams() {
  if (!ReadStream) {
    ({ ReadStream, WriteStream } = require("node:internal/fs/streams"));
  }
}

function createReadStream(path, options) {
  lazyLoadStreams();
  return new ReadStream(path, options);
}

function createWriteStream(path, options) {
  lazyLoadStreams();
  return new WriteStream(path, options);
}

function unsupported(name) {
  return function () {
    throw new Error(`fs.${name} isn't supported by js2wasi3 yet`);
  };
}

const sync = {
  accessSync,
  appendFileSync,
  chmodSync,
  chownSync,
  closeSync,
  copyFileSync,
  existsSync,
  fchmodSync,
  fchownSync,
  fdatasyncSync,
  fstatSync,
  fsyncSync,
  ftruncateSync,
  lchmodSync,
  lchownSync,
  linkSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readdirSync,
  readFileSync,
  readlinkSync,
  readSync,
  realpathSync,
  renameSync,
  rmdirSync,
  rmSync,
  statSync,
  symlinkSync,
  truncateSync,
  unlinkSync,
  utimesSync,
  writeFileSync,
  writeSync,
};

const callbacks = {};
const promises = {};
for (const key of Object.keys(sync)) {
  const name = key.slice(0, -4);
  if (name !== "exists" && name !== "read" && name !== "write") {
    callbacks[name] = callbackify(sync[key], name);
  }
  if (!["exists", "read", "write", "fsync", "fdatasync", "fstat", "ftruncate", "fchmod", "fchown", "close", "open"].includes(name)) {
    promises[name] = promisify(sync[key]);
  }
}
callbacks.realpath.native = callbacks.realpath;
promises.open = async (path, flags, mode) => new FileHandle(openSync(path, flags, mode));
promises.constants = constants;
promises.opendir = async (path) => openDir(path, pathString(path));

module.exports = {
  ...sync,
  ...callbacks,
  exists,
  read,
  write,
  constants,
  promises,
  Dir,
  Dirent,
  Stats,
  opendir,
  opendirSync,
  F_OK: 0,
  R_OK: 4,
  W_OK: 2,
  X_OK: 1,
  createReadStream,
  createWriteStream,
  watch: unsupported("watch"),
  watchFile: unsupported("watchFile"),
  unwatchFile: unsupported("unwatchFile"),
};

// node's lazy stream classes (T60); graceful-fs replaces them through the setters.
let FileReadStream;
let FileWriteStream;
for (const [name, get, set] of [
  ["ReadStream", () => (lazyLoadStreams(), ReadStream), (v) => (ReadStream = v)],
  ["WriteStream", () => (lazyLoadStreams(), WriteStream), (v) => (WriteStream = v)],
  ["FileReadStream", () => (lazyLoadStreams(), FileReadStream || ReadStream), (v) => (FileReadStream = v)],
  ["FileWriteStream", () => (lazyLoadStreams(), FileWriteStream || WriteStream), (v) => (FileWriteStream = v)],
]) {
  Object.defineProperty(module.exports, name, { get, set, enumerable: true, configurable: true });
}
