//! node's built-in modules (T55, `docs/design.md` D4): each one js2wasi3 implements is a CommonJS
//! module shipped with the compiler (`crates/frontend/node/<name>.js`), which the linker loads like a
//! file at the path `node:<name>` for both `x` and `node:x`, so `require`, `import`, `import()` and
//! ESM ⇄ CommonJS interop work as for any file, and `require("path") === require("node:path")`.
//! Their code may use the prelude (`$$…`) and its intrinsics, like the linker's generated code.

/// The implemented built-ins and their sources.
const MODULES: &[(&str, &str)] = &[
    ("path", include_str!("../node/path.js")),
    ("path/posix", "module.exports = require(\"node:path\");\n"),
    ("process", include_str!("../node/process.js")),
    ("events", include_str!("../node/events.js")),
    ("util", include_str!("../node/util.js")),
    ("util/types", "module.exports = require(\"node:util\").types;\n"),
    ("os", include_str!("../node/os.js")),
    ("assert", include_str!("../node/assert.js")),
    ("assert/strict", "module.exports = require(\"node:assert\").strict;\n"),
    ("tty", include_str!("../node/tty.js")),
    ("buffer", include_str!("../node/buffer.js")),
    ("string_decoder", include_str!("../node/string_decoder.js")),
    ("fs", include_str!("../node/fs.js")),
    ("constants", include_str!("../node/constants.js")),
    ("fs/promises", "module.exports = require(\"node:fs\").promises;\n"),
    // `fs`'s `wasi:filesystem` imports (T59): an ES module (a CommonJS file can't `import`).
    ("internal/wasi_fs", include_str!("../node/wasi_fs.mjs")),
    // `stream` (T60): node 24's lib (`scripts/gen-node-streams.mjs`) over support modules partly from
    // readable-stream 4.7.0 (`errors`, `primordials`, `util`, `util/inspect`, `validators`).
    ("stream", include_str!("../node/stream/stream.js")),
    ("stream/promises", include_str!("../node/stream/promises.js")),
    ("_stream_duplex", include_str!("../node/stream/_stream_duplex.js")),
    ("_stream_passthrough", include_str!("../node/stream/_stream_passthrough.js")),
    ("_stream_readable", include_str!("../node/stream/_stream_readable.js")),
    ("_stream_transform", include_str!("../node/stream/_stream_transform.js")),
    ("_stream_writable", include_str!("../node/stream/_stream_writable.js")),
    ("internal/abort", include_str!("../node/abort.js")),
    // `WeakMap`, `WeakSet` (T61): node's globals, bound by the linker (`GLOBALS`).
    ("internal/weak", include_str!("../node/weak.js")),
    ("internal/fs/streams", include_str!("../node/stream/fs_streams.js")),
    ("internal/fs_support", include_str!("../node/fs_support.js")),
    ("internal/errors", include_str!("../node/stream/errors.js")),
    ("internal/primordials", include_str!("../node/stream/primordials.js")),
    ("internal/streams/add-abort-signal", include_str!("../node/stream/streams/add-abort-signal.js")),
    ("internal/streams/compose", include_str!("../node/stream/streams/compose.js")),
    ("internal/streams/destroy", include_str!("../node/stream/streams/destroy.js")),
    ("internal/streams/duplex", include_str!("../node/stream/streams/duplex.js")),
    ("internal/streams/duplexpair", include_str!("../node/stream/streams/duplexpair.js")),
    ("internal/streams/duplexify", include_str!("../node/stream/streams/duplexify.js")),
    ("internal/streams/end-of-stream", include_str!("../node/stream/streams/end-of-stream.js")),
    ("internal/streams/from", include_str!("../node/stream/streams/from.js")),
    ("internal/streams/legacy", include_str!("../node/stream/streams/legacy.js")),
    ("internal/streams/operators", include_str!("../node/stream/streams/operators.js")),
    ("internal/streams/passthrough", include_str!("../node/stream/streams/passthrough.js")),
    ("internal/streams/pipeline", include_str!("../node/stream/streams/pipeline.js")),
    ("internal/streams/readable", include_str!("../node/stream/streams/readable.js")),
    ("internal/streams/state", include_str!("../node/stream/streams/state.js")),
    ("internal/streams/transform", include_str!("../node/stream/streams/transform.js")),
    ("internal/streams/utils", include_str!("../node/stream/streams/utils.js")),
    ("internal/streams/writable", include_str!("../node/stream/streams/writable.js")),
    ("internal/util", include_str!("../node/stream/util.js")),
    ("internal/util/inspect", include_str!("../node/stream/util/inspect.js")),
    ("internal/validators", include_str!("../node/stream/validators.js")),
];

/// node's globals that are a built-in module's export, bound by the linker where a module's code
/// names them without declaring them (T56: `Buffer`; T61: the others): (global, module, export).
/// A CommonJS module gets `const X = require(module).export;` first in its wrapper, an ES module's
/// references become the export (an import); programs that don't name one don't load its module.
pub(crate) const GLOBALS: &[(&str, &str, &str)] = &[
    ("Buffer", "node:buffer", "Buffer"),
    ("WeakMap", "node:internal/weak", "WeakMap"),
    ("WeakSet", "node:internal/weak", "WeakSet"),
    ("AbortController", "node:internal/abort", "AbortController"),
    ("AbortSignal", "node:internal/abort", "AbortSignal"),
    ("EventTarget", "node:internal/abort", "EventTarget"),
    ("Event", "node:internal/abort", "Event"),
];

/// The program path of the built-in `spec` names (`path`, `node:path` → `node:path`), if
/// js2wasi3 implements it.
pub(crate) fn path(spec: &str) -> Option<String> {
    let name = spec.strip_prefix("node:").unwrap_or(spec);
    MODULES.iter().any(|(n, _)| *n == name).then(|| format!("node:{name}"))
}

/// The source of a built-in's program path (`node:path`).
pub(crate) fn source(path: &str) -> Option<&'static str> {
    let name = path.strip_prefix("node:")?;
    MODULES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// The named exports of a built-in whose `module.exports` the lexer can't see (node: every own
/// property of the module object), else `None` (the lexer's names, as for any CommonJS file).
pub(crate) fn exports(path: &str) -> Option<&'static [&'static str]> {
    match path {
        // What `builtins/process.js` (`$$Process`) defines.
        "node:process" => Some(&[
            "addListener", "arch", "argv", "argv0", "chdir", "cwd", "emit", "emitWarning", "env", "execArgv",
            "execPath", "exit", "exitCode", "hrtime", "listenerCount", "listeners", "nextTick", "off", "on", "once",
            "platform", "release", "removeAllListeners", "removeListener", "stderr", "stdout", "title", "uptime",
            "version", "versions",
        ]),
        // What `node/fs.js` exports (its `module.exports` has non-identifier values).
        "node:fs/promises" => Some(&[
            "access", "appendFile", "chmod", "chown", "constants", "copyFile", "lchmod", "lchown", "link", "lstat",
            "mkdir", "mkdtemp", "open", "opendir", "readFile", "readdir", "readlink", "realpath", "rename", "rm",
            "rmdir", "stat", "symlink", "truncate", "unlink", "utimes", "writeFile",
        ]),
        "node:fs" => Some(&[
            "Dir", "Dirent", "F_OK", "FileReadStream", "FileWriteStream", "R_OK", "ReadStream", "Stats", "W_OK",
            "WriteStream", "X_OK", "access", "accessSync", "appendFile", "appendFileSync", "chmod", "chmodSync",
            "chown", "chownSync", "close", "closeSync", "constants", "copyFile", "copyFileSync", "createReadStream",
            "createWriteStream", "exists", "existsSync", "fchmod", "fchmodSync", "fchown", "fchownSync", "fdatasync",
            "fdatasyncSync", "fstat", "fstatSync", "fsync", "fsyncSync", "ftruncate", "ftruncateSync", "lchmod",
            "lchmodSync", "lchown", "lchownSync", "link", "linkSync", "lstat", "lstatSync", "mkdir", "mkdirSync",
            "mkdtemp", "mkdtempSync", "open", "openSync", "opendir", "opendirSync", "promises", "read", "readFile",
            "readFileSync", "readSync", "readdir", "readdirSync", "readlink", "readlinkSync", "realpath",
            "realpathSync", "rename", "renameSync", "rm", "rmSync", "rmdir", "rmdirSync", "stat", "statSync", "symlink",
            "symlinkSync", "truncate", "truncateSync", "unlink", "unlinkSync", "unwatchFile", "utimes", "utimesSync",
            "watch", "watchFile", "write", "writeFile", "writeFileSync", "writeSync",
        ]),
        // What `node/stream/stream.js` (node 24's lib/stream.js) assigns to `Stream` (T60).
        "node:stream" => Some(&[
            "Duplex", "PassThrough", "Readable", "Stream", "Transform", "Writable", "_isArrayBufferView",
            "_isUint8Array", "_uint8ArrayToBuffer", "addAbortSignal", "compose", "destroy", "duplexPair", "finished",
            "getDefaultHighWaterMark", "isDestroyed", "isDisturbed", "isErrored", "isReadable", "isWritable",
            "pipeline", "promises", "setDefaultHighWaterMark",
        ]),
        "node:events" => Some(&[
            "EventEmitter", "EventEmitterAsyncResource", "addAbortListener", "captureRejectionSymbol",
            "captureRejections", "defaultMaxListeners", "errorMonitor", "getEventListeners", "getMaxListeners", "init",
            "listenerCount", "on", "once", "setMaxListeners", "usingDomains",
        ]),
        "node:util" => Some(&[
            "_errnoException", "_exceptionWithHostPort", "_extend", "MIMEParams", "MIMEType", "TextDecoder",
            "TextEncoder", "aborted", "callbackify", "convertProcessSignalToExitCode", "debug", "debuglog",
            "deprecate", "diff", "format", "formatWithOptions", "getCallSites", "getSystemErrorMap",
            "getSystemErrorMessage", "getSystemErrorName", "inherits", "inspect", "isArray", "isDeepStrictEqual",
            "parseArgs", "parseEnv", "promisify", "setTraceSigInt", "stripVTControlCharacters", "styleText",
            "toUSVString", "transferableAbortController", "transferableAbortSignal", "types",
        ]),
        "node:util/types" => Some(&[
            "isAnyArrayBuffer", "isArgumentsObject", "isArrayBuffer", "isArrayBufferView", "isAsyncFunction",
            "isBigInt64Array", "isBigIntObject", "isBigUint64Array", "isBooleanObject", "isBoxedPrimitive",
            "isCryptoKey", "isDataView", "isDate", "isExternal", "isFloat16Array", "isFloat32Array", "isFloat64Array",
            "isGeneratorFunction", "isGeneratorObject", "isInt16Array", "isInt32Array", "isInt8Array", "isKeyObject",
            "isMap", "isMapIterator", "isModuleNamespaceObject", "isNativeError", "isNumberObject", "isPromise",
            "isProxy", "isRegExp", "isSet", "isSetIterator", "isSharedArrayBuffer", "isStringObject",
            "isSymbolObject", "isTypedArray", "isUint16Array", "isUint32Array", "isUint8Array",
            "isUint8ClampedArray", "isWeakMap", "isWeakSet",
        ]),
        "node:os" => Some(&[
            "EOL", "arch", "availableParallelism", "constants", "cpus", "devNull", "endianness", "freemem",
            "getPriority", "homedir", "hostname", "loadavg", "machine", "networkInterfaces", "platform", "release",
            "setPriority", "tmpdir", "totalmem", "type", "uptime", "userInfo", "version",
        ]),
        "node:assert" | "node:assert/strict" => Some(&[
            "Assert", "AssertionError", "CallTracker", "deepEqual", "deepStrictEqual", "doesNotMatch",
            "doesNotReject", "doesNotThrow", "equal", "fail", "ifError", "match", "notDeepEqual",
            "notDeepStrictEqual", "notEqual", "notStrictEqual", "ok", "partialDeepStrictEqual", "rejects", "strict",
            "strictEqual", "throws",
        ]),
        _ => None,
    }
}

/// Is this built-in an ES module? Only js2wasi3's own that import WASI interfaces (T59); the others,
/// `node:internal/…` ones included, are CommonJS.
pub(crate) fn is_esm(path: &str) -> bool {
    path == "node:internal/wasi_fs"
}

/// Is this program path a built-in module's?
pub(crate) fn is_module(path: &str) -> bool {
    path.starts_with("node:")
}
