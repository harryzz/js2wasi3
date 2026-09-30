// node's `events` (T55): EventEmitter listeners, once/prepend, newListener, errorMonitor, unhandled
// 'error' events, subclasses, events.once, validation errors, printing.
import EventEmitter, { once, errorMonitor } from "node:events";
const e = new EventEmitter();
console.log(e);
e.on("x", (a, b) => console.log("x1", a, b));
e.prependListener("x", () => console.log("x0"));
e.once("x", function () { console.log("once", this === e); });
console.log(e.emit("x", 1, 2), e.emit("x", 3), e.listenerCount("x"), e.eventNames());
console.log(e.listeners("x").length, e.rawListeners("x").length);
e.on("newListener", (n) => console.log("newListener", n));
e.on("y", () => {});
e.removeAllListeners("x");
console.log(e.eventNames(), e.emit("x"));
class C extends EventEmitter { constructor() { super(); this.n = 1; } }
const c = new C();
c.on(errorMonitor, (err) => console.log("monitor", err.message));
try { c.emit("error", new Error("bad")); } catch (err) { console.log("thrown", err.message); }
try { c.emit("error", "str"); } catch (err) { console.log(err.code, err.message); }
c.on("error", (err) => console.log("handled", err.message));
c.emit("error", new Error("ok"));
console.log(c instanceof EventEmitter, C.name, c.getMaxListeners(), EventEmitter.defaultMaxListeners);
setTimeout(() => c.emit("ready", 42, "a"), 1);
const args = await once(c, "ready");
console.log("once resolved", args);
try { e.setMaxListeners(-1); } catch (err) { console.log(err.code, err.message); }
try { e.on("z", 5); } catch (err) { console.log(err.code, err.message); }
console.log(c);
