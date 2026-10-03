// `http`, `https`, `http2` (T65): node's shape, constants, `Agent`, header validation and the classes
// packages extend at load. Nothing here opens a socket.
import http from "node:http";
import https from "node:https";
import http2 from "node:http2";
import EventEmitter from "node:events";
import { Readable } from "node:stream";

console.log(Object.keys(http).sort().join(" "));
console.log(Object.keys(https).sort().join(" "));
console.log(Object.keys(http2).sort().join(" "));
console.log(http.METHODS.length, http.METHODS.slice(0, 5), http.STATUS_CODES[200], http.STATUS_CODES[418], http.STATUS_CODES[999]);
console.log(http.maxHeaderSize, http2.constants.NGHTTP2_SESSION_SERVER, http2.constants.HTTP2_HEADER_STATUS, http2.constants.HTTP_STATUS_OK);
console.log(Object.keys(http2.constants).length, http2.getDefaultSettings(), typeof http2.sensitiveHeaders);

const agent = new http.Agent({ keepAlive: true, maxSockets: 4 });
console.log(agent instanceof EventEmitter, agent.defaultPort, agent.protocol, agent.keepAlive, agent.maxSockets,
  agent.maxFreeSockets, agent.scheduling, agent.keepAliveMsecs, agent.totalSocketCount, http.Agent.defaultMaxSockets);
console.log(agent.getName({ host: "a.com", port: 8080, family: 4 }), agent.getName({}), agent.getName({ socketPath: "/s" }));
console.log(http.globalAgent instanceof http.Agent, http.globalAgent.keepAlive, http.globalAgent.options.timeout);
console.log(https.globalAgent instanceof https.Agent, https.globalAgent instanceof http.Agent, https.globalAgent.defaultPort,
  https.globalAgent.protocol, https.globalAgent.maxCachedSessions, new https.Agent({ maxCachedSessions: 3 }).maxCachedSessions);

http.validateHeaderName("x-ok");
http.validateHeaderValue("x-ok", "fine\tvalue");
for (const f of [() => http.validateHeaderName("bad header"), () => http.validateHeaderName(""),
  () => http.validateHeaderValue("x", undefined), () => http.validateHeaderValue("x", "a\nb")]) {
  try { f(); } catch (e) { console.log(e.name, e.code, e.message); }
}

// srvx and ws extend these at load.
class WebIncomingMessage extends http.IncomingMessage {}
class WebServerResponse extends http.ServerResponse {}
const req = new WebIncomingMessage(null);
console.log(req instanceof Readable, req.url, req.method, req.complete, req.httpVersion, req.headers, req.rawHeaders);
const res = new WebServerResponse({ method: "GET" });
console.log(res instanceof http.OutgoingMessage, res.statusCode, res.headersSent, res.writable);
res.setHeader("Content-Type", "text/plain");
res.appendHeader("Set-Cookie", "a=1");
res.appendHeader("set-cookie", "b=2");
console.log(res.getHeader("content-type"), res.getHeaders(), res.getHeaderNames(), res.getRawHeaderNames(), res.hasHeader("Set-Cookie"));
res.removeHeader("content-type");
console.log(res.getHeaderNames());
try { res.setHeader("x", undefined); } catch (e) { console.log(e.code); }
console.log(typeof http.Server, typeof http.ClientRequest, typeof http.createServer, typeof https.request, typeof http2.connect);
