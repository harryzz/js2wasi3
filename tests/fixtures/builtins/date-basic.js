// Date: construction, getters, string forms (compared with node under TZ=UTC).
const d = new Date(2026, 8, 28, 12, 3, 4, 5);
console.log(d.getTime(), d.valueOf(), d.getFullYear(), d.getMonth(), d.getDate(), d.getDay(), d.getHours(),
  d.getMinutes(), d.getSeconds(), d.getMilliseconds(), d.getTimezoneOffset(), d.getYear());
console.log(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(), d.getUTCDay(), d.getUTCHours(), d.getUTCMinutes(),
  d.getUTCSeconds(), d.getUTCMilliseconds());
console.log(String(d), "|", d.toDateString(), "|", d.toTimeString());
console.log(d.toUTCString(), "|", d.toGMTString(), "|", d.toISOString(), "|", d.toJSON());
console.log(d.toLocaleString(), "|", d.toLocaleDateString(), "|", d.toLocaleTimeString(), "|",
  new Date(2026, 0, 1, 0, 5).toLocaleTimeString(), new Date(2026, 0, 1, 12).toLocaleTimeString());
// Every argument count; two-digit years; overflowing fields roll over.
console.log(new Date(0).getTime(), new Date(2026, 0).getTime(), new Date(2026, 0, 31).getTime(),
  new Date(99, 11).getFullYear(), new Date(0, 0).getFullYear(), new Date(100, 0).getFullYear(),
  new Date(2026, 13, 35, 25, 61, 61, 1001).toISOString(), new Date(2026, -1, 0).toISOString());
console.log(new Date(8.64e15).toISOString(), new Date(8.64e15 + 1).getTime(), new Date(-8.64e15).toISOString(),
  new Date(1.9).getTime(), new Date(-1.9).getTime(), new Date(-1).toISOString(), new Date(-62198755200000).toISOString());
console.log(new Date(NaN).getTime(), new Date(NaN).getFullYear(), new Date(NaN).getTimezoneOffset(), new Date(Infinity).getTime(),
  new Date(2026, NaN).getTime(), new Date(2026, 0, 1, Infinity).getTime(), new Date("x").getDay());
// From another Date, strings, objects with valueOf/toString, booleans, null.
const copy = new Date(d);
console.log(copy.getTime() === d.getTime(), copy !== d, new Date("2026-09-28T12:03:04.005Z").getTime(),
  new Date({ valueOf() { return 86400000; } }).getTime(), new Date({ toString() { return "1970-01-02"; }, valueOf: undefined }).getTime(),
  new Date(true).getTime(), new Date(null).getTime(), new Date(undefined).getTime(), new Date("").getTime());
console.log(Date.UTC(2026, 8, 28), Date.UTC(2026), Date.UTC(), Date.UTC(99, 0), Date.UTC(2026, 8, 28, 12, 3, 4, 5), Date.UTC.length);
console.log(new Date(Date.UTC(-271821, 3, 20)).toISOString(), new Date(Date.UTC(275760, 8, 13)).toISOString(),
  Date.UTC(275760, 8, 13, 0, 0, 0, 1), new Date(Date.UTC(0, 0, 1)).getUTCFullYear(), new Date(Date.UTC(-1, 11, 31)).toISOString());
// Leap years and week days across centuries.
for (const y of [1600, 1700, 1900, 2000, 2024, 2100, -400, -1]) {
  const f = new Date(Date.UTC(y, 1, 29));
  console.log(y, f.getUTCMonth(), f.getUTCDate(), f.getUTCDay(), f.toUTCString());
}
// Printing.
console.log(new Date(0), [new Date(0), new Date(NaN)], { when: new Date(1e12) });
const extra = new Date(0);
extra.note = "x";
console.log(extra);
// Date() without new ignores its arguments and gives the current time as a string.
console.log(typeof Date(), typeof Date(0), Date(0).length === Date().length, Date(0).endsWith("GMT+0000 (Coordinated Universal Time)"));
const now = Date.now();
console.log(typeof now, now > 1.7e12, now === Math.floor(now), new Date().getTime() >= now, new Date().getFullYear() >= 2026);
