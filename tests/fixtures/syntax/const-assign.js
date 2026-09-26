// Writing a `const` throws a TypeError after evaluating the value (a pre-existing gap, T24).
const log = (...a) => console.log(a.join(" "));
const k = 1;
let evaluated = 0;
const tries = [
  () => { k = (evaluated++, 2); },
  () => { k += 1; },
  () => { k++; },
  () => { --k; },
  () => { [k] = [5]; },
  () => { ({ k } = { k: 6 }); },
  () => { for (k of [1]) {} },
  () => { for (k in { a: 1 }) {} },
  () => log("value", (k = 3)),
];
for (const t of tries) {
  try { t(); log("no error"); } catch (e) { log(e.name, e.message); }
}
log("k", k, "evaluated", evaluated);
function inner() {
  const c = "c";
  try { c = "d"; } catch (e) { log("inner", e.name); }
  const f = () => { c = 1; };
  try { f(); } catch (e) { log("closure", e.name); }
  return c;
}
log(inner());
for (const x of [1, 2]) {
  try { x = 3; } catch (e) { log("loop const", x, e.name); }
}
