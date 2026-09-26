// An unhandled rejection with a non-error reason.
const p = Promise.resolve(1).then(() => {
  throw { code: 42 };
});
p.then(() => console.log("not reached"));
console.log("sync");
