// `process.exitCode` decides the exit status when the program ends (T55); the 'exit' listeners see
// it and can change it. (js2wasi3's status is 0 or 1: any other code is 1.)
process.on("exit", (code) => {
  console.log("exit", code);
  process.exitCode = 1;
});
process.exitCode = 0;
setTimeout(() => console.log("timer ran"), 5);
console.log("end of main");
