// `tls`, `dns`, `dgram` (T65): node's shape and constants; nothing here touches the network.
import tls from "node:tls";
import dns from "node:dns";
import dgram from "node:dgram";
import EventEmitter from "node:events";
import { Duplex } from "node:stream";

console.log(Object.keys(tls).sort().join(" "));
console.log(tls.DEFAULT_ECDH_CURVE, tls.DEFAULT_MIN_VERSION, tls.DEFAULT_MAX_VERSION, tls.CLIENT_RENEG_LIMIT,
  tls.CLIENT_RENEG_WINDOW, tls.DEFAULT_CIPHERS.split(":").length, tls.DEFAULT_CIPHERS.slice(0, 22));
console.log(tls.getCiphers().length, tls.getCiphers().includes("aes128-gcm-sha256"), Array.isArray(tls.rootCertificates));
console.log(tls.TLSSocket.prototype instanceof Duplex, typeof tls.connect, typeof tls.Server);
const out = {};
tls.convertALPNProtocols(["h2", "http/1.1"], out);
console.log(out.ALPNProtocols);

console.log(Object.keys(dns).sort().join(" "));
console.log(Object.keys(dns.promises).sort().join(" "));
console.log(dns.NOTFOUND, dns.CONNREFUSED, dns.ADDRCONFIG, dns.V4MAPPED, dns.ALL, dns.promises.NOTFOUND);
console.log(dns.getDefaultResultOrder());
dns.setDefaultResultOrder("ipv4first");
console.log(dns.getDefaultResultOrder(), dns.promises.getDefaultResultOrder());
try { dns.setDefaultResultOrder("x"); } catch (e) { console.log(e.name, e.code, e.message); }
dns.setDefaultResultOrder("verbatim");

console.log(Object.keys(dgram).sort().join(" "));
console.log(Object.getPrototypeOf(dgram.Socket.prototype) === EventEmitter.prototype, typeof dgram.createSocket);
