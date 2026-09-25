// Uncaught non-error values print their console.log form.
console.log("start");
function fail(n) {
  throw n * 2;
}
fail(21);
