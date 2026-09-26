// `switch`: matching, fall-through, default anywhere, lazy tests, break/continue/labels,
// lexical scope and TDZ across cases, closures, finally, generators and async functions.
const log = (...a) => console.log(a.join(" "));

function kind(x) {
  switch (typeof x) {
    case "number":
      if (x !== x) return "NaN";
      return x === 0 ? "zero" : "number";
    case "string":
      return "string";
    case "undefined":
    case "object":
      return x === null ? "null" : x === undefined ? "undefined" : "object";
    default:
      return "other";
  }
}
log(kind(1), kind(0), kind(-0), kind(NaN), kind("s"), kind(null), kind(undefined), kind({}), kind(true));

function fall(n) {
  const out = [];
  switch (n) {
    case 1:
      out.push("one");
    case 2:
      out.push("two");
      break;
    default:
      out.push("default");
    case 3:
      out.push("three");
    case 4: {
      out.push("four");
    }
  }
  return out.join(",");
}
for (let i = 0; i <= 5; i++) log("fall", i, fall(i));

// Tests run lazily, in order, and default is taken only when nothing matches.
const seen = [];
const t = (v) => { seen.push(v); return v; };
function lazy(x) {
  seen.length = 0;
  switch (x) {
    case t(1): break;
    default: seen.push("default"); break;
    case t(2): seen.push("two"); break;
    case t(3): break;
  }
  return seen.join(",");
}
log("lazy", lazy(1), "|", lazy(2), "|", lazy(3), "|", lazy(9));

// Strict equality: no conversion, NaN never matches, -0 matches 0, objects by identity.
const o = {};
function strict(x) {
  switch (x) {
    case "1": return "string one";
    case 1: return "number one";
    case NaN: return "nan";
    case 0: return "zero";
    case o: return "the object";
    case null: return "null";
    case undefined: return "undefined";
  }
  return "none";
}
log(strict(1), strict("1"), strict(NaN), strict(-0), strict(o), strict({}), strict(null), strict(undefined), strict(true));

// break / continue / labels.
const trace = [];
outer: for (let i = 0; i < 4; i++) {
  switch (i) {
    case 0: continue;
    case 1: trace.push("b1"); break;
    case 2: trace.push("b2"); break outer;
  }
  trace.push(`after ${i}`);
}
lbl: switch (1) {
  case 1:
    trace.push("in lbl");
    if (trace.length) break lbl;
    trace.push("not reached");
}
while (true) {
  switch (trace.length) {
    default: break;
  }
  break;
}
log(trace.join(","));

// One scope for all cases; functions are hoisted; a `let` read in a later case.
function scope(n) {
  switch (n) {
    case 0:
      let a = "a0";
      const f = () => a + helper();
      return f();
    case 1:
      return typeof helper;
    case 2:
      try { return a; } catch (e) { return `${e.name}: ${e.message}`; }
    case 3:
      a = "set in 3";
      return a;
    case 4:
      function helper() { return "!"; }
      return helper();
  }
}
for (let i = 0; i < 5; i++) {
  try { log("scope", i, scope(i)); } catch (e) { log("scope", i, e.name, e.message); }
}
function fallInto(n) {
  switch (n) {
    case 0:
      let v = "declared";
    case 1:
      return v;
  }
}
log("fallInto", fallInto(0));
try { fallInto(1); } catch (e) { log("fallInto", e.name, e.message); }
function laterClosure(n) {
  switch (n) {
    case 0:
      let w = 5;
    case 1:
      return (() => w * 2)();
  }
}
log("laterClosure", laterClosure(0));
try { laterClosure(1); } catch (e) { log("laterClosure", e.name, e.message); }

// finally and exceptions.
function fin(x) {
  const out = [];
  for (const v of [1, 2, 3]) {
    try {
      switch (v) {
        case x: out.push("hit"); return out.join(",");
        case 2: throw new Error("two");
        default: out.push(v);
      }
    } catch (e) {
      out.push(e.message);
    } finally {
      out.push(`f${v}`);
    }
  }
  return out.join(",");
}
log("fin", fin(1), "|", fin(3), "|", fin(9));

// Discriminants of every kind; an empty switch and one with only a default.
switch (log("evaluated once"), 1) {}
switch (2) { default: log("only default"); }
const big = (k) => { switch (k) { case 1.5: return "1.5"; case true: return "true"; case "": return "empty"; case 1e21: return "1e21"; } return "-"; };
log(big(1.5), big(true), big(""), big(1e21), big(1));

// Inside generators and async functions (suspension points in the discriminant, the tests
// and the bodies).
function* gen(xs) {
  for (const x of xs) {
    switch (yield x) {
      case "skip": continue;
      case "stop": return "stopped";
      default:
        switch (x) {
          case 1: yield "one"; break;
          case 2: yield "two";
          case 3: yield "two or three"; break;
        }
    }
  }
  return "done";
}
const g = gen([1, 2, 3, 4]);
log(JSON_like(g.next()), JSON_like(g.next()), JSON_like(g.next("skip")), JSON_like(g.next()), JSON_like(g.next()), JSON_like(g.next()), JSON_like(g.next("stop")));
function JSON_like(r) { return `${r.value}/${r.done}`; }

const later = (v) => new Promise((r) => queueMicrotask(() => r(v)));
async function asyncSwitch(x) {
  const out = [];
  switch (await later(x)) {
    case await later(1):
      out.push(await later("one"));
    case 2:
      out.push("two");
      break;
    default:
      try {
        switch (x) {
          case 3: throw new Error(await later("three"));
        }
      } catch (e) {
        out.push(`caught ${e.message}`);
      }
  }
  return out.join(",");
}
(async () => {
  for (const x of [1, 2, 3, 4]) log("async", x, await asyncSwitch(x));
})();
