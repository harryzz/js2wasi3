// `module` and `perf_hooks` (T65): `createRequire` fails like node's for a missing package, `Module`
// objects and paths, and the `performance` object.
import Module from "node:module";
import { createRequire, isBuiltin, builtinModules } from "node:module";
import perf from "node:perf_hooks";

console.log(Object.keys(Module).sort().join(" "));
console.log(Module.Module === Module, builtinModules.length, builtinModules.includes("fs"), builtinModules.includes("node:test"));
console.log(isBuiltin("fs"), isBuiltin("node:fs"), isBuiltin("fs/promises"), isBuiltin("test"), isBuiltin("node:test"), isBuiltin("lodash"), isBuiltin(5));
console.log(Module._nodeModulePaths("/a/node_modules/b/c"), Module._nodeModulePaths("/"));
console.log(Module.wrap("x"), Module.wrapper.length);
const m = new Module("/a/b.js");
console.log(m, Object.keys(m));
const req = createRequire("/x/y.js");
console.log(typeof req, typeof req.resolve, req.main, Object.keys(req).join());
try { req("no-such-package-xyz"); } catch (e) { console.log(e.code, JSON.stringify(e.message), e.requireStack); }
try { req.resolve("no-such-package-xyz"); } catch (e) { console.log(e.code, JSON.stringify(e.message)); }
console.log(req.resolve.paths("lodash").slice(0, 2), req.resolve.paths("fs"));
try { createRequire("relative"); } catch (e) { console.log(e.name, e.code, e.message); }
try { createRequire(5); } catch (e) { console.log(e.name, e.code, e.message); }
console.log(typeof createRequire(new URL("file:///a/b.mjs")), typeof createRequire("file:///a/b.mjs"));
console.log(Module.getSourceMapsSupport(), Module.constants);

console.log(Object.keys(perf).sort().join(" "));
console.log(perf.performance === performance, typeof perf.performance.now(), perf.performance.now() >= 0);
console.log(perf.constants.NODE_PERFORMANCE_GC_MAJOR, typeof perf.PerformanceObserver, typeof perf.monitorEventLoopDelay);
