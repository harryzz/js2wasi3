// node's `path` (T55): named, default and namespace imports, `node:` and bare specifiers, `import()`.
import path, { join, basename, dirname, extname, relative, parse, format, normalize, resolve } from "node:path";
import * as ns from "path";
import posix from "path/posix";

console.log(ns.default === path, ns.join === join, posix === path, path.posix === path, Object.keys(ns));

// join / normalize
const joins = [["/a/b", "../c", "d.js"], ["a", "", "b/"], [""], ["/", ".."], ["a/./b", "../../.."], ["//x//y"]];
for (const args of joins) console.log(JSON.stringify(args), "→", join(...args));
for (const p of ["", ".", "./", "a//b/../c/", "/../x", "../../a/..", "a/b/../../..", "//"]) {
  console.log(JSON.stringify(p), "→", normalize(p));
}

// dirname / basename / extname
for (const p of ["/a/b/c", "/a/b/c/", "a", "/", "//a", "", "a/b"]) {
  console.log(JSON.stringify(p), dirname(p), basename(p), JSON.stringify(extname(p)));
}
console.log(basename("/x/y.txt", ".txt"), basename("/x/y.txt", "y.txt"), basename("a.js", "js"), basename("/aaa/", "a"));
for (const p of ["f.tar.gz", ".bashrc", "a.", "a..", "..", ".x.y", "dir.d/file"]) console.log(p, JSON.stringify(extname(p)));

// relative / resolve (absolute paths only: the working directory differs)
console.log(relative("/data/orandea/test/aaa", "/data/orandea/impl/bbb"), relative("/a", "/a/b/c"), relative("/a/b/c", "/a"));
console.log(JSON.stringify(relative("/x", "/x")), relative("/", "/y"), relative("/y", "/"));
console.log(resolve("/foo/bar", "./baz"), resolve("/foo/bar", "/tmp/file/"), resolve("/a", "b", "..", "c"));

// parse / format
for (const p of ["/home/user/dir/file.txt", "./x", "file", "/", ".hidden", "a/b.c.d"]) console.log(parse(p));
console.log(format({ dir: "/x", name: "f", ext: "js" }), format({ root: "/", base: "b.txt" }), format({ name: "n", ext: ".e" }));
console.log(path.sep, path.delimiter, path.isAbsolute("/a"), path.isAbsolute("a"), path.toNamespacedPath("a/b"));
console.log(path.format.name, typeof path.win32.join, path.win32.sep, path.win32.posix === path);

// node's errors
for (const f of [() => join("a", 1), () => basename({}), () => dirname(null), () => path.relative("a"), () => format("x"), () => basename("a", 3)]) {
  try {
    f();
  } catch (e) {
    console.log(e.name, e.code, e.message);
  }
}

const again = await import("node:path");
const bare = await import("path");
console.log(again === bare, again.default === path, again.join === join);
