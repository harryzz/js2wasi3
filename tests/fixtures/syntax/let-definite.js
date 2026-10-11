// T106: `let x;` written before every read stays a number; `let x;` in a loop is `undefined` again each pass.
function sum(n) { let i, s; s = 0; for (i = 0; i < n; i++) s += i * 0.5; return s; }
function branch(c) { let x; if (c) x = 1.5; return x; }
function perPass(n) {
  const out = [];
  for (let k = 0; k < n; k++) { let t; if (k % 2) t = k * 1.5; out.push(t); }
  return out;
}
function resetAfter(n) { let r = []; for (let k = 0; k < n; k++) { let t; r.push(typeof t); t = k; r.push(t); } return r; }
function closure() { let c; const f = () => c; c = 2.5; return f(); }
function block(c) { let out; { let v; v = c ? 3 : 4; out = v * 2; } return out; }
function sw(k) { let w; switch (k) { case 1: w = 1; break; default: w = 2.25; } return w; }
console.log(sum(10), branch(true), branch(false), perPass(4), resetAfter(2));
console.log(closure(), block(true), block(false), sw(1), sw(5));
