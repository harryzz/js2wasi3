// Dates in conversions, operators, JSON, reflection, and Date as a value.
const d = new Date(Date.UTC(2026, 8, 28, 12, 3, 4, 5));
const e = new Date(0);
console.log(d + 1, d - 0, +d, `${d}`, d * 1, [d] + "", String(d) === d.toString());
console.log(d == d.toString(), d == d.getTime(), d > e, d < e, d >= new Date(d), e - d, Math.max(d, e) === +d);
console.log(typeof d, d instanceof Date, d instanceof Object, Object.prototype.toString.call(d),
  Object.prototype.toString.call(Date.prototype), Object.getPrototypeOf(d) === Date.prototype, Object.keys(d).length);
console.log(JSON.stringify(d), JSON.stringify({ at: e, list: [d, new Date(NaN)] }), JSON.stringify(new Date(NaN)));
console.log(Date.prototype.toJSON.call({ toISOString() { return "iso"; } }), Date.prototype.toJSON.call({ valueOf() { return NaN; }, toISOString() { return "x"; } }));
try { Date.prototype.toJSON.call({ toISOString: 1 }); } catch (err) { console.log(err.name, err.message); }
try { new Date(NaN).toISOString(); } catch (err) { console.log(err.name, err.message); }
try { JSON.stringify({ d: new Date(NaN), f() {} }); console.log("ok"); } catch (err) { console.log(err.name); }
// Symbol.toPrimitive.
const tp = Date.prototype[Symbol.toPrimitive];
console.log(typeof tp, tp.call(d, "number"), tp.call(d, "default"), tp.call(d, "string") === d.toString(), tp.length, tp.name);
try { tp.call(d, "x"); } catch (err) { console.log(err.name, err.message); }
try { tp.call(1, "number"); } catch (err) { console.log(err.name, err.message); }
console.log(tp.call({ toString() { return "s"; }, valueOf() { return 7; } }, "default"));
// A date whose toString/valueOf are replaced.
const o = new Date(0);
o.toString = () => "custom";
o.valueOf = () => 42;
console.log(o + 1, o * 2, `${o}`, String(o), o == "custom");
// Sorting and arithmetic.
const list = [new Date(3), new Date(1), new Date(2)];
list.sort((a, b) => a - b);
console.log(list.map((x) => x.getTime()), list.map(Number), new Date(d.getTime() + 86400000).getUTCDate());
// Date as a value.
const D = Date;
console.log(typeof D, D.name, D.length, D.prototype.constructor === Date, new D(0).getTime(), typeof D(),
  D.UTC(1970), D.parse("1970-01-02"), typeof D.now(), Date.prototype.constructor.name);
console.log(Date.UTC.name, Date.UTC.length, Date.parse.length, Date.now.length, Date.prototype.getTime.length,
  Date.prototype.setMinutes.length, Date.prototype.setHours.length, Date.prototype.setFullYear.length,
  Date.prototype.toLocaleString.length, Date.prototype.toJSON.length, Date.prototype.setUTCSeconds.name);
console.log(Reflect.construct(Date, [0]).getTime(), Reflect.apply(Date.prototype.getTime, e, []), [1e12, 0].map((t) => new Date(t).getUTCFullYear()));
const f = Date.prototype.getFullYear;
console.log(f.call(d), Date.prototype.hasOwnProperty("getTime"), d.hasOwnProperty("getTime"), "getTime" in d);
try { f(); } catch (err) { console.log(err.name, err.message); }
// Dates as Map keys and in Sets (identity).
const m = new Map([[d, "d"]]);
console.log(m.get(d), m.get(new Date(d)), new Set([d, d, new Date(d)]).size);
// Locale forms: only the default (en-US) is supported.
console.log(d.toLocaleString(undefined), d.toLocaleString("en-US"), new Date(NaN).toLocaleString());
