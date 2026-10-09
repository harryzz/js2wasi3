"use strict";
// T96 micro-benchmark: the generic get/set on objects that aren't exactly $object (typed arrays, Buffers,
// arrays and functions with string keys), where wasmtime's is_subtype was called.
const n = Number(process.argv[2] || 2000000);
function time(name, f) { const t = Date.now(); const r = f(); console.log(name, r, Date.now() - t, "ms"); }
const buf = Buffer.alloc(64), u8 = new Uint8Array(32);
time("buf.length", () => { let s = 0; for (let i = 0; i < n; i++) s += buf.length; return s; });
time("u8.length", () => { let s = 0; for (let i = 0; i < n; i++) s += u8.length; return s; });
const arr = [1, 2, 3]; arr.extra = 5; const keys = ["extra", "length"];
time("arr[k]", () => { let s = 0; for (let i = 0; i < n; i++) s += arr[keys[i & 1]]; return s; });
function fn() {} fn.tag = 7; const fk = ["tag", "length"];
time("fn[k]", () => { let s = 0; for (let i = 0; i < n; i++) s += fn[fk[i & 1]]; return s; });
const wk = ["a", "b"]; const tgt = [];
time("arr[k]=v", () => { for (let i = 0; i < n; i++) tgt[wk[i & 1]] = i; return tgt.a + tgt.b; });
// ToPrimitive of non-plain objects (to_string / js_add / to_number): arrays, Maps, Dates.
const a3 = [1, 2], mp = new Map(), d = new Date(0);
time("''+arr", () => { let s = 0; for (let i = 0; i < n / 4; i++) s += ("" + a3).length; return s; });
time("String(map)", () => { let s = 0; for (let i = 0; i < n / 4; i++) s += String(mp).length; return s; });
time("+date", () => { let s = 0; for (let i = 0; i < n / 4; i++) s += +d; return s; });
