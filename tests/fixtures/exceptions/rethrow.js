// Catch, inspect, rethrow; the order of catch and finally blocks across frames.
let trace = "";

function inner(n) {
  try {
    trace += "i";
    if (n % 2 === 1) throw new RangeError("odd " + n);
    return n;
  } catch (e) {
    trace += "c";
    if (n > 2) throw e;
    return -1;
  } finally {
    trace += "f";
  }
}

function outer(n) {
  try {
    return inner(n) * 2;
  } catch (e) {
    trace += "C";
    throw new Error("outer(" + n + "): " + e.message);
  } finally {
    trace += "F";
  }
}

for (let n = 0; n < 5; n++) {
  trace = "";
  let r;
  try {
    r = outer(n);
  } catch (e) {
    r = "" + e;
  }
  console.log(n, r, trace);
}

// A caught value can be anything, and the binding is a normal variable.
try {
  throw 3;
} catch (e) {
  e = e + 1;
  console.log(e);
}

// The catch binding shadows an outer name.
let e = "outer e";
try {
  throw "inner e";
} catch (e) {
  console.log(e);
}
console.log(e);
