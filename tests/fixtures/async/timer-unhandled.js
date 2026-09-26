// A rejection left unhandled after a timer's callback and its jobs ends the program before the
// next timer.
setTimeout(() => console.log("never runs"), 30);
setTimeout(async () => {
  console.log("async callback");
  await null;
  throw new RangeError("rejected in a timer");
}, 5);
