// Functions and classes of node built-ins whose facility js2wasi3 lacks (T65): they exist, so modules that
// name them load, and throw when called or constructed.
"use strict";

function unavailableError(what, why) {
  return new Error(`${what} isn't available in js2wasi3 (${why})`);
}

// A function `name` of `module` that throws `<module>.<name>() isn't available …`.
function fn(module, name, why) {
  const f = function () {
    throw unavailableError(`${module}.${name}()`, why);
  };
  Object.defineProperty(f, "name", { value: name });
  return f;
}

// Each of `names` as such a function on `target`.
function fns(target, module, names, why) {
  for (const name of names) target[name] = fn(module, name, why);
}

module.exports = { unavailableError, fn, fns };
