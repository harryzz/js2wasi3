// node's `path` (T55): the POSIX half of node 24's lib/path.js (MIT, Node.js contributors), which
// is what `require("path")` gives on POSIX systems. `path.win32` isn't implemented (a stub that
// throws, F243). `process.cwd()` is the WASI initial directory.
"use strict";

function validateString(value, name) {
  if (typeof value !== "string") throw $$errInvalidArgType(name, "of type string", value);
}

function validateObject(value, name) {
  if (value === null || typeof value !== "object") throw $$errInvalidArgType(name, "of type object", value);
}

// Resolves . and .. elements in a path with directory names.
function normalizeString(path, allowAboveRoot) {
  let res = "";
  let lastSegmentLength = 0;
  let lastSlash = -1;
  let dots = 0;
  let code = 0;
  for (let i = 0; i <= path.length; ++i) {
    if (i < path.length) code = path.charCodeAt(i);
    else if (code === 47) break;
    else code = 47;
    if (code === 47) {
      if (lastSlash === i - 1 || dots === 1) {
        // NOOP
      } else if (dots === 2) {
        if (res.length < 2 || lastSegmentLength !== 2 || res.charCodeAt(res.length - 1) !== 46 || res.charCodeAt(res.length - 2) !== 46) {
          if (res.length > 2) {
            const lastSlashIndex = res.lastIndexOf("/");
            if (lastSlashIndex === -1) {
              res = "";
              lastSegmentLength = 0;
            } else {
              res = res.slice(0, lastSlashIndex);
              lastSegmentLength = res.length - 1 - res.lastIndexOf("/");
            }
            lastSlash = i;
            dots = 0;
            continue;
          } else if (res.length !== 0) {
            res = "";
            lastSegmentLength = 0;
            lastSlash = i;
            dots = 0;
            continue;
          }
        }
        if (allowAboveRoot) {
          res += res.length > 0 ? "/.." : "..";
          lastSegmentLength = 2;
        }
      } else {
        if (res.length > 0) res += "/" + path.slice(lastSlash + 1, i);
        else res = path.slice(lastSlash + 1, i);
        lastSegmentLength = i - lastSlash - 1;
      }
      lastSlash = i;
      dots = 0;
    } else if (code === 46 && dots !== -1) {
      ++dots;
    } else {
      dots = -1;
    }
  }
  return res;
}

function _format(sep, pathObject) {
  validateObject(pathObject, "pathObject");
  const dir = pathObject.dir || pathObject.root;
  const base = pathObject.base || `${pathObject.name || ""}${formatExt(pathObject.ext)}`;
  if (!dir) return base;
  return dir === pathObject.root ? `${dir}${base}` : `${dir}${sep}${base}`;
}

function formatExt(ext) {
  return ext ? `${ext[0] === "." ? "" : "."}${ext}` : "";
}

function resolve(...args) {
  let resolvedPath = "";
  let resolvedAbsolute = false;
  for (let i = args.length - 1; i >= 0 && !resolvedAbsolute; i--) {
    const path = args[i];
    validateString(path, `paths[${i}]`);
    if (path.length === 0) continue;
    resolvedPath = `${path}/${resolvedPath}`;
    resolvedAbsolute = path.charCodeAt(0) === 47;
  }
  if (!resolvedAbsolute) {
    const cwd = $$processCwd();
    resolvedPath = `${cwd}/${resolvedPath}`;
    resolvedAbsolute = cwd.charCodeAt(0) === 47;
  }
  resolvedPath = normalizeString(resolvedPath, !resolvedAbsolute);
  if (resolvedAbsolute) return `/${resolvedPath}`;
  return resolvedPath.length > 0 ? resolvedPath : ".";
}

function normalize(path) {
  validateString(path, "path");
  if (path.length === 0) return ".";
  const isAbsolute = path.charCodeAt(0) === 47;
  const trailingSeparator = path.charCodeAt(path.length - 1) === 47;
  path = normalizeString(path, !isAbsolute);
  if (path.length === 0) {
    if (isAbsolute) return "/";
    return trailingSeparator ? "./" : ".";
  }
  if (trailingSeparator) path += "/";
  return isAbsolute ? `/${path}` : path;
}

function isAbsolute(path) {
  validateString(path, "path");
  return path.length > 0 && path.charCodeAt(0) === 47;
}

function join(...args) {
  if (args.length === 0) return ".";
  const path = [];
  for (let i = 0; i < args.length; ++i) {
    const arg = args[i];
    validateString(arg, "path");
    if (arg.length > 0) path.push(arg);
  }
  if (path.length === 0) return ".";
  return normalize(path.join("/"));
}

function relative(from, to) {
  validateString(from, "from");
  validateString(to, "to");
  if (from === to) return "";
  from = resolve(from);
  to = resolve(to);
  if (from === to) return "";
  const fromStart = 1;
  const fromEnd = from.length;
  const fromLen = fromEnd - fromStart;
  const toStart = 1;
  const toLen = to.length - toStart;
  const length = fromLen < toLen ? fromLen : toLen;
  let lastCommonSep = -1;
  let i = 0;
  for (; i < length; i++) {
    const fromCode = from.charCodeAt(fromStart + i);
    if (fromCode !== to.charCodeAt(toStart + i)) break;
    else if (fromCode === 47) lastCommonSep = i;
  }
  if (i === length) {
    if (toLen > length) {
      if (to.charCodeAt(toStart + i) === 47) {
        // `from` is the exact base path for `to`: /foo/bar → /foo/bar/baz
        return to.slice(toStart + i + 1);
      }
      if (i === 0) {
        // `from` is the root: / → /foo
        return to.slice(toStart + i);
      }
    } else if (fromLen > length) {
      if (from.charCodeAt(fromStart + i) === 47) {
        // `to` is the exact base path for `from`: /foo/bar/baz → /foo/bar
        lastCommonSep = i;
      } else if (i === 0) {
        // `to` is the root: /foo → /
        lastCommonSep = 0;
      }
    }
  }
  let out = "";
  for (i = fromStart + lastCommonSep + 1; i <= fromEnd; ++i) {
    if (i === fromEnd || from.charCodeAt(i) === 47) {
      out += out.length === 0 ? ".." : "/..";
    }
  }
  return `${out}${to.slice(toStart + lastCommonSep)}`;
}

function toNamespacedPath(path) {
  // Non-op on POSIX systems.
  return path;
}

function dirname(path) {
  validateString(path, "path");
  if (path.length === 0) return ".";
  const hasRoot = path.charCodeAt(0) === 47;
  let end = -1;
  let matchedSlash = true;
  for (let i = path.length - 1; i >= 1; --i) {
    if (path.charCodeAt(i) === 47) {
      if (!matchedSlash) {
        end = i;
        break;
      }
    } else {
      // We saw the first non-path separator.
      matchedSlash = false;
    }
  }
  if (end === -1) return hasRoot ? "/" : ".";
  if (hasRoot && end === 1) return "//";
  return path.slice(0, end);
}

function basename(path, suffix) {
  if (suffix !== undefined) validateString(suffix, "suffix");
  validateString(path, "path");
  let start = 0;
  let end = -1;
  let matchedSlash = true;
  if (suffix !== undefined && suffix.length > 0 && suffix.length <= path.length) {
    if (suffix === path) return "";
    let extIdx = suffix.length - 1;
    let firstNonSlashEnd = -1;
    for (let i = path.length - 1; i >= 0; --i) {
      const code = path.charCodeAt(i);
      if (code === 47) {
        if (!matchedSlash) {
          start = i + 1;
          break;
        }
      } else {
        if (firstNonSlashEnd === -1) {
          matchedSlash = false;
          firstNonSlashEnd = i + 1;
        }
        if (extIdx >= 0) {
          // Try to match the explicit extension.
          if (code === suffix.charCodeAt(extIdx)) {
            if (--extIdx === -1) {
              end = i;
            }
          } else {
            extIdx = -1;
            end = firstNonSlashEnd;
          }
        }
      }
    }
    if (start === end) end = firstNonSlashEnd;
    else if (end === -1) end = path.length;
    return path.slice(start, end);
  }
  for (let i = path.length - 1; i >= 0; --i) {
    if (path.charCodeAt(i) === 47) {
      if (!matchedSlash) {
        start = i + 1;
        break;
      }
    } else if (end === -1) {
      matchedSlash = false;
      end = i + 1;
    }
  }
  if (end === -1) return "";
  return path.slice(start, end);
}

function extname(path) {
  validateString(path, "path");
  let startDot = -1;
  let startPart = 0;
  let end = -1;
  let matchedSlash = true;
  let preDotState = 0;
  for (let i = path.length - 1; i >= 0; --i) {
    const code = path.charCodeAt(i);
    if (code === 47) {
      if (!matchedSlash) {
        startPart = i + 1;
        break;
      }
      continue;
    }
    if (end === -1) {
      matchedSlash = false;
      end = i + 1;
    }
    if (code === 46) {
      if (startDot === -1) startDot = i;
      else if (preDotState !== 1) preDotState = 1;
    } else if (startDot !== -1) {
      preDotState = -1;
    }
  }
  if (startDot === -1 || end === -1 || preDotState === 0 || (preDotState === 1 && startDot === end - 1 && startDot === startPart + 1)) {
    return "";
  }
  return path.slice(startDot, end);
}

function parse(path) {
  validateString(path, "path");
  const ret = { root: "", dir: "", base: "", ext: "", name: "" };
  if (path.length === 0) return ret;
  const isAbsolute = path.charCodeAt(0) === 47;
  let start;
  if (isAbsolute) {
    ret.root = "/";
    start = 1;
  } else {
    start = 0;
  }
  let startDot = -1;
  let startPart = 0;
  let end = -1;
  let matchedSlash = true;
  let i = path.length - 1;
  let preDotState = 0;
  for (; i >= start; --i) {
    const code = path.charCodeAt(i);
    if (code === 47) {
      if (!matchedSlash) {
        startPart = i + 1;
        break;
      }
      continue;
    }
    if (end === -1) {
      matchedSlash = false;
      end = i + 1;
    }
    if (code === 46) {
      if (startDot === -1) startDot = i;
      else if (preDotState !== 1) preDotState = 1;
    } else if (startDot !== -1) {
      preDotState = -1;
    }
  }
  if (end !== -1) {
    const start = startPart === 0 && isAbsolute ? 1 : startPart;
    if (startDot === -1 || preDotState === 0 || (preDotState === 1 && startDot === end - 1 && startDot === startPart + 1)) {
      ret.base = ret.name = path.slice(start, end);
    } else {
      ret.name = path.slice(start, startDot);
      ret.base = path.slice(start, end);
      ret.ext = path.slice(startDot, end);
    }
  }
  if (startPart > 0) ret.dir = path.slice(0, startPart - 1);
  else if (isAbsolute) ret.dir = "/";
  return ret;
}

// `path.win32` isn't implemented: its functions throw (F243); its shape is node's.
function win32Stub(name) {
  return function () {
    throw new Error("path.win32." + name + " isn't implemented in js2wasi3 (planned: F243)");
  };
}
const win32 = {
  resolve: win32Stub("resolve"),
  normalize: win32Stub("normalize"),
  isAbsolute: win32Stub("isAbsolute"),
  join: win32Stub("join"),
  relative: win32Stub("relative"),
  toNamespacedPath: win32Stub("toNamespacedPath"),
  dirname: win32Stub("dirname"),
  basename: win32Stub("basename"),
  extname: win32Stub("extname"),
  format: win32Stub("format"),
  parse: win32Stub("parse"),
  matchesGlob: win32Stub("matchesGlob"),
  sep: "\\",
  delimiter: ";",
  win32: null,
  posix: exports,
  _makeLong: null,
};
const win32Names = ["resolve", "normalize", "isAbsolute", "join", "relative", "toNamespacedPath", "dirname", "basename", "extname", "matchesGlob", "parse"];
for (let i = 0; i < win32Names.length; i++) {
  Object.defineProperty(win32[win32Names[i]], "name", { value: win32Names[i] });
}
Object.defineProperty(win32.format, "name", { value: "bound _format" });
win32.win32 = win32;
win32._makeLong = win32.toNamespacedPath;

function matchesGlob(path, pattern) {
  throw new Error("path.matchesGlob isn't implemented in js2wasi3 (planned: F243)");
}

exports.resolve = resolve;
exports.normalize = normalize;
exports.isAbsolute = isAbsolute;
exports.join = join;
exports.relative = relative;
exports.toNamespacedPath = toNamespacedPath;
exports.dirname = dirname;
exports.basename = basename;
exports.extname = extname;
exports.format = _format.bind(null, "/");
exports.parse = parse;
exports.matchesGlob = matchesGlob;
exports.sep = "/";
exports.delimiter = ":";
exports.win32 = win32;
exports.posix = exports;
exports._makeLong = toNamespacedPath;
