// node's `perf_hooks` (T65): `performance` is the global (`now()`, `timeOrigin`); the entry classes, observers,
// histograms and event-loop measurements throw (planned: F285).
"use strict";

const data = require("node:internal/builtin_data");
const { unavailableError, fns } = require("node:internal/unavailable");

const WHY = "no performance timeline in js2wasi3";

module.exports = {};
for (const name of ["Performance", "PerformanceEntry", "PerformanceMark", "PerformanceMeasure", "PerformanceObserver",
  "PerformanceObserverEntryList", "PerformanceResourceTiming"]) {
  const C = class {
    constructor() {
      throw unavailableError(`perf_hooks.${name}`, WHY);
    }
  };
  Object.defineProperty(C, "name", { value: name });
  module.exports[name] = C;
}
fns(module.exports, "perf_hooks", ["monitorEventLoopDelay", "eventLoopUtilization", "timerify", "createHistogram"], WHY);
module.exports.performance = performance;
module.exports.constants = { ...data.perfConstants };
