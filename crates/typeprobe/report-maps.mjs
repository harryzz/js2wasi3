// node report.mjs dir name... : per program and category, executed operations and the share that one type per site covers
import fs from 'fs';
const [dir, ...names] = process.argv.slice(2);
const CATS = ['get', 'set', 'method', 'eget', 'eset', 'egetkey', 'esetkey', 'call', 'arith', 'bit', 'compare', 'equal'];
const coarse = k => k.replace(/\bint\b|\bdouble\b/g, 'num');
const total = {};
const rows = [];
for (const name of names) {
  const cat = {};
  for (const l of fs.readFileSync(`${dir}/${name}.tsv`, 'utf8').trim().split('\n')) {
    const [id, c] = l.split('\t'); cat[id] = c;
  }
  const data = JSON.parse(fs.readFileSync(`${dir}/${name}.json`, 'utf8'));
  const acc = {};
  for (const [id, n, t] of data) {
    const c = cat[id];
    const a = acc[c] ??= { n: 0, top: 0, topc: 0, mono: 0 };
    const counts = Object.values(t);
    const cc = {};
    for (const [k, v] of Object.entries(t)) cc[coarse(k)] = (cc[coarse(k)] || 0) + v;
    a.n += n;
    a.top += Math.max(...counts);
    a.topc += Math.max(...Object.values(cc));
    if (counts.length === 1) a.mono += n;
  }
  rows.push([name, acc]);
  for (const [c, a] of Object.entries(acc)) {
    const T = total[c] ??= { n: 0, top: 0, topc: 0, mono: 0, progs: [] };
    for (const k of ['n', 'top', 'topc', 'mono']) T[k] += a[k];
    T.progs.push(a.n ? a.top / a.n : 1);
  }
}
const pct = x => (100 * x).toFixed(1) + '%';
const M = x => (x / 1e6).toFixed(1) + 'M';
console.log('## All 18 programs, by kind of operation\n');
console.log('| kind | executed | one type per site (exact) | covered by the most common type at each site | same, numbers as one type | lowest program |');
console.log('|---|---|---|---|---|---|');
for (const c of CATS) {
  const T = total[c]; if (!T) continue;
  console.log(`| ${c} | ${M(T.n)} | ${pct(T.mono / T.n)} | ${pct(T.top / T.n)} | ${pct(T.topc / T.n)} | ${pct(Math.min(...T.progs))} |`);
}
console.log('\n## Per program: share covered by the most common type at each site\n');
console.log('| program | ' + CATS.join(' | ') + ' |');
console.log('|---|' + CATS.map(() => '---').join('|') + '|');
for (const [name, acc] of rows) {
  console.log(`| ${name} | ` + CATS.map(c => acc[c] ? `${pct(acc[c].top / acc[c].n)} (${M(acc[c].n)})` : '').join(' | ') + ' |');
}
