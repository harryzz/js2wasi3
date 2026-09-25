// Errors are objects: own message/cause, inherited name, instanceof, changes to their prototypes.
function show(e) {
  return e.name + "|" + e.message + "|" + e;
}

const e = new TypeError("bad");
console.log(show(e), Object.hasOwn(e, "message"), Object.hasOwn(e, "name"), "stack" in e);
console.log(Object.getPrototypeOf(e) === TypeError.prototype);
console.log(Object.getPrototypeOf(TypeError.prototype) === Error.prototype);
console.log(Error.prototype.name, TypeError.prototype.name, "|" + Error.prototype.message + "|");

// No message argument: no own message, the inherited "" shows.
const bare = new Error();
console.log(show(bare), Object.hasOwn(bare, "message"));

// cause from an options object (own, only if present).
const c = new Error("with cause", { cause: 42 });
const noCause = new Error("no cause", {});
console.log(c.cause, Object.hasOwn(c, "cause"), Object.hasOwn(noCause, "cause"), "cause" in noCause);
const inner = new RangeError("inner");
const outer = new Error("outer", { cause: inner });
console.log(outer.cause === inner, "" + outer.cause);

// Properties can be changed and added.
e.name = "CustomError";
e.code = 7;
console.log(show(e), e.code, e instanceof TypeError);
e.message = "";
console.log("" + e);
e.name = "";
e.message = "only message";
console.log("" + e);

// Changing a prototype changes every error that inherits from it.
RangeError.prototype.name = "Renamed";
console.log("" + new RangeError("x"), "" + inner);
delete RangeError.prototype.name;
console.log("" + inner);

// Thrown and caught errors are the same objects.
try {
  throw outer;
} catch (x) {
  console.log(x === outer, x.cause === inner, x instanceof Error);
}

// Properties of a thrown TypeError from the runtime.
try {
  null.prop;
} catch (x) {
  console.log(x instanceof TypeError, x.name, x.message, Object.hasOwn(x, "message"));
}
