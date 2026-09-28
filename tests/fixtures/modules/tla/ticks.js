let n = 0;
function tick() {
  n++;
  console.log("tick", n);
  if (n < 8) Promise.resolve().then(tick);
}
Promise.resolve().then(tick);
