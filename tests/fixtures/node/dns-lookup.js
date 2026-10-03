// dns.lookup (T66): over wasi:sockets/ip-name-lookup; IP strings are given back without a lookup.
import dns from "node:dns";

const lookup = (...args) => new Promise((resolve) => dns.lookup(...args, (...r) => resolve(r)));

console.log(await lookup("localhost"));
console.log(await lookup("localhost", { all: true }));
console.log(await lookup("localhost", 4));
console.log(await lookup("127.0.0.1"));
console.log(await lookup("::1", { all: true }));
const [e] = await lookup("no-such-host.invalid");
console.log(e.message, e.code, e.errno, e.syscall, e.hostname, Object.keys(e).join(" "));
console.log(await dns.promises.lookup("localhost", { family: 4 }));
console.log(await dns.promises.lookup("10.1.2.3", { all: true }));
await dns.promises.lookup("no-such-host.invalid").catch((err) => console.log("rejected", err.code));
for (const f of [() => dns.lookup(1, () => {}), () => dns.lookup("x"), () => dns.lookup("x", 5, () => {})]) {
  try {
    f();
  } catch (err) {
    console.log(err.name, err.code, err.message);
  }
}
