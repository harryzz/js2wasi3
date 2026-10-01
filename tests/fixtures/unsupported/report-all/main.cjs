// T62: every module's errors in one build (a module that fails to parse doesn't stop the link).
const dyn = require("./dyn.cjs");
const ev = require("./eval.cjs");
console.log(dyn, ev);
