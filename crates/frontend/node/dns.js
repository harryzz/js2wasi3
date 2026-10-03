// node's `dns` (T65): loads with node 24's error codes and flags; lookups throw (planned: F283, over
// `wasi:sockets/ip-name-lookup`). `getDefaultResultOrder`/`setDefaultResultOrder` work.
"use strict";

const data = require("node:internal/builtin_data");
const { unavailableError, fns } = require("node:internal/unavailable");

const WHY = "no name lookup over WASI sockets yet";
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

function api(module) {
  const out = {};
  fns(out, module, ["lookup", "lookupService"], WHY);
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
