// node's built-in modules through `require` (T55): one module object per built-in, with or without
// `node:`; printed like node's; used by another CommonJS module.
const path = require("path");
const again = require("node:path");
console.log(path === again, require("path/posix") === path, require("node:path/posix") === path);
console.log(require.resolve("path"), require.resolve("node:path"));
console.log(path.join(__dirname, "..") === path.dirname(__dirname), path.basename(__filename));
console.log(path);
console.log(require("process") === process, require("events") === require("events").EventEmitter);
console.log(require("util/types") === require("util").types, require("assert/strict") === require("assert").strict);
const Counter = require("./emitter");
const c = new Counter();
c.on("tick", (n) => console.log("tick", n));
c.tick();
c.tick();
console.log(c instanceof require("events"), Counter.super_ === require("events"), c.listenerCount("tick"));
try {
  require("node:no-such-module");
} catch (e) {
  console.log(e.code, e.message);
}
