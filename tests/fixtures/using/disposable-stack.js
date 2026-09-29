// DisposableStack and AsyncDisposableStack (T45).
const log = [];
const res = (name, fail) => ({ [Symbol.dispose]() { log.push("dispose " + name); if (fail) throw new Error(name); } });

const s = new DisposableStack();
console.log(s.use(res("a")).constructor === Object, s.use(null), s.adopt(7, (v) => log.push("adopt " + v)));
s.defer(() => log.push("defer"));
console.log(s.disposed, s.dispose(), s.disposed, s.dispose(), log.splice(0));

const t = new DisposableStack();
t.use(res("moved"));
const m = t.move();
console.log(t.disposed, m.disposed, m instanceof DisposableStack);
m[Symbol.dispose]();
console.log(log.splice(0));

const e = new DisposableStack();
e.use(res("x", true));
e.defer(() => { throw new Error("y"); });
try { e.dispose(); } catch (err) { console.log(err.name, err.error.message, err.suppressed.message, log.splice(0)); }

{
  using stack = new DisposableStack();
  stack.defer(() => log.push("block end"));
  log.push("in block");
}
console.log(log.splice(0));

console.log(String(s), Object.prototype.toString.call(new AsyncDisposableStack()), DisposableStack.length, DisposableStack.name);
try { DisposableStack(); } catch (err) { console.log(err.name + ": " + err.message); }
class Sub extends DisposableStack { extra() { return "sub"; } }
const sub = new Sub();
console.log(sub.extra(), sub instanceof DisposableStack, sub.disposed);

const a = new AsyncDisposableStack();
a.use({ async [Symbol.asyncDispose]() { log.push("async a"); } });
a.use(res("sync in async"));
a.use(null);
a.defer(async () => log.push("async defer"));
await a.disposeAsync();
console.log(a.disposed, log.splice(0));
{
  await using b = new AsyncDisposableStack();
  b.defer(() => log.push("await using stack"));
}
console.log(log.splice(0));
AsyncDisposableStack.prototype.disposeAsync.call({}).catch((err) => console.log(err.name + ": " + err.message));
