// T20 slice 4b-2: lists (numeric ones as typed arrays), variants, params in memory.
export function sum(xs) {
  let t = 0;
  for (const x of xs) t += x;
  return t;
}
export function reverseBytes(b) {
  const out = new Uint8Array(b.length);
  for (let i = 0; i < b.length; i++) out[i] = b[b.length - 1 - i];
  return out;
}
export function bytesFromArray(n) {
  const out = [];
  for (let i = 0; i < n; i++) out.push(i * 100);
  return out;
}
export function byteInfo(b) {
  return `${b instanceof Uint8Array} ${b.length} ${b[0]}`;
}
export function words(s) {
  return s.split(" ");
}
export function joinWords(ws) {
  return ws.join("+");
}
export function items(xs) {
  return xs.map((x) => ({ name: x.name.toUpperCase(), tags: [...x.tags, "seen"] }));
}
export function matrix(n) {
  const rows = [];
  for (let i = 0; i < n; i++) {
    const row = new Int32Array(n);
    for (let j = 0; j < n; j++) row[j] = i === j ? -1 : i * n + j;
    rows.push(row);
  }
  return rows;
}
export function floats(xs) {
  return xs.map((x) => x * 2);
}
export function wide(xs) {
  return xs.map((x) => x + 1);
}
export function area(s) {
  switch (s.tag) {
    case "circle":
      return Math.round(Math.PI * s.val * s.val);
    case "rect":
      return s.val[0] * s.val[1];
    case "empty":
      return "val" in s ? -1 : 0;
    default:
      return s.val.length;
  }
}
export function shapes() {
  return [
    { tag: "circle", val: 1.5 },
    { tag: "rect", val: [2, 3] },
    { tag: "empty" },
    { tag: "named", val: "hex" },
  ];
}
export function grow(s) {
  if (s.tag === "circle") return { tag: "circle", val: s.val * 2 };
  throw `can't grow ${s.tag}`;
}
export function countMany(a, b, c) {
  return a.reduce((x, y) => x + y, 0) + b.length + c;
}
export function manyStrings(a, b, c, d, e, f, g, h, i) {
  return [a, b, c, d, e, f, g, h, i].join("");
}
export async function slowBytes(n) {
  await null;
  return new Uint8Array(n).fill(7);
}
export function badShape() {
  return { tag: "triangle" };
}
