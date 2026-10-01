// WeakMap and WeakSet (T61): node globals from node:internal/weak (bound by the linker).
class A { #m = 1; static has(o) { return #m in o; } get(o) { return o.#m; } }
console.log(A.has(new A()), A.has({}));
try { new A().get({}); } catch (e) { console.log(e.name, e.message); }
const wm = new WeakMap(); const k = {};
console.log(String(wm), Object.prototype.toString.call(wm), wm.set(k, 1) === wm, wm.get(k), wm.has({}), wm);
try { wm.set(1, 2) } catch (e) { console.log(e.message) }
try { new WeakSet().add("x") } catch (e) { console.log(e.message) }
try { WeakMap.prototype.get.call({}, k) } catch (e) { console.log(e.message) }
try { new WeakMap([1]) } catch (e) { console.log(e.message) }
console.log(new WeakSet([k]).has(k), WeakMap.length, WeakMap.name, typeof WeakMap.prototype.delete, wm.delete(k), wm.delete(k), new WeakSet());
console.log(wm.set(Symbol("s"), 1) === wm); try { wm.set(Symbol.for("r"), 1) } catch (e) { console.log(e.message) }
console.log(Object.getOwnPropertyNames(WeakMap.prototype), Object.getOwnPropertyDescriptor(WeakMap.prototype, Symbol.toStringTag));
class Cache extends WeakMap {
  remember(k) {
    return this.set(k, (this.get(k) ?? 0) + 1);
  }
}
const c = new Cache([[k, 5]]);
console.log(c.remember(k).get(k), c instanceof WeakMap, c, Object.prototype.toString.call(new WeakSet()));
console.log([new WeakMap(), new WeakSet()], { w: new WeakMap() });
const seen = new WeakSet();
const visit = (o) => (seen.has(o) ? "again" : (seen.add(o), "first"));
const o1 = {};
console.log(visit(o1), visit(o1), visit({}));
