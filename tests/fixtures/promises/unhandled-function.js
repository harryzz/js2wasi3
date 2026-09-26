// An unhandled rejection whose reason is a function: node prints its source (V8's
// NoSideEffectsToString). Sources are kept because the code names `toString` (F104).
const show = (f) => f.toString().length;
console.log(show(show));
Promise.reject(function foo(a) {
  return a + 1;
});
