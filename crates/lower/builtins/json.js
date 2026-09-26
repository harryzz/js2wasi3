// JSON (T21 slice 4). Conventions: crates/lower/src/builtins.rs.
// `$$JSON_stringify` is `JSON.stringify`: the spec's algorithm (toJSON, replacer function or
// property list, gap), writing the pieces into one array that is joined at the end. Circular
// structures throw node's TypeError (the path to the cycle). Not implemented: wrapper objects
// (`new Number(1)`, F88), `rawJSON`, `JSON.parse`.
// Uses string.js's `$$toInteger`, `$$String`.

// The state of one `JSON.stringify` call.
function $$jsonState(replacerFn, propertyList, gap) {
  return {
    replacerFn: replacerFn,
    propertyList: propertyList,
    gap: gap,
    indent: "",
    // The objects being serialized and the keys they were found under (`index` for arrays).
    stack: [],
    keys: [],
    out: [],
  };
}

function $$JSON_stringify(value, replacer, space) {
  let replacerFn = undefined;
  let propertyList = undefined;
  if (typeof replacer === "function") {
    replacerFn = replacer;
  } else if (Array.isArray(replacer)) {
    propertyList = [];
    const len = +replacer.length;
    for (let i = 0; i < len; i++) {
      const v = replacer[i];
      let item = undefined;
      if (typeof v === "string") {
        item = v;
      } else if (typeof v === "number") {
        item = $$String(v);
      }
      if (item !== undefined && !propertyList.includes(item)) {
        propertyList.push(item);
      }
    }
  }
  let gap = "";
  if (typeof space === "number") {
    let n = $$toInteger(space);
    if (n > 10) {
      n = 10;
    }
    for (let i = 0; i < n; i++) {
      gap = gap + " ";
    }
  } else if (typeof space === "string") {
    gap = +space.length <= 10 ? space : space.slice(0, 10);
  }
  const state = $$jsonState(replacerFn, propertyList, gap);
  const wrapper = {};
  __intrinsic_define(wrapper, "", value);
  const v = $$jsonValue(state, wrapper, "", value);
  if (!$$jsonWritable(v)) {
    return undefined;
  }
  $$jsonWrite(state, v, "", false);
  return state.out.join("");
}

// SerializeJSONProperty up to the value to write: `toJSON`, then the replacer function.
function $$jsonValue(state, holder, key, value) {
  const t = typeof value;
  if ((t === "object" && value !== null) || t === "bigint") {
    const toJSON = value.toJSON;
    if (typeof toJSON === "function") {
      value = toJSON.call(value, key);
    }
  }
  if (state.replacerFn !== undefined) {
    value = state.replacerFn.call(holder, key, value);
  }
  return value;
}

// Does SerializeJSONProperty give a string for this value (not `undefined`)?
function $$jsonWritable(v) {
  const t = typeof v;
  return t !== "undefined" && t !== "function" && t !== "symbol";
}

// Write a value `$$jsonWritable` accepted, found under `key` (`index`: an array index).
function $$jsonWrite(state, value, key, index) {
  const out = state.out;
  if (value === null) {
    out.push("null");
    return;
  }
  const t = typeof value;
  if (t === "boolean") {
    out.push(value ? "true" : "false");
    return;
  }
  if (t === "string") {
    out.push($$jsonQuote(value));
    return;
  }
  if (t === "number") {
    out.push(value - value === 0 ? $$String(value) : "null");
    return;
  }
  if (t === "bigint") {
    throw new TypeError("Do not know how to serialize a BigInt");
  }
  if (state.stack.includes(value)) {
    throw new TypeError($$jsonCircular(state, value, key, index));
  }
  state.stack.push(value);
  state.keys.push(key);
  const stepback = state.indent;
  state.indent = stepback + state.gap;
  if (Array.isArray(value)) {
    $$jsonArray(state, value);
  } else {
    $$jsonObject(state, value);
  }
  state.indent = stepback;
  state.stack.pop();
  state.keys.pop();
}

// SerializeJSONObject.
function $$jsonObject(state, value) {
  const out = state.out;
  const keys = state.propertyList !== undefined ? state.propertyList : Object.keys(value);
  const n = +keys.length;
  const gap = state.gap;
  let empty = true;
  out.push("{");
  for (let i = 0; i < n; i++) {
    const k = keys[i];
    const v = $$jsonValue(state, value, k, value[k]);
    if (!$$jsonWritable(v)) {
      continue;
    }
    if (!empty) {
      out.push(",");
    }
    empty = false;
    if (gap !== "") {
      out.push("\n" + state.indent);
    }
    out.push($$jsonQuote(k));
    out.push(gap === "" ? ":" : ": ");
    $$jsonWrite(state, v, k, false);
  }
  if (!empty && gap !== "") {
    out.push("\n" + state.indent.slice(0, +state.indent.length - +gap.length));
  }
  out.push("}");
}

// SerializeJSONArray.
function $$jsonArray(state, value) {
  const out = state.out;
  const len = +value.length;
  const gap = state.gap;
  out.push("[");
  for (let i = 0; i < len; i++) {
    if (i > 0) {
      out.push(",");
    }
    if (gap !== "") {
      out.push("\n" + state.indent);
    }
    const k = $$String(i);
    const v = $$jsonValue(state, value, k, value[i]);
    if ($$jsonWritable(v)) {
      $$jsonWrite(state, v, k, true);
    } else {
      out.push("null");
    }
  }
  if (len > 0 && gap !== "") {
    out.push("\n" + state.indent.slice(0, +state.indent.length - +gap.length));
  }
  out.push("]");
}

// QuoteJSONString.
function $$jsonQuote(s) {
  const len = __intrinsic_str_len(s);
  // The length of the result, and whether anything needs escaping.
  let n = len + 2;
  for (let i = 0; i < len; i++) {
    const u = __intrinsic_unit(s, i);
    if (u < 32) {
      n += u === 8 || u === 9 || u === 10 || u === 12 || u === 13 ? 1 : 5;
    } else if (u === 34 || u === 92) {
      n += 1;
    } else if (u >= 0xd800 && u <= 0xdfff) {
      if (u <= 0xdbff && i + 1 < len) {
        const next = __intrinsic_unit(s, i + 1);
        if (next >= 0xdc00 && next <= 0xdfff) {
          i++;
          continue;
        }
      }
      n += 5;
    }
  }
  if (n === len + 2) {
    return '"' + s + '"';
  }
  const r = __intrinsic_str_new(n);
  let j = 0;
  __intrinsic_str_set(r, j++, 34);
  for (let i = 0; i < len; i++) {
    const u = __intrinsic_unit(s, i);
    let esc = 0;
    if (u === 8) {
      esc = 98;
    } else if (u === 9) {
      esc = 116;
    } else if (u === 10) {
      esc = 110;
    } else if (u === 12) {
      esc = 102;
    } else if (u === 13) {
      esc = 114;
    } else if (u === 34 || u === 92) {
      esc = u;
    } else if (u < 32) {
      esc = -1;
    } else if (u >= 0xd800 && u <= 0xdfff) {
      esc = -1;
      if (u <= 0xdbff && i + 1 < len) {
        const next = __intrinsic_unit(s, i + 1);
        if (next >= 0xdc00 && next <= 0xdfff) {
          __intrinsic_str_set(r, j++, u);
          __intrinsic_str_set(r, j++, next);
          i++;
          continue;
        }
      }
    }
    if (esc === 0) {
      __intrinsic_str_set(r, j++, u);
    } else if (esc > 0) {
      __intrinsic_str_set(r, j++, 92);
      __intrinsic_str_set(r, j++, esc);
    } else {
      // `\u` and four lowercase hex digits.
      __intrinsic_str_set(r, j++, 92);
      __intrinsic_str_set(r, j++, 117);
      for (let shift = 12; shift >= 0; shift -= 4) {
        const d = (u >> shift) & 15;
        __intrinsic_str_set(r, j++, d < 10 ? 48 + d : 87 + d);
      }
    }
  }
  __intrinsic_str_set(r, j, 34);
  return r;
}

// V8's name for an object's class in the circular-structure message: the nearest own
// `constructor`'s name (built-in prototypes' other than Object's and Array's trap: F37).
function $$jsonCtorName(o) {
  if (Array.isArray(o)) {
    return "Array";
  }
  let p = Object.getPrototypeOf(o);
  while (p !== null && p !== Object.prototype && !Object.hasOwn(p, "constructor")) {
    p = Object.getPrototypeOf(p);
  }
  if (p === null || p === Object.prototype) {
    return "Object";
  }
  const c = p.constructor;
  if (typeof c === "function" && c.name !== "") {
    return c.name;
  }
  return "Object";
}

function $$jsonKeyText(key, index) {
  return index ? "index " + key : "property '" + key + "'";
}

// node's message for a cycle: `value` (found under `key`) is already on the stack.
function $$jsonCircular(state, value, key, index) {
  const stack = state.stack;
  const keys = state.keys;
  const start = stack.indexOf(value);
  let msg =
    "Converting circular structure to JSON\n    --> starting at object with constructor '" +
    $$jsonCtorName(stack[start]) +
    "'";
  // The objects between the start and the closing key: the first 2 and the last one.
  const n = +stack.length;
  for (let i = start + 1; i < n; i++) {
    if (n - start - 1 > 3 && i === start + 3) {
      msg = msg + "\n    |     ...";
      i = n - 2;
      continue;
    }
    const k = keys[i];
    msg =
      msg +
      "\n    |     " +
      $$jsonKeyText(k, Array.isArray(stack[i - 1])) +
      " -> object with constructor '" +
      $$jsonCtorName(stack[i]) +
      "'";
  }
  return msg + "\n    --- " + $$jsonKeyText(key, index) + " closes the circle";
}
