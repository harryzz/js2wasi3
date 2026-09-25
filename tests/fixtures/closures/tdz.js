// A closure reading a `let`/`const` before its declaration throws a ReferenceError.
function early() {
  const read = () => value;
  try {
    read();
  } catch (e) {
    console.log(e.name, e.message);
  }
  let value = 42;
  return read();
}
console.log(early());

function hoistedRead() {
  function get() {
    return later;
  }
  let r;
  try {
    r = get();
  } catch (e) {
    r = e.message;
  }
  const later = "set";
  return r + " then " + get();
}
console.log(hoistedRead());

// Top level: a closure made before the declaration.
const peek = () => topLevel;
try {
  peek();
} catch (e) {
  console.log(e.message);
}
let topLevel = "ok";
console.log(peek());
