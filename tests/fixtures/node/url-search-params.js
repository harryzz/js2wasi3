// `URLSearchParams` (T64): parsing, serializing, the methods, iteration, inspect.
const p = new URLSearchParams("?a=1&b=x+y&c=%20%zz&d&=e&a=2&&f=%E2%82%AC&g=%FF");
console.log([...p], p.size, p.toString());
console.log(p.get("a"), p.get("nope"), p.getAll("a"), p.has("d"), p.has("a", "2"), p.has("a", "3"));
p.set("a", "only"); p.append("h", "é &=+?#"); p.delete("b"); p.delete("c", "nope");
console.log(p.toString(), String(p));
p.sort(); console.log([...p.keys()], [...p.values()]);
for (const [k, v] of p.entries()) console.log(JSON.stringify(k), JSON.stringify(v));
p.forEach(function (v, k, self) { console.log(k, v, self === p, this.t); }, { t: "this" });
console.log(p);
console.log(new URLSearchParams(), new URLSearchParams(""), new URLSearchParams("?"));
console.log(new URLSearchParams({ x: 1, y: [2, 3], z: undefined }).toString());
console.log(new URLSearchParams([["k", "v"], ["k", "w"]]).toString(), new URLSearchParams(new Map([["m", "n"]])).toString());
console.log(new URLSearchParams(p).toString() === p.toString(), new URLSearchParams("a=\ud800").toString());
try { new URLSearchParams([["only one"]]); } catch (e) { console.log(e.name, e.code, e.message); }
try { new URLSearchParams([1]); } catch (e) { console.log(e.name, e.code, e.message); }
try { p.get(); } catch (e) { console.log(e.name, e.code, e.message); }
try { p.append("x"); } catch (e) { console.log(e.name, e.code, e.message); }
try { p.forEach(1); } catch (e) { console.log(e.name, e.code, e.message); }
const big = new URLSearchParams();
for (let i = 0; i < 8; i++) big.append("key" + (7 - i), "value-number-" + i);
console.log(big);
big.sort(); console.log(big.toString());
const it = big.keys(); it.next(); console.log(it.next(), it[Symbol.iterator]() === it, Object.prototype.toString.call(it));
console.log(Object.prototype.toString.call(p), URLSearchParams.length, Object.keys(URLSearchParams.prototype));
const many = new URLSearchParams();
for (let i = 120; i > 0; i--) many.append(String(i % 7), String(i));
many.sort(); console.log(many.toString().slice(0, 120));
