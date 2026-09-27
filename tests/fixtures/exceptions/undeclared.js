// Reading a name declared nowhere throws a ReferenceError at run time (F139).
try {
  console.log(notDeclared + 1);
} catch (e) {
  console.log(e instanceof ReferenceError, e.name, e.message);
}
console.log(typeof notDeclared, typeof (alsoMissing));
console.log(typeof notDeclared === "undefined", "undefined" !== typeof alsoMissing, typeof nope == "object");
function later() {
  return missingInFunction;
}
try {
  later();
} catch (e) {
  console.log(String(e));
}
const f = () => [1, ...unresolvableReference];
try {
  f();
} catch (e) {
  console.log(e.constructor === ReferenceError, e.message);
}
let evaluated = false;
try {
  missing(evaluated = true);
} catch (e) {
  console.log(e.message, evaluated);
}
