// `process.exit` (T55): the 'exit' listeners run, then nothing else: no `catch` or `finally`, no
// microtasks or timers; an `exit` in an 'exit' listener stops the others (the status is its code:
// js2wasi3 has only 0 and 1).
process.on("exit", (code) => {
  console.log("exit listener", code);
  process.exit(1);
});
process.on("exit", () => console.log("never: second listener"));
setTimeout(() => console.log("never: timer"), 0);
Promise.resolve().then(() => console.log("never: microtask"));
function f() {
  try {
    console.log("before exit");
    process.exit(0);
  } catch (e) {
    console.log("never: catch", e);
  } finally {
    console.log("never: finally");
  }
}
f();
console.log("never: after");
