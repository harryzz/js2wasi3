// node's deprecated `constants` module (T59, for `graceful-fs`): `os.constants`' groups and
// `fs.constants`, flattened (node also adds `crypto`'s, which js2wasi3 lacks).
"use strict";
const os = require("node:os").constants;
module.exports = {
  ...os.dlopen,
  ...os.errno,
  ...os.signals,
  ...os.priority,
  ...require("node:fs").constants,
};
