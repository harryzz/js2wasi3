// fs.opendir / opendirSync / promises.opendir and fs.Dir (T60; fs-extra's copySync uses them).
import fs from "node:fs";
import { opendirSync } from "node:fs";

const d = opendirSync(".");
console.log(d.path, Object.keys(d), d instanceof fs.Dir);
const names = [];
let e;
while ((e = d.readSync()) !== null) names.push([e.name, e.isFile(), e.isDirectory(), e.parentPath]);
console.log(names.sort());
d.closeSync();
for (const f of [() => d.closeSync(), () => d.readSync()]) {
  try {
    f();
  } catch (err) {
    console.log(err.name, err.code, err.message);
  }
}
try {
  fs.opendirSync("nope");
} catch (err) {
  console.log(err.code, err.syscall, err.path, err.message);
}
const seen = [];
for await (const ent of await fs.promises.opendir("sub")) seen.push(ent.name);
console.log("async", seen);
fs.opendir("sub", (err, dir) => {
  dir.read((err2, ent) => {
    console.log("cb", err, err2, ent.name);
    dir.read().then((next) => {
      console.log("next", next);
      dir.close(() => console.log("closed"));
    });
  });
});
fs.opendir("nope", (err) => console.log("cb error", err.code, err.path));
