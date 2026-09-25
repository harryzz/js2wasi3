// Error values: the native constructors, name/message, conversions, equality.
function show(e) {
  console.log(typeof e, e.name, "|" + e.message + "|", `${e}`, "" + e, !e, !!e);
}

show(new Error("plain"));
show(new TypeError("type"));
show(new RangeError("range"));
show(new ReferenceError("ref"));
show(new SyntaxError("syntax"));
show(new EvalError("eval"));
show(new URIError("uri"));
show(Error("called without new"));
show(TypeError("also without new"));
show(new Error());
show(new Error(undefined));
show(new Error(null));
show(new Error(12.5));
show(new Error(-0));
show(new Error(true));
show(new TypeError(""));
show(new Error(new RangeError("nested")));

const e = new Error("m");
const f = e;
console.log(e === f, e === new Error("m"), e == f, e == "Error: m", e != "Error: x", e == null);
console.log(e + 1, 1 + e, e + e);
console.log(typeof e === "object", typeof e == "function");
console.log(e.message.length, e.name.length);

// Primitives have no name/message.
console.log("str".name, (5).message, true.name);
