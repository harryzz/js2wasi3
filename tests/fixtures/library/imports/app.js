// Calls every function of the imported interface `acme:demo/host` (with and without its version).
import { add, neg, wide, half, initial, greet, isEven, shift, bytes, maybe, pair } from "acme:demo/host";
import { paint, area, parse, check, countMany, log as note } from "acme:demo/host@0.1.0";

const show = (v) => (typeof v === "string" ? v : JSON.stringify(v));

function attempt(f) {
  try {
    return "ok " + show(f());
  } catch (e) {
    return `threw ${e instanceof Error} ${e.message} ${show(e.payload)}`;
  }
}

export function run(n) {
  note("run");
  const plus = add; // an import as a value
  const b = bytes(n);
  return [
    show(add(n, 2)),
    show(plus(n, 40)),
    show(neg(n)),
    show(wide(n)),
    show(half(n)),
    initial("πx"),
    greet("js"),
    show(isEven(n)),
    show(shift({ x: 1, y: 2 }, [n, 10])),
    show([b instanceof Uint8Array, ...b]),
    show(maybe(n)),
    show(maybe(0)),
    show(pair("ab", n)),
    paint("dark-blue", { read: true, writeAll: false }),
    show(area({ tag: "circle", val: 2 })),
    show(area({ tag: "empty" })),
    attempt(() => parse("42")),
    attempt(() => parse("x")),
    attempt(() => check(true)),
    attempt(() => check(false)),
    show(countMany([1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1], "abc")),
  ];
}
