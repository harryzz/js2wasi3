// What `net` and `dns` share (T66): node's `isIP` (lib/internal/net.js), IP addresses as strings and
// as `wasi:sockets` values, node's errors for `wasi:sockets` error codes, and name lookup over
// `wasi:sockets/ip-name-lookup`.
"use strict";

const wasi = require("node:internal/wasi_sockets");

// IPv4 Segment
const v4Seg = "(?:25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9][0-9]|[0-9])";
const v4Str = `(?:${v4Seg}\\.){3}${v4Seg}`;
const IPv4Reg = new RegExp(`^${v4Str}$`);

// IPv6 Segment
const v6Seg = "(?:[0-9a-fA-F]{1,4})";
const IPv6Reg = new RegExp("^(?:" +
  `(?:${v6Seg}:){7}(?:${v6Seg}|:)|` +
  `(?:${v6Seg}:){6}(?:${v4Str}|:${v6Seg}|:)|` +
  `(?:${v6Seg}:){5}(?::${v4Str}|(?::${v6Seg}){1,2}|:)|` +
  `(?:${v6Seg}:){4}(?:(?::${v6Seg}){0,1}:${v4Str}|(?::${v6Seg}){1,3}|:)|` +
  `(?:${v6Seg}:){3}(?:(?::${v6Seg}){0,2}:${v4Str}|(?::${v6Seg}){1,4}|:)|` +
  `(?:${v6Seg}:){2}(?:(?::${v6Seg}){0,3}:${v4Str}|(?::${v6Seg}){1,5}|:)|` +
  `(?:${v6Seg}:){1}(?:(?::${v6Seg}){0,4}:${v4Str}|(?::${v6Seg}){1,6}|:)|` +
  `(?::(?:(?::${v6Seg}){0,5}:${v4Str}|(?::${v6Seg}){1,7}|:))` +
")(?:%[0-9a-zA-Z-.:]{1,})?$");

function isIPv4(s) {
  return IPv4Reg.test(s);
}

function isIPv6(s) {
  return IPv6Reg.test(s);
}

function isIP(s) {
  if (isIPv4(s)) return 4;
  if (isIPv6(s)) return 6;
  return 0;
}

// An `ip-address` (`{tag: "ipv4", val: [a, b, c, d]}` or eight 16-bit pieces) as node shows it.
function formatIP(ip) {
  const t = ip.val;
  if (ip.tag === "ipv4") return `${t[0]}.${t[1]}.${t[2]}.${t[3]}`;
  if (t[0] === 0 && t[1] === 0 && t[2] === 0 && t[3] === 0 && t[4] === 0 && t[5] === 0xffff) {
    return `::ffff:${t[6] >> 8}.${t[6] & 255}.${t[7] >> 8}.${t[7] & 255}`;
  }
  // The longest run of two or more zero pieces becomes `::` (RFC 5952).
  let best = -1;
  let bestLen = 1;
  for (let i = 0; i < 8;) {
    if (t[i] !== 0) {
      i++;
      continue;
    }
    let j = i;
    while (j < 8 && t[j] === 0) j++;
    if (j - i > bestLen) {
      best = i;
      bestLen = j - i;
    }
    i = j;
  }
  const hex = (a, b) => t.slice(a, b).map((x) => x.toString(16)).join(":");
  if (best < 0) return hex(0, 8);
  return `${hex(0, best)}::${hex(best + bestLen, 8)}`;
}

// The `ip-address` of an IP string (`isIP(s)` is 4 or 6).
function parseIP(s) {
  if (isIPv4(s)) return { tag: "ipv4", val: s.split(".").map(Number) };
  const zone = s.indexOf("%");
  if (zone >= 0) s = s.slice(0, zone);
  let tail = [];
  const v4 = s.lastIndexOf(".");
  if (v4 >= 0) {
    const at = s.lastIndexOf(":") + 1;
    const b = s.slice(at).split(".").map(Number);
    tail = [(b[0] << 8) | b[1], (b[2] << 8) | b[3]];
    s = s.slice(0, at) + "0";
  }
  const pieces = (part) => (part === "" ? [] : part.split(":").map((x) => parseInt(x, 16)));
  const gap = s.indexOf("::");
  let out;
  if (gap < 0) {
    out = pieces(s);
  } else {
    const head = pieces(s.slice(0, gap));
    const rest = pieces(s.slice(gap + 2));
    out = head.concat(new Array(8 - head.length - rest.length).fill(0), rest);
  }
  if (tail.length) out.splice(6, 2, tail[0], tail[1]);
  return { tag: "ipv6", val: out };
}

// The `ip-socket-address` of an IP string and a port.
function socketAddress(address, port) {
  const ip = parseIP(address);
  if (ip.tag === "ipv4") return { tag: "ipv4", val: { port, address: ip.val } };
  return { tag: "ipv6", val: { port, flowInfo: 0, address: ip.val, scopeId: 0 } };
}

// An `ip-socket-address` as node's `{ address, family, port }`.
function addressInfo(sa) {
  return {
    address: formatIP({ tag: sa.tag, val: sa.val.address }),
    family: sa.tag === "ipv4" ? "IPv4" : "IPv6",
    port: sa.val.port,
  };
}

// `wasi:sockets` error codes as libuv's (code, errno, description).
const CODES = {
  "access-denied": ["EACCES", -13, "permission denied"],
  "not-supported": ["ENOTSUP", -95, "operation not supported on socket"],
  "invalid-argument": ["EINVAL", -22, "invalid argument"],
  "out-of-memory": ["ENOMEM", -12, "not enough memory"],
  "timeout": ["ETIMEDOUT", -110, "connection timed out"],
  "invalid-state": ["EINVAL", -22, "invalid argument"],
  "address-not-bindable": ["EADDRNOTAVAIL", -99, "address not available"],
  "address-in-use": ["EADDRINUSE", -98, "address already in use"],
  "remote-unreachable": ["EHOSTUNREACH", -113, "host is unreachable"],
  "connection-refused": ["ECONNREFUSED", -111, "connection refused"],
  "connection-broken": ["EPIPE", -32, "broken pipe"],
  "connection-reset": ["ECONNRESET", -104, "connection reset by peer"],
  "connection-aborted": ["ECONNABORTED", -103, "software caused connection abort"],
  "datagram-too-large": ["EMSGSIZE", -90, "message too long"],
  "other": ["EIO", -5, "i/o error"],
};

// The `error-code` of what a `wasi:sockets` call threw (or a failed `result`), as a string.
function wasiCode(e) {
  const p = e !== null && typeof e === "object" && "payload" in e ? e.payload : e;
  if (p !== null && typeof p === "object") return p.tag;
  return typeof p === "string" ? p : "other";
}

// node's `ExceptionWithHostPort` (`connect ECONNREFUSED 127.0.0.1:1`); `uv`: libuv's form
// (`listen EADDRINUSE: address already in use 127.0.0.1:80`).
function errnoError(e, syscall, address, port, uv) {
  const [code, errno, description] = CODES[wasiCode(e)] || CODES.other;
  let details = "";
  if (address !== undefined) details = port ? ` ${address}:${port}` : ` ${address}`;
  const err = new Error(uv ? `${syscall} ${code}: ${description}${details}` : `${syscall} ${code}${details}`);
  err.errno = errno;
  err.code = code;
  err.syscall = syscall;
  if (address !== undefined) err.address = address;
  if (port) err.port = port;
  return err;
}

// Lookup errors: node's `DNSException` (`getaddrinfo ENOTFOUND example.invalid`).
const DNS_CODES = {
  "name-unresolvable": ["ENOTFOUND", -3008],
  "temporary-resolver-failure": ["EAI_AGAIN", -3001],
  "permanent-resolver-failure": ["EAI_FAIL", -3004],
  "access-denied": ["EAI_FAIL", -3004],
  "invalid-argument": ["ENOTFOUND", -3008],
  "other": ["EAI_FAIL", -3004],
};

function dnsError(e, hostname) {
  const [code, errno] = DNS_CODES[wasiCode(e)] || DNS_CODES.other;
  const err = new Error(`getaddrinfo ${code} ${hostname}`);
  err.errno = errno;
  err.code = code;
  err.syscall = "getaddrinfo";
  err.hostname = hostname;
  return err;
}

// A promise of `hostname`'s addresses (`[{ address, family }]`), of `family` (4, 6 or 0: any) in
// `order` ("verbatim", "ipv4first", "ipv6first"). Rejects with a `dnsError`.
async function lookupAll(hostname, family, order) {
  let ips;
  try {
    ips = await wasi.resolveAddresses(hostname);
  } catch (e) {
    throw dnsError(e, hostname);
  }
  let out = ips.map((ip) => ({ address: formatIP(ip), family: ip.tag === "ipv4" ? 4 : 6 }));
  if (family === 4 || family === 6) out = out.filter((x) => x.family === family);
  if (out.length === 0) throw dnsError("name-unresolvable", hostname);
  if (order === "ipv4first") out.sort((a, b) => a.family - b.family);
  else if (order === "ipv6first") out.sort((a, b) => b.family - a.family);
  return out;
}

module.exports = {
  isIP, isIPv4, isIPv6, formatIP, parseIP, socketAddress, addressInfo, wasiCode, errnoError, dnsError,
  lookupAll,
};
