// JSON (T21 slice 4). Conventions: crates/lower/src/builtins.rs.
// `$$JSON_stringify` is `JSON.stringify`: the spec's algorithm (toJSON, replacer function or
// property list, gap), writing the pieces into one array that is joined at the end. Circular
// structures throw node's TypeError (the path to the cycle). `$$JSON_parse` is `JSON.parse`
// (T25 slice 1): a recursive-descent parser over code units with node 24's SyntaxError messages,
// then the reviver walk. Not implemented: wrapper objects (`new Number(1)`, F88), `rawJSON`.
// Uses string.js's `$$toInteger`, `$$String`, `$$fromUnit`, `$$StringToNumber`.

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
      let v = replacer[i];
      if (typeof v === "object" && v !== null) {
        const p = __intrinsic_unwrap(v);
        if (typeof p === "string" || typeof p === "number") {
          v = __intrinsic_to_string(v);
        }
      }
      let item = undefined;
      if (typeof v === "string") {
        item = v;
      } else if (typeof v === "number") {
        item = $$String(v);
      }
      if (item !== undefined && !propertyList.includes(item)) {
        $$append(propertyList, item);
      }
    }
  }
  let gap = "";
  if (typeof space === "object" && space !== null) {
    const p = __intrinsic_unwrap(space);
    if (typeof p === "number") {
      space = +space;
    } else if (typeof p === "string") {
      space = __intrinsic_to_string(space);
    }
  }
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
  return $$jsonUnwrap(value);
}

// Number, String and Boolean wrapper objects (T30): ToNumber, ToString, their boolean.
function $$jsonUnwrap(value) {
  const p = __intrinsic_unwrap(value);
  if (p === value) {
    return value;
  }
  const t = typeof p;
  if (t === "number") {
    return +value;
  }
  if (t === "string") {
    return __intrinsic_to_string(value);
  }
  if (t === "boolean") {
    return p;
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
    $$append(out, "null");
    return;
  }
  const t = typeof value;
  if (t === "boolean") {
    $$append(out, value ? "true" : "false");
    return;
  }
  if (t === "string") {
    $$append(out, $$jsonQuote(value));
    return;
  }
  if (t === "number") {
    $$append(out, value - value === 0 ? $$String(value) : "null");
    return;
  }
  if (t === "bigint") {
    throw new TypeError("Do not know how to serialize a BigInt");
  }
  if (state.stack.includes(value)) {
    throw new TypeError($$jsonCircular(state, value, key, index));
  }
  $$append(state.stack, value);
  $$append(state.keys, key);
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
  $$append(out, "{");
  for (let i = 0; i < n; i++) {
    const k = keys[i];
    const v = $$jsonValue(state, value, k, value[k]);
    if (!$$jsonWritable(v)) {
      continue;
    }
    if (!empty) {
      $$append(out, ",");
    }
    empty = false;
    if (gap !== "") {
      $$append(out, "\n" + state.indent);
    }
    $$append(out, $$jsonQuote(k));
    $$append(out, gap === "" ? ":" : ": ");
    $$jsonWrite(state, v, k, false);
  }
  if (!empty && gap !== "") {
    $$append(out, "\n" + state.indent.slice(0, +state.indent.length - +gap.length));
  }
  $$append(out, "}");
}

// SerializeJSONArray.
function $$jsonArray(state, value) {
  const out = state.out;
  const len = +value.length;
  const gap = state.gap;
  $$append(out, "[");
  for (let i = 0; i < len; i++) {
    if (i > 0) {
      $$append(out, ",");
    }
    if (gap !== "") {
      $$append(out, "\n" + state.indent);
    }
    const k = $$String(i);
    const v = $$jsonValue(state, value, k, value[i]);
    if ($$jsonWritable(v)) {
      $$jsonWrite(state, v, k, true);
    } else {
      $$append(out, "null");
    }
  }
  if (len > 0 && gap !== "") {
    $$append(out, "\n" + state.indent.slice(0, +state.indent.length - +gap.length));
  }
  $$append(out, "]");
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

// ---- JSON.parse ----------------------------------------------------------------------------

// The text being parsed and the index of the next code unit. No user code runs while parsing
// (the reviver runs after), so one parse at a time.
let $$jsonSrc = null;
let $$jsonLen = 0;
let $$jsonPos = 0;

function $$JSON_parse(text, reviver) {
  const src = __intrinsic_to_string(text);
  $$jsonSrc = src;
  $$jsonLen = __intrinsic_str_len(src);
  $$jsonPos = 0;
  const value = $$jsonParseValue();
  $$jsonSkipSpace();
  if ($$jsonPos < $$jsonLen) {
    throw new SyntaxError(
      "Unexpected non-whitespace character after JSON at position " + $$jsonWhere($$jsonPos),
    );
  }
  $$jsonSrc = null;
  if (typeof reviver === "function") {
    const root = {};
    __intrinsic_define(root, "", value);
    return $$jsonInternalize(root, "", reviver);
  }
  return value;
}

function $$jsonFail(msg, pos) {
  throw new SyntaxError(msg + " in JSON at position " + $$jsonWhere(pos));
}

// `n (line l column c)` for a position (node 24); `\n`, `\r\n` and `\r` end lines. Resets the state.
function $$jsonWhere(pos) {
  const src = $$jsonSrc;
  $$jsonSrc = null;
  let line = 1;
  let start = 0;
  for (let i = 0; i < pos; i++) {
    const c = __intrinsic_unit(src, i);
    if (c === 13 && i + 1 < pos && __intrinsic_unit(src, i + 1) === 10) {
      i++;
    }
    if (c === 10 || c === 13) {
      line++;
      start = i + 1;
    }
  }
  return pos + " (line " + line + " column " + (pos - start + 1) + ")";
}

// V8's error for an unexpected token at `pos` (or the end of the input).
function $$jsonUnexpected(pos) {
  const src = $$jsonSrc;
  const len = $$jsonLen;
  if (pos >= len) {
    $$jsonSrc = null;
    throw new SyntaxError("Unexpected end of JSON input");
  }
  const c = __intrinsic_unit(src, pos);
  if (c === 34) {
    $$jsonFail("Unexpected string", pos);
  }
  if (c === 45 || (c >= 48 && c <= 57)) {
    $$jsonFail("Unexpected number", pos);
  }
  $$jsonSrc = null;
  if (src === "undefined" || src === "NaN" || src === "Infinity" || src === "[object Object]") {
    throw new SyntaxError('"' + src + '" is not valid JSON');
  }
  // Sources of 21 code units or more show 10 units of context on each side of `pos`.
  const token = "Unexpected token '" + $$fromUnit(c) + "', ";
  let context = '"' + src + '"';
  if (len >= 21) {
    if (pos < 10) {
      context = '"' + __intrinsic_sub(src, 0, pos + 10) + '"...';
    } else if (pos < len - 10) {
      context = '..."' + __intrinsic_sub(src, pos - 10, pos + 10) + '"...';
    } else {
      context = '..."' + __intrinsic_sub(src, pos - 10, len) + '"';
    }
  }
  throw new SyntaxError(token + context + " is not valid JSON");
}

// Skip JSON whitespace (tab, LF, CR, space).
function $$jsonSkipSpace() {
  const src = $$jsonSrc;
  const len = $$jsonLen;
  let i = $$jsonPos;
  while (i < len) {
    const c = __intrinsic_unit(src, i);
    if (c !== 32 && c !== 10 && c !== 13 && c !== 9) {
      break;
    }
    i++;
  }
  $$jsonPos = i;
}

// The code unit at `i`, or -1 at the end.
function $$jsonAt(i) {
  return i < $$jsonLen ? __intrinsic_unit($$jsonSrc, i) : -1;
}

function $$jsonIsDigit(c) {
  return c >= 48 && c <= 57;
}

function $$jsonParseValue() {
  $$jsonSkipSpace();
  const pos = $$jsonPos;
  const c = $$jsonAt(pos);
  if (c === 34) {
    return $$jsonParseString();
  }
  if (c === 123) {
    return $$jsonParseObject();
  }
  if (c === 91) {
    return $$jsonParseArray();
  }
  if (c === 45 || $$jsonIsDigit(c)) {
    return $$jsonParseNumber();
  }
  if (c === 116) {
    $$jsonLiteral("true");
    return true;
  }
  if (c === 102) {
    $$jsonLiteral("false");
    return false;
  }
  if (c === 110) {
    $$jsonLiteral("null");
    return null;
  }
  return $$jsonUnexpected(pos);
}

// `true`, `false` or `null` at the current position (its first unit already checked).
function $$jsonLiteral(word) {
  const pos = $$jsonPos;
  const n = __intrinsic_str_len(word);
  for (let i = 1; i < n; i++) {
    if ($$jsonAt(pos + i) !== __intrinsic_unit(word, i)) {
      $$jsonUnexpected(pos + i);
    }
  }
  $$jsonPos = pos + n;
}

function $$jsonParseNumber() {
  const src = $$jsonSrc;
  const start = $$jsonPos;
  let i = start;
  let c = $$jsonAt(i);
  const neg = c === 45;
  if (neg) {
    i++;
    c = $$jsonAt(i);
    if (!$$jsonIsDigit(c)) {
      $$jsonFail("No number after minus sign", i);
    }
  }
  // An integer of up to 15 digits is exact in a double: no need for StringToNumber.
  let simple = true;
  let v = 0;
  if (c === 48) {
    i++;
    if ($$jsonIsDigit($$jsonAt(i))) {
      $$jsonFail("Unexpected number", i);
    }
  } else {
    const digits = i;
    while ($$jsonIsDigit(c)) {
      v = v * 10 + (c - 48);
      i++;
      c = $$jsonAt(i);
    }
    simple = i - digits <= 15;
  }
  c = $$jsonAt(i);
  if (c === 46) {
    simple = false;
    i++;
    if (!$$jsonIsDigit($$jsonAt(i))) {
      $$jsonFail("Unterminated fractional number", i);
    }
    while ($$jsonIsDigit($$jsonAt(i))) {
      i++;
    }
    c = $$jsonAt(i);
  }
  if (c === 101 || c === 69) {
    simple = false;
    i++;
    c = $$jsonAt(i);
    if (c === 43 || c === 45) {
      i++;
    }
    if (!$$jsonIsDigit($$jsonAt(i))) {
      $$jsonFail("Exponent part is missing a number", i);
    }
    while ($$jsonIsDigit($$jsonAt(i))) {
      i++;
    }
  }
  $$jsonPos = i;
  if (simple) {
    return neg ? -v : v;
  }
  return $$StringToNumber(__intrinsic_sub(src, start, i));
}

// A string literal at the current position (a `"`).
function $$jsonParseString() {
  const src = $$jsonSrc;
  const len = $$jsonLen;
  const start = $$jsonPos + 1;
  // Fast path: no escapes.
  let i = start;
  for (;;) {
    if (i >= len) {
      $$jsonFail("Unterminated string", len);
    }
    const u = __intrinsic_unit(src, i);
    if (u === 34) {
      $$jsonPos = i + 1;
      return __intrinsic_sub(src, start, i);
    }
    if (u === 92) {
      break;
    }
    if (u < 32) {
      $$jsonFail("Bad control character in string literal", i);
    }
    i++;
  }
  // Validate the rest and count the result's length, then build it.
  let n = i - start;
  for (;;) {
    if (i >= len) {
      $$jsonFail("Unterminated string", len);
    }
    const u = __intrinsic_unit(src, i);
    if (u === 34) {
      break;
    }
    if (u < 32) {
      $$jsonFail("Bad control character in string literal", i);
    }
    if (u === 92) {
      if (i + 1 >= len) {
        $$jsonUnexpected(len);
      }
      const e = __intrinsic_unit(src, i + 1);
      if (e === 117) {
        for (let k = i + 2; k < i + 6; k++) {
          if ($$jsonHex($$jsonAt(k)) < 0) {
            $$jsonFail("Bad Unicode escape", k);
          }
        }
        i += 6;
      } else if (
        e === 34 ||
        e === 92 ||
        e === 47 ||
        e === 98 ||
        e === 102 ||
        e === 110 ||
        e === 114 ||
        e === 116
      ) {
        i += 2;
      } else {
        $$jsonFail("Bad escaped character", i + 1);
      }
    } else {
      i++;
    }
    n++;
  }
  const end = i;
  const r = __intrinsic_str_new(n);
  let j = 0;
  i = start;
  while (i < end) {
    let u = __intrinsic_unit(src, i);
    i++;
    if (u === 92) {
      const e = __intrinsic_unit(src, i);
      i++;
      if (e === 117) {
        u =
          ($$jsonHex(__intrinsic_unit(src, i)) << 12) |
          ($$jsonHex(__intrinsic_unit(src, i + 1)) << 8) |
          ($$jsonHex(__intrinsic_unit(src, i + 2)) << 4) |
          $$jsonHex(__intrinsic_unit(src, i + 3));
        i += 4;
      } else if (e === 98) {
        u = 8;
      } else if (e === 102) {
        u = 12;
      } else if (e === 110) {
        u = 10;
      } else if (e === 114) {
        u = 13;
      } else if (e === 116) {
        u = 9;
      } else {
        u = e;
      }
    }
    __intrinsic_str_set(r, j++, u);
  }
  $$jsonPos = end + 1;
  return r;
}

// The value of a hex digit, or -1.
function $$jsonHex(c) {
  if (c >= 48 && c <= 57) {
    return c - 48;
  }
  if (c >= 97 && c <= 102) {
    return c - 87;
  }
  if (c >= 65 && c <= 70) {
    return c - 55;
  }
  return -1;
}

function $$jsonParseArray() {
  const arr = [];
  $$jsonPos++;
  $$jsonSkipSpace();
  if ($$jsonAt($$jsonPos) === 93) {
    $$jsonPos++;
    return arr;
  }
  for (;;) {
    $$append(arr, $$jsonParseValue());
    $$jsonSkipSpace();
    const c = $$jsonAt($$jsonPos);
    if (c === 93) {
      $$jsonPos++;
      return arr;
    }
    if (c !== 44) {
      $$jsonFail("Expected ',' or ']' after array element", $$jsonPos);
    }
    $$jsonPos++;
  }
  return arr;
}

// A missing property name has one message for the first property, another after a comma.
function $$jsonParseObject() {
  const obj = {};
  $$jsonPos++;
  $$jsonSkipSpace();
  if ($$jsonAt($$jsonPos) === 125) {
    $$jsonPos++;
    return obj;
  }
  let first = true;
  for (;;) {
    if ($$jsonAt($$jsonPos) !== 34) {
      $$jsonFail(
        first ? "Expected property name or '}'" : "Expected double-quoted property name",
        $$jsonPos,
      );
    }
    const key = $$jsonParseString();
    $$jsonSkipSpace();
    if ($$jsonAt($$jsonPos) !== 58) {
      $$jsonFail("Expected ':' after property name", $$jsonPos);
    }
    $$jsonPos++;
    // CreateDataProperty: `__proto__` is an own key; a repeated key keeps its first position.
    __intrinsic_define(obj, key, $$jsonParseValue());
    $$jsonSkipSpace();
    const c = $$jsonAt($$jsonPos);
    if (c === 125) {
      $$jsonPos++;
      return obj;
    }
    if (c !== 44) {
      $$jsonFail("Expected ',' or '}' after property value", $$jsonPos);
    }
    $$jsonPos++;
    $$jsonSkipSpace();
    first = false;
  }
  return obj;
}

// InternalizeJSONProperty: the reviver walk, depth first; `undefined` deletes.
function $$jsonInternalize(holder, name, reviver) {
  const val = holder[name];
  if (typeof val === "object" && val !== null) {
    if (Array.isArray(val)) {
      const len = +val.length;
      for (let i = 0; i < len; i++) {
        $$jsonRevive(val, $$String(i), reviver);
      }
    } else {
      const keys = Object.keys(val);
      const n = +keys.length;
      for (let i = 0; i < n; i++) {
        $$jsonRevive(val, keys[i], reviver);
      }
    }
  }
  return reviver.call(holder, name, val);
}

function $$jsonRevive(val, key, reviver) {
  const v = $$jsonInternalize(val, key, reviver);
  if (v === undefined) {
    delete val[key];
  } else {
    __intrinsic_define(val, key, v);
  }
}
