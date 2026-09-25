// An uncaught error: output before it is kept, then "Uncaught TypeError: ..." and exit status 1.
console.log("before");
console.error("to stderr before");

function check(x) {
  if (x > 1) {
    throw new TypeError("bad value " + x);
  }
  return x;
}

check(1);
try {
  check(2);
} finally {
  console.log("finally runs before the error escapes");
}
console.log("not reached");
