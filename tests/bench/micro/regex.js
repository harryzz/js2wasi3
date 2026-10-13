// T114: regex calls by pattern kind (ns per call, the loop's own cost subtracted). Generic shapes, not any
// benchmark's patterns: how the start of a match is found (literal, class, alternation, anchor, a literal
// inside only) and what the matcher does (loops, captures, lazy, case folding), on long and short inputs.
"use strict";
let sink = 0;
function time(n, f) { let best = Infinity; for (let r = 0; r < 5; r++) { const t0 = Date.now(); f(n); best = Math.min(best, Date.now() - t0); } return best * 1e6 / n; }
function text(len, seed) {
  // Words of letters, digits and punctuation, deterministic.
  const parts = ["alpha", "beta", "gamma", "delta", "12", "345", "x-y", "foo.bar", "(a)", "k=v", "  ", ";", "Q"];
  let s = "", x = seed;
  while (s.length < len) { x = (x * 1103515245 + 12345) & 0x7fffffff; s += parts[x % parts.length] + " "; }
  return s.slice(0, len);
}
const long = text(2000, 7);                                     // no "needle", no "@", no "<"
const longEnd = long + "needle";
const mail = text(300, 3) + " ann@example.com " + text(300, 5);
const html = text(200, 9).replace(/ /g, "<b> ") + "</b>";
const short = ["k=v", "abc", "x=1;y=2", "", "q", "a=b=c"];
const loop = time(1000000, (n) => { for (let i = 0; i < n; i++) sink += i & 1; });
const cases = [
  ["literal prefix, match at the end (2k)", 4000, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += /needle/.exec(longEnd).index; }],
  ["literal prefix, no match (2k)", 4000, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += /needle/.test(long) ? 1 : 0; }],
  ["inner literal only, no match (2k)", 50, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += /(^|[^\\])"\/Item\((\d+)\)"/.test(long) ? 1 : 0; }],
  ["leading class loop \\d+px, no match (2k)", 500, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += /\d+px/.test(long) ? 1 : 0; }],
  ["alternation start, no match (2k)", 2000, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += /cat|dog|bird/.test(long) ? 1 : 0; }],
  ["anchored trim replace (2k)", 200, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += (" " + long + " ").replace(/^\s+|\s+$/g, "").length; }],
  ["captures (\\w+)@(\\w+)\\.com exec (600)", 250, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += /(\w+)@(\w+)\.com/.exec(mail)[2].length; }],
  ["case-insensitive literal /GAMMA/i (2k)", 20000, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += /GAMMA/i.exec(long).index; }],
  ["lazy <.*?> global replace (~1k)", 500, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += html.replace(/<.*?>/g, "").length; }],
  ["global replace /a/g, many matches (2k)", 250, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += long.replace(/a/g, "").length; }],
  ["split /[;\\s]+/ (2k)", 200, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += long.split(/[;\s]+/).length; }],
  ["short inputs: /=/ test", 40000, (n) => { for (let i = 0; i < n; i++) sink += i & 1, sink += /=/.test(short[i % 6]) ? 1 : 0; }],
  ["short inputs: /^(\\w+)=(\\w*)$/ exec", 20000, (n) => { for (let i = 0; i < n; i++) { sink += i & 1; const m = /^(\w+)=(\w*)$/.exec(short[i % 6]); sink += m === null ? 0 : m[1].length; } }],
];
console.log("loop", loop.toFixed(1));
for (const [name, n, f] of cases) console.log(name, (time(n, f) - loop).toFixed(1));
console.log(sink > 0);
