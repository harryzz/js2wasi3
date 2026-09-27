#!/usr/bin/env node
// test262 runner: builds each test with js2wasi3 (release) and runs it with `wasmtime run -Sp3`.
//
// Usage: node scripts/test262.mjs [options] <path | subset>...
//   A path is relative to test262/test (a directory or a file), e.g. `built-ins/Math/abs`;
//   `subset` is the agreed subset (SUBSET below). Options:
//     -j N          parallel jobs (default: half the logical CPUs: compiling is CPU-bound, and with
//                   hyperthreads -j nproc was no faster than -j nproc/2)
//     --timeout S   seconds per test, build + run (default 10)
//     --depth N     group the per-directory counts by the first N path components under test/ (default 3)
//     --update      merge this run's results into the baseline (tests/test262/baseline.json)
//     --list        print every test that didn't pass, with its reason
//     --causes N    print the N most common reasons per status (default 0: none)
//
// test262 itself is a sparse clone at the commit pinned in TOOLS.md: scripts/fetch-test262.sh.
//
// Each test is its frontmatter's harness (`print` shim, sta.js, assert.js, doneprintHandle.js for
// `flags: [async]`, then `includes`) followed by the test, compiled as one ES module (always strict).
// wasmtime runs without its compile cache (it would grow by ~1 MB per test), at opt-level 0 with the
// single-pass register allocator (compiling is most of a test's time; this halves it) and with
// one compile thread per test (the runner already runs tests in parallel).
// Results:
//   pass         sync: exit 0; async: `Test262:AsyncTestComplete` printed, exit 0;
//                negative parse/early: the build fails with an error that isn't "unsupported";
//                negative runtime: the uncaught error has the expected type
//   fail         anything else (the reason is the first line of the evidence)
//   unsupported  the build failed with `unsupported: …`
//   crash        the compiler panicked
//   timeout      build + run took longer than --timeout
//   skip         not runnable here: `noStrict`/`raw` (sloppy mode), `_FIXTURE` files, module resolution
//                (multi-file modules, F7), `$262` (host-defined test hooks)
// The run prints counts per directory, then every test whose status differs from the baseline
// (or that isn't in it). Exit 1 if a test that passed in the baseline doesn't pass now.
// Per-test details of the last run: out/test262/results.tsv (path, status, ms, reason).
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, statSync, readdirSync, existsSync } from "node:fs";
import { availableParallelism, tmpdir } from "node:os";
import { join, relative, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const T262 = join(ROOT, "test262");
const TESTS = join(T262, "test");
const HARNESS = join(T262, "harness");
const BIN = join(ROOT, "target/release/js2wasi3");
const BASELINE = join(ROOT, "tests/test262/baseline.json");
const OUT = join(ROOT, "out/test262");
const SUBSET = [
  "language/expressions",
  "language/statements",
  "built-ins/Array",
  "built-ins/String",
  "built-ins/Math",
  "built-ins/JSON",
  "built-ins/Promise",
  "built-ins/Map",
  "built-ins/Set",
];
const PRINT_SHIM = "function print(x) { console.log(x); }\n";

// ---- options
const args = process.argv.slice(2);
const opt = { jobs: Math.max(1, availableParallelism() >> 1), timeout: 10, depth: 3, update: false, list: false, causes: 0 };
const paths = [];
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a === "-j") opt.jobs = +args[++i];
  else if (a === "--timeout") opt.timeout = +args[++i];
  else if (a === "--depth") opt.depth = +args[++i];
  else if (a === "--update") opt.update = true;
  else if (a === "--list") opt.list = true;
  else if (a === "--causes") opt.causes = +args[++i];
  else if (a === "subset") paths.push(...SUBSET);
  else if (a.startsWith("-")) usage(`unknown option ${a}`);
  else paths.push(a.replace(/^test262\/test\//, "").replace(/\/$/, ""));
}
if (paths.length === 0) usage("no tests given");
if (!existsSync(TESTS)) usage("test262/ is missing: run scripts/fetch-test262.sh");

function usage(msg) {
  console.error(`${msg}\nusage: node scripts/test262.mjs [-j N] [--timeout S] [--depth N] [--update] [--list] [--causes N] <path | subset>...`);
  process.exit(2);
}

// ---- collect tests
function collect(rel, out) {
  const p = join(TESTS, rel);
  if (!existsSync(p)) usage(`no such test or directory: ${rel} (is it in the sparse checkout?)`);
  if (statSync(p).isDirectory()) {
    for (const e of readdirSync(p).sort()) collect(rel ? `${rel}/${e}` : e, out);
  } else if (rel.endsWith(".js")) {
    out.add(rel);
  }
}
const tests = new Set();
for (const p of paths) collect(p, tests);

// ---- frontmatter (the subset of YAML test262 uses: scalars, `[a, b]` lists, `- a` lists, one nested map)
function frontmatter(src) {
  const m = src.match(/\/\*---([\s\S]*?)---\*\//);
  const meta = { includes: [], flags: [], features: [], negative: null };
  if (!m) return meta;
  let key = null;
  for (const line of m[1].split("\n")) {
    let r;
    if ((r = line.match(/^(\w+):\s*(.*)$/))) {
      key = r[1];
      const v = r[2].trim();
      if (key === "negative") meta.negative = {};
      else if (v.startsWith("[")) meta[key] = v.slice(1, v.lastIndexOf("]")).split(",").map((s) => s.trim()).filter(Boolean);
      else if (Array.isArray(meta[key]) && v === "") meta[key] = [];
    } else if ((r = line.match(/^\s+-\s*(\S+)/)) && Array.isArray(meta[key])) {
      meta[key].push(r[1]);
    } else if ((r = line.match(/^\s+(\w+):\s*(\S+)/)) && key === "negative") {
      meta.negative[r[1]] = r[2];
    }
  }
  return meta;
}

const harnessCache = new Map();
function harness(name) {
  if (!harnessCache.has(name)) harnessCache.set(name, readFileSync(join(HARNESS, name), "utf8"));
  return harnessCache.get(name);
}

function skipReason(rel, src, meta) {
  if (/_FIXTURE\.js$/.test(rel)) return "fixture, not a test";
  if (meta.flags.includes("noStrict")) return "noStrict (sloppy mode only)";
  if (meta.flags.includes("raw")) return "raw (sloppy mode, no harness)";
  if (meta.negative?.phase === "resolution") return "module resolution (F7)";
  if (/\$262\b/.test(src)) return "$262 (host-defined hooks)";
  return null;
}

// ---- run one test
function run(cmd, argv, timeoutMs) {
  return new Promise((resolve) => {
    const child = spawn(cmd, argv, { stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "", stderr = "", timedOut = false;
    child.stdout.on("data", (d) => (stdout += d));
    child.stderr.on("data", (d) => (stderr += d));
    const timer = setTimeout(() => { timedOut = true; child.kill("SIGKILL"); }, timeoutMs);
    child.on("close", (code) => { clearTimeout(timer); resolve({ code, stdout, stderr, timedOut }); });
  });
}

const firstLine = (s) => (s.trim().split("\n")[0] ?? "").slice(0, 200);
// wasmtime's stderr for a trap: the trap and the innermost two frames; otherwise the uncaught
// error, joined onto one line (objects print over several).
function runError(stderr) {
  const trap = stderr.match(/wasm trap: (.*)/);
  if (trap) {
    const frames = [...stderr.matchAll(/^\s*\d+:\s+0x[0-9a-f]+ - [^!]*!(\S+)/gm)].map((m) => m[1]);
    return `trap: ${trap[1]}${frames.length ? ` in ${frames.slice(0, 2).join(" < ")}` : ""}`;
  }
  return stderr.trim().split("\n").slice(0, 4).map((l) => l.trim()).join(" ").slice(0, 200);
}
// Build diagnostics start with `Error: <file>:<line>:<col>: error: `; keep the message.
const diagnostic = (s) => firstLine(s.replace(/^Error: /, "").replace(/^\S+:\d+:\d+: error: /, ""));

async function one(rel, work) {
  const src = readFileSync(join(TESTS, rel), "utf8");
  const meta = frontmatter(src);
  const skip = skipReason(rel, src, meta);
  if (skip) return { status: "skip", reason: skip };

  const parts = [PRINT_SHIM, harness("sta.js"), harness("assert.js")];
  if (meta.flags.includes("async")) parts.push(harness("doneprintHandle.js"));
  for (const inc of meta.includes) parts.push(harness(inc));
  parts.push(src);
  const id = rel.replace(/[/.]/g, "_");
  const js = join(work, `${id}.js`), wasm = join(work, `${id}.wasm`);
  writeFileSync(js, parts.join("\n"));

  const deadline = Date.now() + opt.timeout * 1000;
  const b = await run(BIN, ["build", js, "-o", wasm], deadline - Date.now());
  const phase = meta.negative?.phase;
  const early = phase === "parse" || phase === "early";
  if (b.timedOut) return { status: "timeout", reason: "build" };
  if (b.code !== 0) {
    if (b.code === 101 || /panicked at/.test(b.stderr)) {
      const p = b.stderr.match(/panicked at (\S+?):?\n(.*)/);
      return { status: "crash", reason: p ? `panic: ${p[2]} at ${p[1]}` : firstLine(b.stderr) };
    }
    const msg = diagnostic(b.stderr);
    if (/unsupported: /.test(b.stderr)) {
      return { status: "unsupported", reason: firstLine(b.stderr.match(/unsupported: .*/)[0].replace(/ \(planned: .*\)$/, "")) };
    }
    if (early) return { status: "pass" };
    return { status: "fail", reason: `build error: ${msg}` };
  }
  if (early) return { status: "fail", reason: `expected a ${meta.negative.type} at build time, but it built` };

  const r = await run("wasmtime", ["run", "-Sp3", "-C", "cache=n", "-C", "parallel-compilation=n", "-O", "opt-level=0", "-O", "regalloc-algorithm=single-pass", wasm], deadline - Date.now());
  rmSync(wasm, { force: true });
  if (r.timedOut) return { status: "timeout", reason: "run" };
  const err = runError(r.stderr);
  if (phase === "runtime") {
    if (r.code !== 0 && new RegExp(`^Uncaught ${meta.negative.type}\\b`).test(err)) return { status: "pass" };
    return { status: "fail", reason: `expected an uncaught ${meta.negative.type}, got ${r.code === 0 ? "exit 0" : err}` };
  }
  if (meta.flags.includes("async")) {
    const fail = r.stdout.match(/Test262:AsyncTestFailure:.*/);
    if (fail) return { status: "fail", reason: firstLine(fail[0]) };
    if (r.code === 0 && r.stdout.includes("Test262:AsyncTestComplete")) return { status: "pass" };
    return { status: "fail", reason: r.code === 0 ? "async test never completed" : err || `exit ${r.code}` };
  }
  if (r.code === 0) return { status: "pass" };
  return { status: "fail", reason: err || `exit ${r.code}` };
}

// ---- main
execFileSync("cargo", ["build", "-q", "--release", "-p", "js2wasi3"], { cwd: ROOT, stdio: "inherit" });
const work = mkdtempSync(join(tmpdir(), "js2wasi3-test262-"));
const list = [...tests];
const results = new Map();
const t0 = Date.now();
let next = 0, done = 0;
async function worker() {
  while (next < list.length) {
    const rel = list[next++];
    const s = Date.now();
    let res;
    try {
      res = await one(rel, work);
    } catch (e) {
      res = { status: "crash", reason: `runner: ${e.message}` };
    }
    res.ms = Date.now() - s;
    results.set(rel, res);
    if (++done % 200 === 0 && process.stderr.isTTY) process.stderr.write(`\r${done}/${list.length}`);
  }
}
await Promise.all(Array.from({ length: opt.jobs }, worker));
if (process.stderr.isTTY) process.stderr.write("\r\x1b[K");
rmSync(work, { recursive: true, force: true });

const STATUSES = ["pass", "fail", "unsupported", "crash", "timeout", "skip"];
const groups = new Map();
for (const [rel, r] of results) {
  const g = dirname(rel).split("/").slice(0, opt.depth).join("/");
  if (!groups.has(g)) groups.set(g, Object.fromEntries(STATUSES.map((s) => [s, 0])));
  groups.get(g)[r.status]++;
}
const total = Object.fromEntries(STATUSES.map((s) => [s, 0]));
const pct = (c) => {
  const ran = c.pass + c.fail + c.unsupported + c.crash + c.timeout;
  return ran ? `${((100 * c.pass) / ran).toFixed(1)}%` : "-";
};
console.log(`${"directory".padEnd(56)} ${STATUSES.map((s) => s.padStart(6).slice(0, 6)).join(" ")}   pass%`);
for (const g of [...groups.keys()].sort()) {
  const c = groups.get(g);
  for (const s of STATUSES) total[s] += c[s];
  console.log(`${g.padEnd(56)} ${STATUSES.map((s) => String(c[s]).padStart(6)).join(" ")}  ${pct(c).padStart(6)}`);
}
console.log(`${"total".padEnd(56)} ${STATUSES.map((s) => String(total[s]).padStart(6)).join(" ")}  ${pct(total).padStart(6)}`);
console.log(`(pass% = pass / (tests run, skips excluded); ${list.length} tests in ${((Date.now() - t0) / 1000).toFixed(0)} s, -j ${opt.jobs})`);

if (opt.list) {
  console.log("\nNot passing:");
  for (const [rel, r] of [...results].sort()) if (r.status !== "pass" && r.status !== "skip") console.log(`${r.status.padEnd(11)} ${rel}: ${r.reason}`);
}
if (opt.causes > 0) {
  for (const s of ["unsupported", "fail", "crash"]) {
    const counts = new Map();
    for (const r of results.values()) if (r.status === s) {
      const k = r.reason.replace(/«[^»]*»/g, "«…»").replace(/'[^']*'/g, "'…'");
      counts.set(k, (counts.get(k) ?? 0) + 1);
    }
    if (counts.size === 0) continue;
    console.log(`\nTop ${s} reasons:`);
    for (const [k, n] of [...counts].sort((a, b) => b[1] - a[1]).slice(0, opt.causes)) console.log(`${String(n).padStart(6)}  ${k}`);
  }
}

// ---- baseline
const baseline = existsSync(BASELINE) ? JSON.parse(readFileSync(BASELINE, "utf8")) : {};
const changes = [];
let regressions = 0;
for (const [rel, r] of [...results].sort()) {
  const old = baseline[rel];
  if (old === r.status) continue;
  if (old === "pass") regressions++;
  changes.push(`${(old ?? "new").padEnd(11)} -> ${r.status.padEnd(11)} ${rel}${r.reason ? `: ${r.reason}` : ""}`);
}
if (Object.keys(baseline).length === 0) {
  console.log("\nNo baseline yet (--update writes one).");
} else if (changes.length > 0) {
  console.log(`\nChanged since the baseline (${changes.length}, ${regressions} regressions):`);
  for (const c of changes) console.log(c);
} else {
  console.log("\nNo changes since the baseline.");
}

mkdirSync(OUT, { recursive: true });
writeFileSync(join(OUT, "results.tsv"), [...results].sort().map(([rel, r]) => `${rel}\t${r.status}\t${r.ms}\t${r.reason ?? ""}`).join("\n") + "\n");
if (opt.update) {
  for (const [rel, r] of results) baseline[rel] = r.status;
  const keys = Object.keys(baseline).sort();
  mkdirSync(dirname(BASELINE), { recursive: true });
  writeFileSync(BASELINE, `{\n${keys.map((k) => `  ${JSON.stringify(k)}: ${JSON.stringify(baseline[k])}`).join(",\n")}\n}\n`);
  console.log(`Baseline updated: ${relative(ROOT, BASELINE)} (${keys.length} tests)`);
}
process.exit(regressions > 0 ? 1 : 0);
