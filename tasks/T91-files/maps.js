"use strict";
// T91 micro-benchmark: Map/Set with string keys built at run time (never identical to the stored key), and
// number keys to show no loss.
// Usage: node maps.js [case] [n]; case: hit | miss | churn | set | num | all (default all).
const which = process.argv[2] || "all";
const n = Number(process.argv[3] || 300000);

// 4096 distinct 8-character keys over "ACGT", rebuilt by substring each time.
const text = [];
for (let i = 0; i < 4096 + 8; i++) text.push("ACGT"[(i * 7 + (i >> 3) * 13) & 3]);
const seq = text.join("");
const K = 4096;

// get/set on hits: a counting loop like knucleotide's.
function hit() {
  const m = new Map();
  for (let i = 0; i < K; i++) m.set(seq.substring(i, i + 8), 0);
  let s = 0;
  for (let i = 0; i < n; i++) {
    const k = seq.substring(i % K, (i % K) + 8);
    const v = m.get(k);
    m.set(k, v + 1);
    s += v;
  }
  return s + m.size;
}

// has on misses: 9-character keys, none stored.
function miss() {
  const m = new Map();
  for (let i = 0; i < K; i++) m.set(seq.substring(i, i + 8), i);
  let s = 0;
  for (let i = 0; i < n; i++) if (m.has(seq.substring(i % K, (i % K) + 9))) s++;
  return s;
}

// deletes interleaved with inserts (holes, rehashes).
function churn() {
  const m = new Map();
  let s = 0;
  for (let i = 0; i < n; i++) {
    const j = i % K;
    m.set(seq.substring(j, j + 8) + (i & 1023), i);
    if (i >= 512) {
      const d = (i - 512) % K;
      if (m.delete(seq.substring(d, d + 8) + ((i - 512) & 1023))) s++;
    }
  }
  return s + m.size;
}

// Set add/has.
function set() {
  const st = new Set();
  let s = 0;
  for (let i = 0; i < n; i++) {
    const j = i % K;
    const k = seq.substring(j, j + 8);
    if (st.has(k)) s++;
    else st.add(k);
  }
  return s + st.size;
}

// number keys: integers and doubles.
function num() {
  const m = new Map();
  for (let i = 0; i < K; i++) m.set(i, i);
  for (let i = 0; i < K; i++) m.set(i + 0.5, i);
  let s = 0;
  for (let i = 0; i < n; i++) {
    s += m.get(i % K);
    s += m.get((i % K) + 0.5);
    if (m.has(i % K + K)) s++;
  }
  return s;
}

const cases = { hit, miss, churn, set, num };
for (const name of which === "all" ? Object.keys(cases) : [which]) {
  const t = Date.now();
  const r = cases[name]();
  console.log(name, r, Date.now() - t, "ms");
}
