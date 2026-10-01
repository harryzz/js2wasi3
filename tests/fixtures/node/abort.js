// AbortController, AbortSignal, EventTarget, Event (T61): node globals from node:internal/abort.
const ac = new AbortController();
console.log(ac, ac.signal, ac.signal.aborted, ac.signal.reason, typeof AbortSignal, typeof EventTarget, typeof Event);
const order = [];
ac.signal.onabort = (e) => order.push("onabort " + e.type + " " + (e.target === ac.signal));
ac.signal.addEventListener("abort", (e) => order.push("listener " + e.type + " " + e.isTrusted), { once: true });
ac.signal.addEventListener("abort", { handleEvent(e) { order.push("handleEvent " + e.eventPhase); } });
ac.abort();
ac.abort("again");
console.log(order, ac.signal.aborted, ac.signal.reason.name, ac.signal.reason.message, ac.signal.reason.code, ac.signal.reason instanceof Error);
try { ac.signal.throwIfAborted(); } catch (e) { console.log("thrown", e === ac.signal.reason); }
const ac2 = new AbortController();
ac2.abort(new Error("why"));
console.log(ac2.signal.reason.message, AbortSignal.abort().reason.name, AbortSignal.abort(42).reason);
try { new AbortSignal(); } catch (e) { console.log(e.name, e.code, e.message); }
const t = new EventTarget();
const e = new Event("ping", { cancelable: true });
t.addEventListener("ping", (ev) => { ev.preventDefault(); console.log("got", ev.type, ev.cancelable, ev.defaultPrevented, ev.bubbles); });
console.log("dispatch", t.dispatchEvent(e), e.defaultPrevented, e.target === t, e.currentTarget);
class My extends EventTarget {}
const m = new My();
m.addEventListener("x", () => console.log("my x"));
m.dispatchEvent(new Event("x"));
console.log(e.type, e instanceof Event, Object.prototype.toString.call(ac), Object.prototype.toString.call(ac.signal), String(t));
const s = AbortSignal.timeout(5);
s.addEventListener("abort", () => console.log("timeout", s.reason.name, s.reason.message, s.reason.code));
const any = AbortSignal.any([ac2.signal, s]);
console.log("any", any.aborted, any.reason.message);
