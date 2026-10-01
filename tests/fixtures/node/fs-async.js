// T59: `fs`'s callbacks and `fs/promises` (diff.sh copies `fs-async.fs/` and runs in the copy).
import fs from "node:fs";
import { readFile, writeFile, mkdir, readdir, rm, open, stat } from "node:fs/promises";

// (node's order between callbacks and other I/O depends on its thread pool: one part at a time.)
await new Promise((done) => fs.readFile("hello.txt", "utf8", (err, data) => {
  console.log("cb readFile", err, data);
  fs.writeFile("cb.txt", "from a callback", (err) => {
    console.log("cb writeFile", err, fs.readFileSync("cb.txt", "utf8"));
    fs.stat("missing", (err, st) => {
      console.log("cb stat", err.code, err.message, st);
      fs.exists("cb.txt", (yes) => {
        console.log("cb exists", yes);
        done();
      });
    });
  });
}));

console.log(fs.promises === (await import("node:fs/promises")).default);
console.log(await readFile("hello.txt", "utf8"));
await writeFile("p.txt", "via promises");
console.log(await readFile("p.txt", { encoding: "utf8" }));
await mkdir("d/e", { recursive: true });
console.log((await readdir(".")).sort(), (await stat("d")).isDirectory());
try {
  await readFile("missing.txt");
} catch (e) {
  console.log("rejected", e.code, e.syscall, e.path);
}
const h = await open("p.txt", "r+");
console.log(typeof h.fd, (await h.readFile("utf8")), (await h.stat()).size);
await h.close();
await rm("d", { recursive: true });
console.log((await readdir(".")).sort());
try {
  fs.readFile("hello.txt");
} catch (e) {
  console.log(e.code);
}
