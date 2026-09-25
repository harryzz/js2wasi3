// Errors thrown by the runtime: reading a property of null/undefined, TDZ.
function len(v) {
  return v.length;
}

function msg(v) {
  return v.message;
}

let values = 0;
for (let i = 0; i < 4; i++) {
  let v = i === 0 ? null : i === 1 ? undefined : i === 2 ? "abc" : 7;
  try {
    console.log("length:", len(v), "message:", msg(v));
  } catch (e) {
    console.log(e.name + ": " + e.message, typeof e);
  }
  values++;
}
console.log(values);

// TDZ in top-level code.
try {
  console.log(early);
} catch (e) {
  console.log("" + e);
}
let early = "initialized";
console.log(early);

// TDZ reached through a function called before the declaration (F22).
function readLate() {
  return late;
}
function writeLate(v) {
  late = v;
}
try {
  console.log(readLate());
} catch (e) {
  console.log("read:", e.name, e.message);
}
try {
  writeLate(5);
} catch (e) {
  console.log("write:", e.name, e.message);
}
const late0 = 1;
let late = 42;
console.log(readLate(), late0);
writeLate(43);
console.log(readLate());
