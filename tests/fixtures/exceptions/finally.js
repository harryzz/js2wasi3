// finally: normal completion, throw, return (and return overriding), order of effects.
function normal() {
  let log = "";
  try {
    log += "try ";
  } finally {
    log += "finally";
  }
  return log;
}

function returnInTry() {
  let n = 0;
  try {
    n = 1;
    return n;
  } finally {
    n = 2;
    console.log("finally ran, n =", n);
  }
}

function returnOverrides() {
  try {
    return "try";
  } finally {
    return "finally";
  }
}

function returnSwallowsThrow() {
  try {
    throw new Error("lost");
  } finally {
    return "swallowed";
  }
}

function throwInFinally() {
  try {
    return 1;
  } finally {
    throw new TypeError("from finally");
  }
}

function catchThenFinally(fail) {
  let s = "";
  try {
    s += "a";
    if (fail) throw "x";
    s += "b";
  } catch (e) {
    s += "c" + e;
  } finally {
    s += "f";
  }
  return s;
}

function rethrowThroughFinally() {
  try {
    throw new RangeError("through");
  } finally {
    console.log("cleanup before propagating");
  }
}

console.log(normal());
console.log(returnInTry());
console.log(returnOverrides());
console.log(returnSwallowsThrow());
try {
  throwInFinally();
} catch (e) {
  console.log("caught:", e.name, e.message);
}
console.log(catchThenFinally(false), catchThenFinally(true));
try {
  rethrowThroughFinally();
} catch (e) {
  console.log("caught:", "" + e);
}
