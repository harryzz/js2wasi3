// Support for node's built-in modules (T55; their code is in `crates/frontend/node/`): node's
// errors.

// node's `determineSpecificType` (lib/internal/errors.js): how an error message shows a value.
function $$nodeSpecificType(v) {
  if (v === undefined || v === null) return "" + v;
  if (typeof v === "function") return "function " + v.name;
  if (typeof v === "object") {
    // `constructor` is readable only when the program names it (T25).
    const proto = Object.getPrototypeOf(v);
    let name = proto === null ? "" : proto === Array.prototype ? "Array" : "Object";
    if (__intrinsic_named("constructor") && proto !== null) {
      const c = v.constructor;
      name = typeof c === "function" ? c.name : "";
    }
    return name !== "" ? "an instance of " + name : String(v);
  }
  let shown = String(v);
  if (typeof v === "number" && v === 0 && 1 / v < 0) shown = "-0";
  if (typeof v === "string") {
    if (v.length > 28) v = v.slice(0, 25) + "...";
    shown = v.indexOf("'") === -1 ? "'" + v + "'" : $$JSON_stringify(v);
  }
  if (__intrinsic_bigint_mode()) {
    if (typeof v === "bigint") shown += "n";
  }
  return "type " + typeof v + " (" + shown + ")";
}

// node's `ERR_INVALID_ARG_TYPE`: `The "path" argument must be of type string. Received …`
// (`expected`: "of type string"; a name with a `.` is a property).
function $$errInvalidArgType(name, expected, v) {
  const what = name.includes(".") ? "property" : "argument";
  const e = new TypeError('The "' + name + '" ' + what + " must be " + expected + ". Received " + $$nodeSpecificType(v));
  e.code = "ERR_INVALID_ARG_TYPE";
  return e;
}


// node's `ERR_OUT_OF_RANGE`: `The value of "code" is out of range. It must be an integer. Received 2.5`.
function $$errOutOfRange(name, range, v) {
  const e = new RangeError('The value of "' + name + '" is out of range. It must be ' + range + ". Received " + String(v));
  e.code = "ERR_OUT_OF_RANGE";
  return e;
}

// `util.inspect.custom` (T56): the formatter calls this first for every object (only in programs
// whose code has the string "nodejs.util.inspect.custom"): `v` itself if it has no such method (or
// is a prototype, as node checks), else the method's result, called with node's arguments
// (`depth`: the levels left, the options, `inspect`).
let $$inspectSymbol = undefined;
function $$inspectCustom(v, depth) {
  if ($$inspectSymbol === undefined) $$inspectSymbol = Symbol.for("nodejs.util.inspect.custom");
  const f = v[$$inspectSymbol];
  if (typeof f !== "function") return v;
  // node skips a prototype (`value.constructor.prototype === value`): its own `constructor`.
  const d = Object.getOwnPropertyDescriptor(v, "constructor");
  if (d !== undefined && typeof d.value === "function" && d.value.prototype === v) return v;
  const options = {
    showHidden: false, depth: 2, colors: false, customInspect: true, showProxy: false, maxArrayLength: 100,
    maxStringLength: 10000, breakLength: 80, compact: 3, sorted: false, getters: false, numericSeparator: false,
    stylize: $$inspectStylize,
  };
  options.depth = depth;
  return f.call(v, depth, options, $$inspectValue);
}
function $$inspectStylize(text, styleType) {
  return text;
}
// `inspect` for custom inspectors: `util.inspect(v, options)` with only `depth`.
function $$inspectValue(v, options) {
  let depth = 2;
  if (options !== null && typeof options === "object" && options.depth !== undefined) depth = options.depth;
  if (depth === 2) return __intrinsic_inspect(v, 0);
  if (depth === null || depth > 1000000) depth = 1000000;
  return __intrinsic_inspect_depth(v, 2 - Math.floor(depth));
}
