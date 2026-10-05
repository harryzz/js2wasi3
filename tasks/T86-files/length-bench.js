// T86 micro-benchmark: `.length` of Buffers and typed arrays, directly and through Buffer methods (zigbee2mqtt's
// frame parsing and socket writes). Prints checksums (stdout, compared with node) and times (stderr).
// Usage: tasks/T86-files/bench.sh <compiler>...
function time(name, f) {
  const t = performance.now();
  const r = f();
  console.error(`${name} ${(performance.now() - t).toFixed(0)} ms`);
  console.log(name, r);
}

const buf = Buffer.from("fe0d4581a3b2c1d0e0f00112233445566778899aabbccddeeff", "hex");
const u8 = new Uint8Array(32);

time("length", () => {
  let sum = 0;
  for (let i = 0; i < 1000000; i++) sum += buf.length + u8.length;
  return sum;
});

time("read", () => {
  let sum = 0;
  for (let i = 0; i < 200000; i++) for (let j = 0; j < 8; j++) sum += buf.readUInt8(j) + buf.readUInt16LE(j * 2);
  return sum;
});

time("slice-copy", () => {
  const out = Buffer.alloc(64);
  let sum = 0;
  for (let i = 0; i < 100000; i++) {
    const s = buf.subarray(2, 10 + (i & 7));
    sum += s.copy(out, i & 31) + s.length;
  }
  return sum;
});

time("concat", () => {
  let sum = 0;
  for (let i = 0; i < 30000; i++) sum += Buffer.concat([buf, buf.subarray(0, i & 15)]).length;
  return sum;
});

// A stream state: `state.length` on a plain object (readable-stream's writeOrBuffer).
time("state", () => {
  const state = { objectMode: false, highWaterMark: 16384, length: 0, writing: false, corked: 0, sync: true,
    bufferedRequestCount: 0, needDrain: false, ending: false, ended: false, finished: false, destroyed: false };
  let n = 0;
  for (let i = 0; i < 300000; i++) {
    state.length += buf.length;
    if (state.length < state.highWaterMark) n++;
    else state.length = 0;
  }
  return n + state.length;
});
