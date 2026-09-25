// A throw from a nested call unwinds through several frames (and their finally blocks).
function depth3(n) {
  if (n > 2) throw new RangeError("too deep: " + n);
  return n * 10;
}

function depth2(n) {
  try {
    return depth3(n) + 1;
  } finally {
    console.log("depth2 finally", n);
  }
}

function depth1(n) {
  return depth2(n) + depth2(n - 1);
}

for (let i = 1; i <= 4; i++) {
  try {
    console.log("depth1", i, "=", depth1(i));
  } catch (e) {
    console.log("caught at top:", e.name + ": " + e.message);
  }
}

function countdown(n) {
  if (n === 0) throw "bottom";
  countdown(n - 1);
}

try {
  countdown(50);
} catch (e) {
  console.log("recursion unwound:", e);
}

function alwaysThrows(msg) {
  throw new Error(msg);
}

function useIt(ok) {
  if (ok) return "fine";
  alwaysThrows("not ok");
}

console.log(useIt(true));
try {
  useIt(false);
} catch (e) {
  console.log(e.message);
}
