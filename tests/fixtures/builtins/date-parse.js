// Date.parse and new Date(string): the Date Time String Format, then V8's legacy forms.
const forms = [
  // ISO: date only (UTC), date-time (local = UTC here), offsets, fractions, extended years.
  "2026", "2026-09", "2026-09-28", "2026-09-28T10:20", "2026-09-28T10:20:30", "2026-09-28T10:20:30.5",
  "2026-09-28T10:20:30.123456", "2026-09-28T10:20:30.123Z", "2026-09-28T10:20:30.123+02:00",
  "2026-09-28T10:20:30-05:30", "2026-09-28t10:20:30z", "+002026-09-28T00:00:00Z", "-000001-01-01T00:00:00Z",
  "+275760-09-13T00:00:00.000Z", "+275760-09-13T00:00:00.001Z", "-271821-04-20T00:00:00.000Z",
  "-271821-04-19T23:59:59.999Z", "2026-09-28T24:00", "2026-09-28T24:00:01", "-000000-01-01T00:00:00Z",
  "2026-13-01", "2026-02-29", "2024-02-29", "2026-02-30", "2026-04-31", "2026-09-28T25:00", "2026-09-28T10:60",
  "2026-09-28T10", "2026-09-28T10:20:30.", "2026-09-28T10:20+0200", "20260928",
  // Legacy: month names, slashes, toString/toUTCString output, AM/PM, offsets, comments.
  "Sep 28 2026", "September 28, 2026 10:00:00", "28 Sep 2026", "Sep 2026", "2026 Sep 28", "Mon, 28 Sep 2026",
  "2026/09/28", "9/28/2026", "9/28/26", "1/2/3", "12/31/1999 23:59:59", "2026-9-8", "2026-09-28 10:20",
  "2026/9/28 10:20:30.25", "Mon Sep 28 2026 12:03:04 GMT+0000 (Coordinated Universal Time)",
  "Mon, 28 Sep 2026 12:03:04 GMT", "Thu, 01 Jan 1970 00:00:00 GMT+0100", "Thu, 01 Jan 1970 00:00:00 GMT-0130",
  "Jan 1 1970 00:00:00 UTC+2", "Jan 1 1970 00:00:00 +01:30", "1970-01-01 00:00:00Z", "Sep 28 2026 10:00 PM",
  "Sep 28 2026 12:00 AM", "Sep 28 2026 12:30 pm", "Sep 28 2026 13:00 PM", "sep 28 2026", "SEPTEMBER 28 2026",
  "Sep 28 2026 (comment (nested)) 10:00", "Sep 28 2026 10:00:00.5", "Sep 31 2026", "Feb 30 2026",
  // Invalid.
  "", " ", "garbage", "12:00", "tomorrow", "2026-09-28 garbage", "Sep", "Sep 32 2026", "13/1/2026",
  "Sep 28 2026 25:00",
];
for (const s of forms) {
  const t = Date.parse(s);
  console.log(JSON.stringify(s), t, t === t ? new Date(t).toISOString() : "", new Date(s).getTime() === t || (t !== t && isNaN(new Date(s))));
}
console.log(Date.parse(), Date.parse(undefined), Date.parse(null), Date.parse(1e12), Date.parse({ toString() { return "2026"; } }));
try { Date.parse(Symbol("x")); } catch (e) { console.log(e.name, e.message); }
// Round trips through every string form.
const t = Date.UTC(2026, 8, 28, 12, 3, 4, 5);
const d = new Date(t);
for (const s of [d.toString(), d.toUTCString(), d.toISOString(), d.toDateString(), d.toLocaleString(), String(d)]) {
  console.log(Date.parse(s) - t);
}
for (const y of [-271820, -1, 0, 1, 99, 100, 1969, 10000, 275759]) {
  const e = new Date(Date.UTC(y, 5, 15, 1, 2, 3, 4));
  console.log(y, e.toISOString(), Date.parse(e.toISOString()) === e.getTime(), Date.parse(e.toUTCString()) === e.getTime() - 4);
}
