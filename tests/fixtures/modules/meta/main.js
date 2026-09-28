// `import.meta` (T41 slice 3): one object per module with node's properties; the paths are the
// source file's at build time.
import { meta, where } from "./lib/dep.js";

const here = import.meta;
console.log(Object.keys(here), Object.getPrototypeOf(here), here.main, meta.main);
console.log(here === import.meta, meta === here, meta.dirname === here.dirname + "/lib");
console.log(here.filename.slice(here.dirname.length), here.url === "file://" + here.filename);
console.log(where(), meta.resolve("../main.js") === here.url);
console.log(here.resolve("./a b/ü.js").slice(here.url.length - "main.js".length));
console.log(typeof here.resolve, here.resolve.name, here.resolve.length);
try {
  here.resolve("some-package");
} catch (e) {
  console.log(e.code, e.message.slice(0, 32));
}
here.extra = 1;
console.log(import.meta.extra);
