// node's ERR_INVALID_ARG_TYPE errors carry a `code` and print it in their header (F67).
try {
  queueMicrotask(5);
} catch (e) {
  console.log(e.code, e.name, e.message, Object.keys(e), String(e), e instanceof TypeError);
}
try {
  setTimeout("x");
} catch (e) {
  console.log(`${e}`, e.code);
}
try {
  setInterval(null);
} catch (e) {
  console.log(e + "", e.code);
}
queueMicrotask({});
