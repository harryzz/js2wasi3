// T77: zigbee-herdsman's time cluster (the 2026-10-04 production crash, F330) under node and js2wasi3.
// Run from the zigbee2mqtt build tree (~/js2wasi3-prod-run/zp/patched): node zigbee-time.cjs, or built with
// js2wasi3 and run with wasmtime. Prints only fields that don't change with the clock.
const { getTimeClusterAttributes } = require("zigbee-herdsman/dist/utils/timeService");
const { tzScan, tzOffset } = require("@date-fns/tz");

const a = getTimeClusterAttributes();
console.log(JSON.stringify({ timeStatus: a.timeStatus, timeZone: a.timeZone, dstStart: a.dstStart, dstEnd: a.dstEnd, dstShift: a.dstShift }));
for (const zone of ["Europe/Sofia", "America/New_York", "Australia/Sydney", "Asia/Kolkata", "Africa/Casablanca"]) {
  for (const year of [2026, 2027]) {
    const changes = tzScan(zone, { start: new Date(year, 1), end: new Date(year, 12) });
    console.log(zone, year, JSON.stringify(changes.map((c) => [c.date.toISOString(), c.change, c.offset])));
  }
  console.log(zone, tzOffset(zone, new Date(Date.UTC(2026, 9, 4, 7, 12, 44))), tzOffset(zone, new Date(Date.UTC(2026, 0, 15))));
}
