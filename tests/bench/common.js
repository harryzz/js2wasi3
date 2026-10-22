// A minimal stand-in for node's benchmark/common.js (the harness the files in this folder `require('../common.js')`
// from) so node's own file runs unchanged under node and under js2wasi3. Runs every
// combination of the configs one after the other, each after the previous `bench.end(n)`, and prints
// `<config>: <ns per operation>`.
'use strict';

exports.createBenchmark = function createBenchmark(fn, configs) {
  const keys = Object.keys(configs);
  const combos = keys.reduce((acc, k) => acc.flatMap((c) => configs[k].map((v) => ({ ...c, [k]: v }))), [{}]);
  let current;
  let t0;
  let next = 0;
  const bench = {
    start() { t0 = process.hrtime.bigint(); },
    end(n) {
      const ns = Number(process.hrtime.bigint() - t0) / n;
      console.log(keys.map((k) => `${k}=${JSON.stringify(current[k])}`).join(' ') + ': ' + ns.toFixed(0) + ' ns/op');
      setImmediate(run);
    },
  };
  function run() {
    if (next === combos.length) return;
    current = combos[next++];
    fn(current);
  }
  setImmediate(run);
  return bench;
};
