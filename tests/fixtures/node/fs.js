// T59: node's `fs` sync API over `wasi:filesystem` (diff.sh copies `fs.fs/` and runs in the copy).
import fs from "node:fs";
import { Buffer } from "node:buffer";

const show = (f) => {
  try {
    const r = f();
    console.log(r);
  } catch (e) {
    console.log(e.message, e.code, e.errno, e.syscall, e.path, e.dest);
  }
};

// Whole files.
console.log(fs.readFileSync("hello.txt"));
console.log(fs.readFileSync("hello.txt", "utf8"));
console.log(fs.readFileSync("hello.txt", { encoding: "base64" }));
fs.writeFileSync("out.txt", "line 1\n");
fs.appendFileSync("out.txt", "line 2\n");
fs.writeFileSync("bin.dat", Buffer.from([1, 2, 3, 250]));
console.log(fs.readFileSync("out.txt", "utf8"), fs.readFileSync("bin.dat"));
fs.writeFileSync("out.txt", "x", { flag: "a" });
console.log(JSON.stringify(fs.readFileSync("out.txt", "latin1")));
show(() => fs.writeFileSync("out.txt", "y", { flag: "wx" }));
show(() => fs.readFileSync("missing.txt"));
show(() => fs.readFileSync("sub/missing/deeper.txt"));
fs.copyFileSync("hello.txt", "copy.txt");
console.log(fs.readFileSync("copy.txt", "utf8"));
show(() => fs.copyFileSync("hello.txt", "copy.txt", fs.constants.COPYFILE_EXCL));

// Metadata.
const st = fs.statSync("hello.txt");
console.log(st.isFile(), st.isDirectory(), st.size, st.mode.toString(8), st.nlink, st.mtime instanceof Date);
console.log(fs.statSync("sub").isDirectory(), fs.statSync("sub").mode.toString(8));
console.log(fs.statSync("missing", { throwIfNoEntry: false }));
show(() => fs.statSync("missing"));
console.log(fs.existsSync("hello.txt"), fs.existsSync("nope"), fs.existsSync("sub"));
show(() => fs.accessSync("nope"));

// Directories.
console.log(fs.readdirSync(".").sort());
console.log(
  fs
    .readdirSync(".", { withFileTypes: true })
    .map((d) => `${d.name}:${d.isFile() ? "f" : d.isDirectory() ? "d" : "?"}`)
    .sort(),
);
console.log(fs.readdirSync("sub", { withFileTypes: true }));
show(() => fs.readdirSync("hello.txt"));
show(() => fs.readdirSync("nope"));
show(() => fs.mkdirSync("sub"));
console.log(fs.mkdirSync("x/y/z", { recursive: true }), fs.mkdirSync("x/y/z", { recursive: true }));
fs.writeFileSync("x/y/z/f.txt", "deep");
console.log(fs.readdirSync("x", { recursive: true }).sort());
show(() => fs.rmdirSync("x"));
show(() => fs.rmSync("x"));
fs.rmSync("x", { recursive: true });
fs.rmSync("x", { recursive: true, force: true });
console.log(fs.existsSync("x"));
show(() => fs.rmSync("x"));

// Renames, links.
fs.renameSync("copy.txt", "moved.txt");
console.log(fs.existsSync("copy.txt"), fs.readFileSync("moved.txt", "utf8"));
show(() => fs.renameSync("nope", "b"));
fs.symlinkSync("hello.txt", "link.txt");
console.log(fs.readlinkSync("link.txt"), fs.lstatSync("link.txt").isSymbolicLink(), fs.statSync("link.txt").isFile());
console.log(fs.readFileSync("link.txt", "utf8"));
console.log(fs.realpathSync("link.txt") === fs.realpathSync("hello.txt"));
fs.unlinkSync("link.txt");
fs.unlinkSync("moved.txt");
show(() => fs.unlinkSync("moved.txt"));

// File descriptors.
const fd = fs.openSync("fd.txt", "w+");
console.log(typeof fd, fs.writeSync(fd, "hello "), fs.writeSync(fd, Buffer.from("world")));
const buf = Buffer.alloc(5);
console.log(fs.readSync(fd, buf, 0, 5, 6), buf.toString());
console.log(fs.fstatSync(fd).size);
fs.ftruncateSync(fd, 5);
fs.fsyncSync(fd);
fs.closeSync(fd);
console.log(fs.readFileSync("fd.txt", "utf8"));
show(() => fs.closeSync(fd));
const r = fs.openSync("fd.txt");
const b2 = Buffer.alloc(3);
console.log(fs.readSync(r, b2), b2.toString(), fs.readSync(r, b2), b2.toString(), fs.readSync(r, b2));
fs.closeSync(r);
fs.truncateSync("fd.txt", 2);
console.log(fs.readFileSync("fd.txt", "utf8"));
show(() => fs.openSync("nope.txt", "r"));
console.log(fs.readdirSync(".").sort());
