// throw and catch: primitives, errors, catch without a binding, nesting, rethrow.
function thrower(kind) {
  if (kind === 0) throw 42;
  if (kind === 1) throw "a string";
  if (kind === 2) throw new Error("boom");
  if (kind === 3) throw undefined;
  return "no throw";
}

for (let k = 0; k < 5; k++) {
  try {
    console.log("result:", thrower(k));
  } catch (e) {
    console.log("caught", typeof e, e === undefined ? "undefined" : "" + e);
  }
}

try {
  throw 1;
} catch {
  console.log("caught without a binding");
}

try {
  try {
    throw "inner";
  } catch (e) {
    console.log("inner catch:", e);
    throw e + " rethrown";
  }
} catch (e) {
  console.log("outer catch:", e);
}

let x = 0;
try {
  x = 1;
  throw -0;
} catch (e) {
  console.log("x =", x, "e =", e);
}
