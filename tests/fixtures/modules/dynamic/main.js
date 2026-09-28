// Dynamic `import()` (T41 slice 4): the build finds the modules each `import()` can load (string
// literals, `const`s, `?:`, template and `+` patterns); at run time the specifier is resolved and
// looked up. Each `import()` here is awaited, so the order doesn't depend on node's loader ticks.
import { counter } from "./lib/counter.js";

const helper = "./lib/helper.js";
const first = await import(helper);
console.log(first.greet("dynamic"), first.loads);

// The same module again: the same namespace, evaluated once.
const again = await import("./lib/helper.js");
console.log(again === first, again.loads, Object.keys(again));

// A statically imported module: already evaluated.
const c = await import("./lib/counter.js");
console.log(c.counter === counter, c.counter);

// Template and `+` patterns: every matching file is in the build.
for (const lang of ["en", "fr", "de"]) {
  const m = await import(`./locales/${lang}.js`);
  console.log(lang, m.default, m.hello);
}
const name = "fr";
console.log((await import("./locales/" + name + ".js")).hello);

// `?:` and `||`.
const pick = (x) => import(x ? "./lib/a.js" : "./lib/b.js");
console.log((await pick(true)).v, (await pick(false)).v);

// JSON through `import()`, with the attribute.
const json = await import("./lib/data.json", { with: { type: "json" } });
console.log(json.default.items.length, Object.keys(json));

// A specifier known only at run time: the modules come from `--include` (main.flags).
// Errors are rejected promises, as in node.
for (const [spec, options] of [
  ["./locales/xx.js", undefined],
  ["some-package", undefined],
  ["./lib/data.json", undefined],
  ["./lib/a.js", { with: { type: "json" } }],
  ["./lib/a.js", { with: { type: "css" } }],
  ["./lib/a.js", 1],
]) {
  try {
    await import(spec, options);
  } catch (e) {
    console.log(e.name, e.code, e.message.replaceAll(import.meta.dirname, "DIR"));
  }
}

// A module that throws while evaluating: every `import()` of it rejects with the same error.
for (let i = 0; i < 2; i++) {
  try {
    await import("./lib/throws.js");
  } catch (e) {
    console.log("throws:", e.message);
  }
}

// Not awaited: the importer goes on first.
import("./lib/late.js").then((m) => console.log("late loaded", m.late));
console.log("main done");
