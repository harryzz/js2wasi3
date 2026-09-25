// T09: strings. Exports take and return strings or numbers; strings.out holds node's results
// (strings as WIT prints them, "..." with escapes).

// Boundary: `name` is a string by usage, the result is a string.
export function greet(name) {
  return "hi " + name;
}

/**
 * @param {string} a
 * @param {string} b
 * @param {number} n
 */
export function joinWith(a, b, n) {
  return `${a}-${b}:${n}`;
}

export function len(s) {
  return s.length;
}

// Non-ASCII: é (2 bytes UTF-8), € (3), 😀 (4, a surrogate pair in UTF-16).
export function unicode() {
  const s = "é€😀";
  return s + s.length;
}

// A lone surrogate can't cross the boundary: it becomes U+FFFD.
export function lone() {
  return "a\uD800b";
}

// A value that may be a number or a string: `+` decides at run time.
function pick(i) {
  if (i === 0) return "7";
  if (i === 1) return 7;
  if (i === 2) return true;
  if (i === 3) return null;
  if (i === 4) return undefined;
  if (i === 5) return "";
  return 1.5;
}

const N = 7;

// ToString of every kind, through `+` with a string and through a template literal.
export function show() {
  let out = "";
  for (let i = 0; i < N; i++) out = out + "[" + pick(i) + `|${pick(i)}]`;
  return out;
}

// `+` on two values of unknown kind (number + number, string + number, ...), as one string.
export function mixedAdd() {
  let out = "";
  for (let i = 0; i < N; i++) {
    for (let j = 0; j < N; j++) out += pick(i) + pick(j) + ";";
  }
  return out;
}

// typeof, ===, ==, truthiness and .length on strings, as bits.
export function checks() {
  let r = 0;
  const a = "abc";
  const b = "ab" + "c";
  r = r * 2 + (a === b ? 1 : 0);
  r = r * 2 + (a !== "abd" ? 1 : 0);
  r = r * 2 + (a == b ? 1 : 0);
  r = r * 2 + (typeof a === "string" ? 1 : 0);
  r = r * 2 + (typeof pick(0) === "string" ? 1 : 0);
  r = r * 2 + (typeof pick(1) === "string" ? 0 : 1);
  r = r * 2 + ("" ? 0 : 1);
  r = r * 2 + (pick(5) ? 0 : 1);
  r = r * 2 + (pick(0) ? 1 : 0);
  r = r * 2 + (a.length === 3 ? 1 : 0);
  r = r * 2 + ("".length === 0 ? 1 : 0);
  r = r * 2 + (pick(0) === "7" ? 1 : 0);
  r = r * 2 + (pick(0) == null ? 0 : 1);
  r = r * 2 + (pick(3) == undefined ? 1 : 0);
  r = r * 2 + (typeof (a + 1) === "string" ? 1 : 0);
  return r;
}

export function typeofNames() {
  let out = "";
  for (let i = 0; i < N; i++) out += typeof pick(i) + ",";
  return out;
}

// Numbers into strings: integers, decimals, exponents, the i31 edge.
export function numbers(x) {
  return 1 + "," + -0 + "," + (0.1 + 0.2) + "," + 1e21 + "," + 1e-7 + "," + 1073741824 + "," + x / 3 + `,${-1.5e-10}`;
}
