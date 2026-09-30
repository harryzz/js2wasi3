// node's `assert` (T55): the assertions and `AssertionError` (name, `code`, `actual`, `expected`,
// `operator`, `generatedMessage`). Messages follow node 24's lib/assert.js and
// lib/internal/assert/assertion_error.js for primitives; node's source excerpts (`assert(x)`
// without a message quotes the call) and object diffs are approximated (F242).
// `CallTracker`, `Assert` and `partialDeepStrictEqual` throw.
"use strict";

const { isDeepStrictEqual } = require("node:util");

function inspect(v) {
  return __intrinsic_inspect(v, 0);
}

class AssertionError extends Error {
  constructor(options) {
    if (options === null || typeof options !== "object") throw $$errInvalidArgType("options", "of type object", options);
    const { message, operator, actual, expected } = options;
    if (message != null) {
      super(String(message));
    } else {
      super(generate(operator, actual, expected));
    }
    this.generatedMessage = message == null;
    Object.defineProperty(this, "name", {
      value: "AssertionError",
      enumerable: false,
      writable: true,
      configurable: true,
    });
    this.code = "ERR_ASSERTION";
    this.actual = actual;
    this.expected = expected;
    this.operator = operator;
  }

  toString() {
    return `${this.name} [${this.code}]: ${this.message}`;
  }
}

function comparison(operator, actual, expected) {
  const a = inspect(actual);
  const b = inspect(expected);
  if (!a.includes("\n") && !b.includes("\n")) return `${a} ${operator} ${b}\n`;
  return `+ actual - expected\n\n+ ${a.split("\n").join("\n+ ")}\n- ${b.split("\n").join("\n- ")}\n`;
}

function generate(operator, actual, expected) {
  switch (operator) {
    case "strictEqual":
      return "Expected values to be strictly equal:\n\n" + comparison("!==", actual, expected);
    case "deepStrictEqual":
      return "Expected values to be strictly deep-equal:\n" + comparison("!==", actual, expected);
    case "deepEqual":
      return "Expected values to be loosely deep-equal:\n\n" + inspect(actual) + "\n\nshould loosely deep-equal\n\n" + inspect(expected);
    case "notStrictEqual":
      return 'Expected "actual" to be strictly unequal to: ' + inspect(expected);
    case "notDeepStrictEqual":
      return 'Expected "actual" not to be strictly deep-equal to: ' + inspect(expected);
    case "notDeepEqual":
      return '"actual" deepEqual "expected"';
    case "fail":
      return "Failed";
    default:
      return `${inspect(actual)} ${operator} ${inspect(expected)}`;
  }
}

function innerFail(obj) {
  if (obj.message instanceof Error) throw obj.message;
  throw new AssertionError(obj);
}

function innerOk(fn, argLen, value, message) {
  if (!value) {
    let generatedMessage = false;
    if (argLen === 0) {
      generatedMessage = true;
      message = "No value argument passed to `assert.ok()`";
    } else if (message == null) {
      generatedMessage = true;
      message = "The expression evaluated to a falsy value:\n\n  assert.ok(" + inspect(value) + ")\n";
    } else if (message instanceof Error) {
      throw message;
    }
    const err = new AssertionError({ actual: value, expected: true, message, operator: "==" });
    err.generatedMessage = generatedMessage;
    throw err;
  }
}

function ok(...args) {
  innerOk(ok, args.length, ...args);
}

const assert = ok;
module.exports = assert;
assert.AssertionError = AssertionError;

function notImplemented(what) {
  return function () {
    throw new Error("assert." + what + " isn't implemented in js2wasi3 (planned: F242)");
  };
}
assert.CallTracker = notImplemented("CallTracker");
assert.ok = ok;

assert.fail = function fail(message) {
  if (message instanceof Error) throw message;
  const err = new AssertionError({ message, operator: "fail" });
  err.generatedMessage = message === undefined;
  throw err;
};

assert.equal = function equal(actual, expected, message) {
  if (arguments.length < 2) throw missingArgs();
  // eslint-disable-next-line eqeqeq
  if (actual != expected && (actual === actual || expected === expected)) {
    innerFail({ actual, expected, message, operator: "==" });
  }
};

assert.notEqual = function notEqual(actual, expected, message) {
  if (arguments.length < 2) throw missingArgs();
  // eslint-disable-next-line eqeqeq
  if (actual == expected || (actual !== actual && expected !== expected)) {
    innerFail({ actual, expected, message, operator: "!=" });
  }
};

function missingArgs() {
  const e = new TypeError('The "actual" and "expected" arguments must be specified');
  e.code = "ERR_MISSING_ARGS";
  return e;
}

// Loose deep equality (node's `deepEqual`): `==` for primitives, prototypes ignored.
function looseEqual(a, b, memo) {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
    // eslint-disable-next-line eqeqeq
    return (typeof a !== "object" || a === null) && (typeof b !== "object" || b === null) && (a == b || (a !== a && b !== b));
  }
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  for (let i = 0; i < memo.length; i++) {
    if (memo[i][0] === a && memo[i][1] === b) return true;
  }
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  if (ka.length !== kb.length) return false;
  memo.push([a, b]);
  try {
    for (let i = 0; i < ka.length; i++) {
      if (!Object.prototype.hasOwnProperty.call(b, ka[i])) return false;
      if (!looseEqual(a[ka[i]], b[ka[i]], memo)) return false;
    }
  } finally {
    memo.pop();
  }
  return true;
}

assert.deepEqual = function deepEqual(actual, expected, message) {
  if (arguments.length < 2) throw missingArgs();
  if (!looseEqual(actual, expected, [])) innerFail({ actual, expected, message, operator: "deepEqual" });
};

assert.notDeepEqual = function notDeepEqual(actual, expected, message) {
  if (arguments.length < 2) throw missingArgs();
  if (looseEqual(actual, expected, [])) innerFail({ actual, expected, message, operator: "notDeepEqual" });
};

assert.deepStrictEqual = function deepStrictEqual(actual, expected, message) {
  if (arguments.length < 2) throw missingArgs();
  if (!isDeepStrictEqual(actual, expected)) innerFail({ actual, expected, message, operator: "deepStrictEqual" });
};

assert.notDeepStrictEqual = function notDeepStrictEqual(actual, expected, message) {
  if (arguments.length < 2) throw missingArgs();
  if (isDeepStrictEqual(actual, expected)) innerFail({ actual, expected, message, operator: "notDeepStrictEqual" });
};

assert.strictEqual = function strictEqual(actual, expected, message) {
  if (arguments.length < 2) throw missingArgs();
  if (!Object.is(actual, expected)) {
    if (message != null && !(message instanceof Error)) {
      message = String(message) + "\n\n" + comparison("!==", actual, expected);
    }
    innerFail({ actual, expected, message, operator: "strictEqual" });
  }
};

assert.notStrictEqual = function notStrictEqual(actual, expected, message) {
  if (arguments.length < 2) throw missingArgs();
  if (Object.is(actual, expected)) innerFail({ actual, expected, message, operator: "notStrictEqual" });
};

assert.partialDeepStrictEqual = notImplemented("partialDeepStrictEqual");

function internalMatch(string, regexp, message, fn, not) {
  if (!(regexp instanceof RegExp)) throw $$errInvalidArgType("regexp", "an instance of RegExp", regexp);
  const match = fn === "match";
  if (typeof string !== "string" || (regexp.exec(string) !== null) !== match) {
    if (message instanceof Error) throw message;
    const generatedMessage = !message;
    message =
      message ||
      (typeof string !== "string"
        ? 'The "string" argument must be of type string. Received type ' + `${typeof string} (${inspect(string)})`
        : (match ? "The input did not match the regular expression " : "The input was expected to not match the regular expression ") +
          `${inspect(regexp)}. Input:\n\n${inspect(string)}\n`);
    const err = new AssertionError({ actual: string, expected: regexp, message, operator: fn });
    err.generatedMessage = generatedMessage;
    throw err;
  }
}

assert.match = function match(string, regexp, message) {
  internalMatch(string, regexp, message, "match");
};

assert.doesNotMatch = function doesNotMatch(string, regexp, message) {
  internalMatch(string, regexp, message, "doesNotMatch");
};

const NO_EXCEPTION = {};

function getActual(fn) {
  if (typeof fn !== "function") throw $$errInvalidArgType("fn", "of type function", fn);
  try {
    fn();
  } catch (e) {
    return e;
  }
  return NO_EXCEPTION;
}

async function waitForActual(promiseFn) {
  let resultPromise;
  if (typeof promiseFn === "function") {
    resultPromise = promiseFn();
    if (resultPromise === null || typeof resultPromise !== "object" || typeof resultPromise.then !== "function") {
      throw $$errInvalidArgType("promiseFn", "of type Promise", resultPromise);
    }
  } else if (promiseFn !== null && typeof promiseFn === "object" && typeof promiseFn.then === "function") {
    resultPromise = promiseFn;
  } else {
    throw $$errInvalidArgType("promiseFn", "of type function or an instance of Promise", promiseFn);
  }
  try {
    await resultPromise;
  } catch (e) {
    return e;
  }
  return NO_EXCEPTION;
}

// Does `actual` (thrown) satisfy `expected` (a class, a validation function, a RegExp or an object
// of properties)? Throws the AssertionError (or `actual`) when it doesn't.
function expectedException(actual, expected, message, fn) {
  if (typeof expected === "function") {
    if (expected.prototype !== undefined && actual instanceof expected) return;
    if (Error.isPrototypeOf(expected) || expected === Error) {
      // A class the error isn't an instance of.
      const err = new AssertionError({
        actual,
        expected,
        message: message || `The error is expected to be an instance of "${expected.name}". Received "${actual?.constructor?.name}"\n\nError message:\n\n${actual?.message}`,
        operator: fn,
      });
      err.generatedMessage = !message;
      throw err;
    }
    const res = expected.call({}, actual);
    if (res !== true) {
      const err = new AssertionError({
        actual,
        expected,
        message: message || `The ${expected.name ? `"${expected.name}" validation function` : "validation function"} is expected to return "true". Received ${inspect(res)}\n\nCaught error:\n\n${actual}`,
        operator: fn,
      });
      err.generatedMessage = !message;
      throw err;
    }
    return;
  }
  if (expected instanceof RegExp) {
    const str = String(actual);
    if (expected.exec(str) !== null) return;
    const err = new AssertionError({
      actual,
      expected,
      message: message || `The input did not match the regular expression ${inspect(expected)}. Input:\n\n${inspect(str)}\n`,
      operator: fn,
    });
    err.generatedMessage = !message;
    throw err;
  }
  if (expected !== null && typeof expected === "object") {
    const keys = Object.keys(expected);
    if (expected instanceof Error) keys.push("name", "message");
    for (const key of keys) {
      const want = expected[key];
      const got = actual === null || actual === undefined ? undefined : actual[key];
      if (typeof got === "string" && want instanceof RegExp && want.exec(got) !== null) continue;
      if (!isDeepStrictEqual(got, want)) {
        innerFail({ actual, expected, message, operator: fn });
      }
    }
    return;
  }
  throw $$errInvalidArgType("expected", "of type function or an instance of Error, RegExp, or Object", expected);
}

function expectsError(fn, actual, error, message) {
  if (typeof error === "string") {
    message = error;
    error = undefined;
  }
  if (actual === NO_EXCEPTION) {
    let details = "";
    if (error && error.name) details += ` (${error.name})`;
    details += message ? `: ${message}` : ".";
    const fnType = fn === "rejects" ? "rejection" : "exception";
    innerFail({ actual: undefined, expected: error, operator: fn, message: `Missing expected ${fnType}${details}` });
  }
  if (!error) return;
  expectedException(actual, error, message, fn);
}

function expectsNoError(fn, actual, error, message) {
  if (actual === NO_EXCEPTION) return;
  if (typeof error === "string") {
    message = error;
    error = undefined;
  }
  if (!error || (typeof error === "function" && error.prototype !== undefined && actual instanceof error)) {
    const details = message ? `: ${message}` : ".";
    const fnType = fn === "doesNotReject" ? "rejection" : "exception";
    innerFail({
      actual,
      expected: error,
      operator: fn,
      message: `Got unwanted ${fnType}${details}\nActual message: "${actual && actual.message}"`,
    });
  }
  throw actual;
}

assert.throws = function throws(promiseFn, ...args) {
  expectsError("throws", getActual(promiseFn), ...args);
};

assert.rejects = async function rejects(promiseFn, ...args) {
  expectsError("rejects", await waitForActual(promiseFn), ...args);
};

assert.doesNotThrow = function doesNotThrow(fn, ...args) {
  expectsNoError("doesNotThrow", getActual(fn), ...args);
};

assert.doesNotReject = async function doesNotReject(fn, ...args) {
  expectsNoError("doesNotReject", await waitForActual(fn), ...args);
};

assert.ifError = function ifError(err) {
  if (err !== null && err !== undefined) {
    let message = "ifError got unwanted exception: ";
    if (typeof err === "object" && typeof err.message === "string") {
      if (err.message.length === 0 && err.constructor) {
        message += err.constructor.name;
      } else {
        message += err.message;
      }
    } else {
      message += inspect(err);
    }
    const newErr = new AssertionError({ actual: err, expected: null, operator: "ifError", message });
    newErr.generatedMessage = false;
    throw newErr;
  }
};

// The strict mode: `equal` is `strictEqual`, and so on.
function strict(...args) {
  innerOk(strict, args.length, ...args);
}
assert.strict = strict;
assert.Assert = notImplemented("Assert");
const keys = Object.keys(assert);
for (let i = 0; i < keys.length; i++) {
  if (keys[i] !== "strict") strict[keys[i]] = assert[keys[i]];
}
strict.equal = assert.strictEqual;
strict.deepEqual = assert.deepStrictEqual;
strict.notEqual = assert.notStrictEqual;
strict.notDeepEqual = assert.notDeepStrictEqual;
strict.strict = strict;
