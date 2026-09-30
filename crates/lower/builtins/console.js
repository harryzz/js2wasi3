// `console.log` format strings (T25 slice 5): node's `formatWithOptions` (lib/internal/util/inspect.js).
// Used when the first of several arguments is a string with `%` (only linked when a string in the
// user's code has a `%`). Intrinsics: `inspect(v, depth)` is `util.inspect` starting at `depth`
// (0: the default, 2: node's `depth: 0`), strings quoted; `builtin_to_string(o)` is node's
// `hasBuiltInToString`. Not implemented: `%o`'s `showHidden` for objects (traps), BigInt.
// Uses string.js's `$$String`, `$$parseInt`, `$$parseFloat`, json.js's `$$JSON_stringify`.

// node's `formatNumber` without numeric separators: `-0` stays `-0`.
function $$consoleNumber(x) {
  if (x === 0 && 1 / x < 0) {
    return "-0";
  }
  return $$String(x);
}

// `tryStringify`: `JSON.stringify`, `[Circular]` for a cycle.
function $$consoleJson(v) {
  try {
    return $$String($$JSON_stringify(v));
  } catch (e) {
    if (e instanceof TypeError && e.message.startsWith("Converting circular structure to JSON")) {
      return "[Circular]";
    }
    throw e;
  }
}

function $$consoleFormat(args) {
  const first = args[0];
  const n = +args.length;
  let a = 0;
  let str = "";
  let join = "";
  if (typeof first === "string") {
    let lastPos = 0;
    const len = __intrinsic_str_len(first);
    for (let i = 0; i < len - 1; i++) {
      if (__intrinsic_unit(first, i) === 37) {
        i++;
        const next = __intrinsic_unit(first, i);
        if (a + 1 !== n) {
          let tempStr = "";
          if (next === 115) {
            // %s
            const v = args[++a];
            if (typeof v === "number") {
              tempStr = $$consoleNumber(v);
            } else if (typeof v !== "object" || v === null || !__intrinsic_builtin_to_string(v)) {
              tempStr = $$String(v);
            } else {
              tempStr = __intrinsic_inspect(v, 2);
            }
          } else if (next === 106) {
            // %j
            tempStr = $$consoleJson(args[++a]);
          } else if (next === 100) {
            // %d
            const v = args[++a];
            tempStr = typeof v === "symbol" ? "NaN" : $$consoleNumber(+v);
          } else if (next === 79) {
            // %O
            tempStr = __intrinsic_inspect(args[++a], 0);
          } else if (next === 111) {
            // %o: `showHidden` (non-enumerable properties) isn't implemented for objects.
            const v = args[++a];
            if ((typeof v === "object" && v !== null) || typeof v === "function") {
              __intrinsic_trap();
            }
            tempStr = __intrinsic_inspect(v, 0);
          } else if (next === 105) {
            // %i
            const v = args[++a];
            tempStr = typeof v === "symbol" ? "NaN" : $$consoleNumber($$parseInt(v));
          } else if (next === 102) {
            // %f
            const v = args[++a];
            tempStr = typeof v === "symbol" ? "NaN" : $$consoleNumber($$parseFloat(v));
          } else if (next === 99) {
            // %c
            a += 1;
            tempStr = "";
          } else if (next === 37) {
            str += __intrinsic_sub(first, lastPos, i);
            lastPos = i + 1;
            continue;
          } else {
            continue;
          }
          if (lastPos !== i - 1) {
            str += __intrinsic_sub(first, lastPos, i - 1);
          }
          str += tempStr;
          lastPos = i + 1;
        } else if (next === 37) {
          str += __intrinsic_sub(first, lastPos, i);
          lastPos = i + 1;
        }
      }
    }
    if (lastPos !== 0) {
      a++;
      join = " ";
      if (lastPos < len) {
        str += __intrinsic_sub(first, lastPos, len);
      }
    }
  }
  while (a < n) {
    const value = args[a];
    str += join;
    str += typeof value !== "string" ? __intrinsic_inspect(value, -1) : value;
    join = " ";
    a++;
  }
  return str;
}

// ---- `console` as a value (T53) --------------------------------------------------------------
// `console.log(x)` is lowered directly; `console` anywhere else (`const log = console.log`,
// `console.log(...xs)`, `typeof console`) is this object, made on first use, with the methods
// implemented here (enumerable, in node's order). Intrinsics `write_stdout/stderr(text)`.

let $$consoleObject = undefined;

function $$Console() {
  if ($$consoleObject === undefined) {
    const c = {};
    c.log = $$Console_log;
    c.info = $$Console_info;
    c.debug = $$Console_debug;
    c.warn = $$Console_warn;
    c.error = $$Console_error;
    $$consoleObject = c;
  }
  return $$consoleObject;
}

// A line as `console.log` writes it: node's `formatWithOptions` (a lone string as it is).
function $$consoleLine(args) {
  if (+args.length === 1 && typeof args[0] === "string") {
    return args[0] + "\n";
  }
  return $$consoleFormat(args) + "\n";
}

function $$Console_log(...args) {
  __intrinsic_write_stdout($$consoleLine(args));
}

function $$Console_warn(...args) {
  __intrinsic_write_stderr($$consoleLine(args));
}

function $$Console_debug(...args) {
  __intrinsic_write_stdout($$consoleLine(args));
}

function $$Console_info(...args) {
  __intrinsic_write_stdout($$consoleLine(args));
}

function $$Console_error(...args) {
  __intrinsic_write_stderr($$consoleLine(args));
}
