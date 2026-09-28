// Date setters: arguments present or not, conversion order, NaN, rollover, return values.
const base = Date.UTC(2026, 8, 28, 12, 3, 4, 5);
const show = (d, r) => console.log(r, d.getTime(), Number.isNaN(d.getTime()) ? "Invalid" : d.toISOString());
let d = new Date(base);
show(d, d.setMilliseconds(999));
show(d, d.setSeconds(59));
show(d, d.setSeconds(1, 2));
show(d, d.setMinutes(30));
show(d, d.setMinutes(30, 40));
show(d, d.setMinutes(30, 40, 50));
show(d, d.setHours(23));
show(d, d.setHours(0, 1, 2, 3));
show(d, d.setDate(31));
show(d, d.setDate(0));
show(d, d.setMonth(1));
show(d, d.setMonth(13, 1));
show(d, d.setFullYear(2000));
show(d, d.setFullYear(2001, 1));
show(d, d.setFullYear(2004, 1, 29));
show(d, d.setYear(99));
show(d, d.setYear(2030));
show(d, d.setTime(0));
show(d, d.setTime(1.5));
show(d, d.setTime("86400000"));
show(d, d.setUTCMilliseconds(-1));
show(d, d.setUTCSeconds(-1, -1));
show(d, d.setUTCMinutes(90, 90, 1500));
show(d, d.setUTCHours(48));
show(d, d.setUTCDate(-1));
show(d, d.setUTCMonth(-13, 45));
show(d, d.setUTCFullYear(-5, 0, 1));
// Rolled far out of range: invalid.
show(d, d.setUTCFullYear(300000));
// A NaN time value: most setters convert their arguments, then give NaN; setFullYear/setYear start from +0.
d = new Date(NaN);
show(d, d.setHours(1));
show(d, d.setDate(1));
show(d, d.setMonth(1));
show(d, d.setFullYear(2026));
d = new Date(NaN);
show(d, d.setYear(95));
show(d, d.setYear(NaN));
show(d, d.setTime(NaN));
// Conversion order and "present" arguments (undefined counts as present: NaN).
d = new Date(base);
const log = [];
const v = (name, x) => ({ valueOf() { log.push(name); return x; } });
d.setHours(v("h", 1), v("m", 2), v("s", 3), v("ms", 4));
console.log(log.join(","), d.toISOString());
show(d, d.setMinutes(5, undefined));
d = new Date(NaN);
log.length = 0;
console.log(d.setHours(v("h", 1), v("m", 2)), log.join(","));
// Setters return the new time value; getters after them.
d = new Date(2026, 0, 31);
d.setMonth(1);
console.log(d.getMonth(), d.getDate(), d.getDay(), d.setDate(d.getDate() + 30), d.toDateString());
// Setting through the prototype on a non-Date throws.
for (const m of ["setTime", "getTime", "setHours", "toISOString", "valueOf"]) {
  try {
    Date.prototype[m].call({}, 1);
  } catch (e) {
    console.log(m, e.name, e.message);
  }
}
try { Date.prototype.getTime(); } catch (e) { console.log(e.name, e.message); }
