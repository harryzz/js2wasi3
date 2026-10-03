// node's `dns` (T65): loads with node 24's error codes and flags. `lookup` (T66) is over
// `wasi:sockets/ip-name-lookup`; the resolver (DNS queries) throws. `getDefaultResultOrder`/
// `setDefaultResultOrder` work.
"use strict";

const data = require("node:internal/builtin_data");
const { unavailableError, fns } = require("node:internal/unavailable");
const { isIP, lookupAll } = require("node:internal/net");

const WHY = "no DNS queries over WASI sockets";
const RESOLVE = ["getServers", "resolve", "resolve4", "resolve6", "resolveAny", "resolveCaa", "resolveCname",
  "resolveMx", "resolveNaptr", "resolveNs", "resolvePtr", "resolveSoa", "resolveSrv", "resolveTlsa", "resolveTxt",
  "reverse"];

let order = "verbatim";

function getDefaultResultOrder() {
  return order;
}

function setDefaultResultOrder(value) {
  if (value !== "ipv4first" && value !== "ipv6first" && value !== "verbatim") {
    const e = new TypeError(`The argument 'dnsOrder' must be one of: 'verbatim', 'ipv4first', 'ipv6first'. Received ${typeof value === "string" ? `'${value}'` : String(value)}`);
    e.code = "ERR_INVALID_ARG_VALUE";
    throw e;
  }
  order = value;
}

// node's `lookup` arguments: `{ family, all, order }` (`hostname` checked).
function lookupOptions(hostname, options) {
  if (typeof hostname !== "string") throw $$errInvalidArgType("hostname", "of type string", hostname);
  let family = 0;
  let all = false;
  let lookupOrder = order;
  if (typeof options === "number") {
    family = options;
  } else if (options !== null && typeof options === "object") {
    if (options.family === "IPv4") family = 4;
    else if (options.family === "IPv6") family = 6;
    else if (options.family !== undefined) family = options.family;
    all = options.all === true;
    if (options.order !== undefined) lookupOrder = options.order;
    else if (options.verbatim === false) lookupOrder = "ipv4first";
  }
  if (family !== 0 && family !== 4 && family !== 6) {
    const e = new TypeError(`The argument 'family' must be one of: 0, 4, 6. Received ${String(family)}`);
    e.code = "ERR_INVALID_ARG_VALUE";
    throw e;
  }
  return { family, all, order: lookupOrder };
}

// What a lookup of `hostname` gives without asking the resolver: an IP, or nothing for "".
function lookupLocal(hostname, o) {
  if (!hostname) return o.all ? [] : { address: null, family: o.family === 6 ? 6 : 4 };
  const family = isIP(hostname);
  if (family === 0) return undefined;
  return o.all ? [{ address: hostname, family }] : { address: hostname, family };
}

async function lookupPromise(hostname, options) {
  const o = lookupOptions(hostname, options);
  const local = lookupLocal(hostname, o);
  if (local !== undefined) return local;
  const addresses = await lookupAll(hostname, o.family, o.order);
  return o.all ? addresses : addresses[0];
}

function lookup(hostname, options, callback) {
  if (typeof options === "function") {
    callback = options;
    options = undefined;
  }
  if (typeof callback !== "function") throw $$errInvalidArgType("callback", "of type function", callback);
  const o = lookupOptions(hostname, options);
  const local = lookupLocal(hostname, o);
  if (local !== undefined) {
    if (o.all) process.nextTick(callback, null, local);
    else process.nextTick(callback, null, local.address, local.family);
    return {};
  }
  lookupAll(hostname, o.family, o.order).then(
    (addresses) => {
      if (o.all) callback(null, addresses);
      else callback(null, addresses[0].address, addresses[0].family);
    },
    (err) => callback(err),
  );
  return {};
}

function api(module) {
  const out = {};
  out.lookup = module === "dns" ? lookup : lookupPromise;
  fns(out, module, ["lookupService"], WHY);
  out.Resolver = class Resolver {
    constructor() {
      throw unavailableError(`${module}.Resolver`, WHY);
    }
  };
  out.getDefaultResultOrder = getDefaultResultOrder;
  out.setDefaultResultOrder = setDefaultResultOrder;
  fns(out, module, ["setServers"], WHY);
  return out;
}

const promises = api("dns.promises");
Object.assign(promises, data.dnsCodes);
fns(promises, "dns.promises", RESOLVE, WHY);

module.exports = api("dns");
Object.assign(module.exports, data.dnsFlags, data.dnsCodes);
fns(module.exports, "dns", RESOLVE, WHY);
Object.defineProperty(module.exports, "promises", { configurable: true, enumerable: true, get: () => promises });
