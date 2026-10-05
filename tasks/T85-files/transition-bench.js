// T85 micro-benchmark: objects built by the runtime and by literals, after the root shape has collected many
// first keys (as in a large program). Prints checksums (stdout, compared with node) and times (stderr).
// Usage: tasks/T85-files/bench.sh <compiler>...
function time(name, f) {
  const t = performance.now();
  const r = f();
  console.error(`${name} ${(performance.now() - t).toFixed(0)} ms`);
  console.log(name, r);
}

// A large program makes objects with many different first keys: fill the root shape's transitions.
const keep = [];
for (let i = 0; i < 200; i++) keep.push(JSON.parse(`{"k${i}": ${i}, "v": 1}`));
console.log("setup", keep.length);

// Array spread: one `{ value, done }` per element from the array iterator.
time("spread", () => {
  const a = [1, 2, 3, 4, 5, 6, 7, 8];
  let sum = 0;
  for (let i = 0; i < 40000; i++) sum += [...a].length;
  return sum;
});

// Generators: `{ value, done }` per step.
time("generator", () => {
  function* g(n) { for (let i = 0; i < n; i++) yield i; }
  let sum = 0;
  for (let i = 0; i < 20000; i++) for (const v of g(8)) sum += v;
  return sum;
});

// `arguments` objects.
time("arguments", () => {
  function f() { return arguments.length + arguments[0]; }
  let sum = 0;
  for (let i = 0; i < 200000; i++) sum += f(i & 7, 2, 3);
  return sum;
});

// Property descriptors and `Object.assign` (objects made by the runtime with several keys).
time("descriptors", () => {
  const src = { a: 1, b: 2, c: 3 };
  let sum = 0;
  for (let i = 0; i < 50000; i++) {
    const d = Object.getOwnPropertyDescriptor(src, "b");
    const o = Object.assign({}, src);
    sum += d.value + o.c;
  }
  return sum;
});

// Literals with first keys late in the root's list, as MQTT/log objects in zigbee2mqtt.
time("literals", () => {
  let sum = 0;
  for (let i = 0; i < 100000; i++) {
    const m = { topic: "t", payload: i & 15, qos: 0 };
    const l = { level: "info", message: "m", ts: i };
    sum += m.payload + l.ts;
  }
  return sum;
});
