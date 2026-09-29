
// ---- the global object (T42, docs/design.md D6) ----------------------------------------------
// Made on first use, with the built-ins the user's code names (`__intrinsic_global_init`: the
// lowering writes the definitions) as writable, non-enumerable, configurable data properties.

let $$globalObject = undefined;

function $$GlobalObject() {
  if ($$globalObject === undefined) {
    const g = {};
    $$globalObject = g;
    __intrinsic_global_init(g);
    Object.defineProperty(g, "NaN", { value: NaN, writable: false, enumerable: false, configurable: false });
    Object.defineProperty(g, "Infinity", { value: Infinity, writable: false, enumerable: false, configurable: false });
    Object.defineProperty(g, "undefined", { value: undefined, writable: false, enumerable: false, configurable: false });
  }
  return $$globalObject;
}

// Reading an unresolvable name `name`: the global object's property, else a ReferenceError.
function $$GlobalGet(name) {
  const g = $$GlobalObject();
  if (!(name in g)) {
    throw new ReferenceError(name + " is not defined");
  }
  return g[name];
}

// `typeof name` of an unresolvable name: no ReferenceError.
function $$GlobalRead(name) {
  const g = $$GlobalObject();
  return g[name];
}

// Assigning to an unresolvable name (strict code): only an existing property.
function $$GlobalSet(name, value) {
  const g = $$GlobalObject();
  if (!(name in g)) {
    throw new ReferenceError(name + " is not defined");
  }
  g[name] = value;
}

// `eval` and `Function` as values (T42): code known at build time is compiled where it's called
// (`eval("…")`, `Function("…")`); code only known at run time needs an engine inside the program
// (F191).
function $$eval(x) {
  if (typeof x !== "string") {
    return x;
  }
  throw new EvalError("js2wasi3 compiles eval of code known at build time only (a string literal where eval is called)");
}

function $$Function() {
  throw new EvalError("js2wasi3 compiles Function of code known at build time only (string literals where Function is called)");
}
