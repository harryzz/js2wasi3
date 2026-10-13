// T116: run a WASI 0.3 command component transpiled by jco (`scripts/v8bench.sh`) on node's V8, with
// preview3-shim as the host (as jco's own P3 test runner, packages/jco-transpile/test/p3/cli-runner.js).
//
// Usage: node v8run.mjs <transpiled.js> [args...]
// The working directory is preopened as `/` (the benchmarks read input.fa there, as under `wasmtime --dir`).
// With V8RUN_TIMES=<file>: appends "init_ms run_ms" (import + compile + instantiate, then `run()`).
// Resolves '@bytecodealliance/preview3-shim' from its own directory: copy it next to a node_modules with jco's
// packages (v8bench.sh does).
import process from 'node:process';
import { appendFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { performance } from 'node:perf_hooks';
import {
  _setArgs,
  _setCwd,
  _setTerminalStdin,
  _setTerminalStdout,
  _setTerminalStderr,
} from '@bytecodealliance/preview3-shim/cli';
import { _setPreopens } from '@bytecodealliance/preview3-shim/filesystem';

const [, , modPath, ...args] = process.argv;
_setArgs([modPath, ...args]);
_setCwd(null);
_setTerminalStdin(null);
_setTerminalStdout(null);
_setTerminalStderr(null);
_setPreopens({ '/': process.cwd() });

const t0 = performance.now();
const m = await import(pathToFileURL(modPath).href);
if (m.$init) await m.$init;
const t1 = performance.now();
const iface = m['wasi:cli/run@0.3.1'] ?? m.run;
try {
  await iface.run();
} catch (e) {
  console.error(e?.stack ?? e);
  process.exitCode = 1;
}
const t2 = performance.now();
if (process.env.V8RUN_TIMES) appendFileSync(process.env.V8RUN_TIMES, `${(t1 - t0).toFixed(1)} ${(t2 - t1).toFixed(1)}\n`);
