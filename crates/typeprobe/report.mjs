// T131: joins `js2wasi3 types` (what the compiler predicts) with a `typeprobe --sites` run (what happened).
// Usage: node report.mjs <dir> name...   (<dir>/<name>.types, .tsv, .json)
// Per site and kind, executions are:
//   typed    — the analysis names one type (both operands, for operators): the compiler would emit a fast path;
//   verified — typed, and every execution's run-time type is that type (the guard would pass);
//   upper    — what one type per site could cover at best (the most common run-time type at each site).
import fs from 'fs';
const [dir, ...names] = process.argv.slice(2);
const CATS = ['get', 'set', 'method', 'eget', 'eset', 'egetkey', 'esetkey', 'call', 'arith', 'bit', 'compare', 'equal'];
const BIN = new Set(['arith', 'bit', 'compare', 'equal']);
const total = {}, rows = [];
const pct = (a, b) => b ? (100 * a / b).toFixed(1) + '%' : '';
const M = x => (x / 1e6).toFixed(1) + 'M';
for (const name of names) {
  const pred = new Map();
  for (const l of fs.readFileSync(`${dir}/${name}.types`, 'utf8').trim().split('\n')) {
    const [off, cat, g, t] = l.split('\t');
    const [gl, gr] = g.split('|').map(Number);
    const [tl, tr] = t.split('|');
    let typed = BIN.has(cat) ? gl === 1 && gr === 1 : gl === 1;
    const tag = BIN.has(cat) ? `${tl},${tr}` : tl;
    // `===`/`==` against null/undefined, or between references only, is one pointer compare: no type needed
    const refs = t => t.split(',').every(x => !['num', 'string', '?', '-'].includes(x));
    const nullish = t => t.split(',').every(x => x === 'null' || x === 'undefined');
    const any = cat === 'equal' && (nullish(tl) || nullish(tr) || (refs(tl) && refs(tr)));
    pred.set(`${off} ${cat}`, { typed: typed || any, tag, any });
  }
  const site = {};
  for (const l of fs.readFileSync(`${dir}/${name}.tsv`, 'utf8').trim().split('\n')) {
    const [id, cat, , off] = l.split('\t'); site[id] = [cat, off];
  }
  const acc = {};
  for (const [id, n, t] of JSON.parse(fs.readFileSync(`${dir}/${name}.json`, 'utf8'))) {
    const [cat, off] = site[id];
    const a = acc[cat] ??= { n: 0, typed: 0, verified: 0, upper: 0, missing: 0 };
    a.n += n;
    a.upper += Math.max(...Object.values(t));
    const p = pred.get(`${off} ${cat}`);
    if (!p) { a.missing += n; continue; }
    if (p.typed) { a.typed += n; a.verified += p.any ? n : t[p.tag] ?? 0; }
  }
  rows.push([name, acc]);
  for (const [c, a] of Object.entries(acc)) {
    const T = total[c] ??= { n: 0, typed: 0, verified: 0, upper: 0, missing: 0 };
    for (const k in a) T[k] += a[k];
  }
}
console.log('## All programs, by kind of operation\n');
console.log('| kind | executed | typed by the analysis | typed and right (guard passes) | best possible (one type per site) | sites the analysis never reached |');
console.log('|---|---|---|---|---|---|');
const all = { n: 0, typed: 0, verified: 0, upper: 0, missing: 0 };
for (const c of CATS) {
  const T = total[c]; if (!T) continue;
  for (const k in all) all[k] += T[k];
  console.log(`| ${c} | ${M(T.n)} | ${pct(T.typed, T.n)} | ${pct(T.verified, T.n)} | ${pct(T.upper, T.n)} | ${pct(T.missing, T.n)} |`);
}
console.log(`| **all** | ${M(all.n)} | ${pct(all.typed, all.n)} | ${pct(all.verified, all.n)} | ${pct(all.upper, all.n)} | ${pct(all.missing, all.n)} |`);
console.log('\n## Per program: typed and right / best possible (all kinds)\n');
console.log('| program | executed | typed | typed and right | best possible |');
console.log('|---|---|---|---|---|');
for (const [name, acc] of rows) {
  const s = { n: 0, typed: 0, verified: 0, upper: 0 };
  for (const a of Object.values(acc)) for (const k in s) s[k] += a[k];
  console.log(`| ${name} | ${M(s.n)} | ${pct(s.typed, s.n)} | ${pct(s.verified, s.n)} | ${pct(s.upper, s.n)} |`);
}
