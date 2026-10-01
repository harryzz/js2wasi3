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
            "mkdir", "mkdtemp", "open", "readFile", "readdir", "readlink", "realpath", "rename", "rm", "rmdir", "stat",
            "symlink", "truncate", "unlink", "utimes", "writeFile",
        ]),
        "node:fs" => Some(&[
            "Dirent", "F_OK", "R_OK", "Stats", "W_OK", "X_OK", "access", "accessSync", "appendFile",
            "appendFileSync", "chmod", "chmodSync", "chown", "chownSync", "close", "closeSync", "constants",
            "copyFile", "copyFileSync", "createReadStream", "createWriteStream", "exists", "existsSync", "fchmod",
            "fchmodSync", "fchown", "fchownSync", "fdatasync", "fdatasyncSync", "fstat", "fstatSync", "fsync",
            "fsyncSync", "ftruncate", "ftruncateSync", "lchmod", "lchmodSync", "lchown", "lchownSync", "link",
            "linkSync", "lstat", "lstatSync", "mkdir", "mkdirSync", "mkdtemp", "mkdtempSync", "open", "openSync",
            "promises", "read", "readFile", "readFileSync", "readSync", "readdir", "readdirSync", "readlink",
            "readlinkSync", "realpath", "realpathSync", "rename", "renameSync", "rm", "rmSync", "rmdir",
            "rmdirSync", "stat", "statSync", "symlink", "symlinkSync", "truncate", "truncateSync", "unlink",
            "unlinkSync", "unwatchFile", "utimes", "utimesSync", "watch", "watchFile", "write", "writeFile",
            "writeFileSync", "writeSync",
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

/// Is this program path a built-in module's?
pub(crate) fn is_module(path: &str) -> bool {
    path.starts_with("node:")
}
