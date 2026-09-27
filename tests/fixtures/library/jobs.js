// T20 (F65): a library runs the jobs a call queued before returning, and fails the call with a
// rejection nobody handled (as node would crash with it).
let log = "";
Promise.resolve().then(() => {
  log += "init job;";
});

/** @returns {string} */
export function afterInit() {
  return log;
}
/** @returns {string} */
export function call() {
  log += "call;";
  Promise.resolve(1).then((v) => {
    log += "job " + v + ";";
  });
  return log;
}
async function late() {
  await null;
  await null;
  throw new RangeError("late failure");
}
export function rejects() {
  late();
  return 1;
}
export function handled() {
  late().catch(() => {});
  return 2;
}
export function throwsInJob() {
  queueMicrotask(() => {
    throw new TypeError("job failed");
  });
  return 3;
}
export function rejectsInThen() {
  Promise.resolve().then(() => {
    throw "plain";
  });
  return 4;
}
