// T79: replays a record-prod.sh recording to a zigbee2mqtt under test (node or js2wasi3), offline.
//   node replay.cjs <recording dir> --port 16638 --mqtt mqtt://127.0.0.1:18830 [--stats out.json] [--seconds N]
// Start it first (it preloads the broker), then zigbee2mqtt with serial port tcp://127.0.0.1:<port> and that broker.
//
// Fake coordinator (zigbee.jsonl, decoded by znp.cjs):
// - every request zigbee2mqtt sends is matched with a recorded one (same command and data; AF requests by
//   destination, cluster and ZCL payload without the sequence number, else by destination, cluster and ZCL command
//   (answers with live data: time reads); else the next one of the same command) and
//   answered with what the coordinator answered then: its SRSP at once, and the frames it caused (AF.dataConfirm
//   with the request's transid, the device's ZCL reply with the request's ZCL sequence number, the ZDO response from
//   the addressed device, SYS.resetInd, ZDO.stateChangeInd) after their recorded delays, rewritten to the live
//   transid / sequence number. A request with no recorded match gets a zero-filled SRSP (counted).
// - every other frame from the coordinator (device reports, announcements, ...) is replayed on the recorded
//   timeline, anchored where zigbee2mqtt reported itself started: the recording's `bridge/state` online publish ↔
//   the live one (frames before it in the recording go out at the live anchor), up to the recording's `bridge/state`
//   offline (later frames arrived during its shutdown: not handled then). `--print-steady`: that span in seconds.
// MQTT (mqtt.jsonl, MQTT 3.1.1): the retained messages the broker gave zigbee2mqtt are published retained before
// it starts; the other messages it received (not echoes of its own publishes) are published on the same anchored
// timeline.
// Stats (on exit or after --seconds): matched/unmatched requests, replayed frames and messages, anchor, timings.
"use strict";
const net = require("node:net");
const fs = require("node:fs");
const path = require("node:path");
const { frames, show } = require("./znp.cjs");

const PKG = process.env.Z2M_PKG || path.join(process.env.HOME, "js2wasi3-prod-run/zp/package");
const mqtt = require(path.join(PKG, "node_modules/mqtt"));
const mqttPacket = require(path.join(PKG, "node_modules/mqtt-packet"));

const args = process.argv.slice(2);
const dir = args[0];
const opt = (k, d) => (args.includes(k) ? args[args.indexOf(k) + 1] : d);
const PORT = +opt("--port", 16638);
const BROKER = opt("--mqtt", "mqtt://127.0.0.1:18830");
const STATS = opt("--stats", null);
const SECONDS = +opt("--seconds", 0);
const log = (...a) => console.error(new Date().toISOString(), "replay:", ...a);

// ---- the recording ----------------------------------------------------------------------------

const startOf = (file) => Date.parse(JSON.parse(fs.readFileSync(file, "utf8").split("\n", 1)[0]).start);
const zStart = startOf(path.join(dir, "zigbee.jsonl"));
const mStart = startOf(path.join(dir, "mqtt.jsonl"));
const all = frames(path.join(dir, "zigbee.jsonl")).map((f) => ({ ...f, t: f.t + (zStart - mStart) })); // MQTT's clock

// MQTT packets per direction.
function mqttPackets(file) {
  const out = [];
  const parsers = new Map();
  for (const line of fs.readFileSync(file, "utf8").split("\n")) {
    if (!line) continue;
    const r = JSON.parse(line);
    if (!r.x) continue;
    const key = `${r.c}:${r.d}`;
    if (!parsers.has(key)) {
      const p = mqttPacket.parser({ protocolVersion: 4 });
      const st = { t: 0 };
      p.on("packet", (pk) => out.push({ t: st.t, d: r.d, c: r.c, p: pk }));
      parsers.set(key, { p, st });
    }
    const { p, st } = parsers.get(key);
    st.t = r.t;
    p.parse(Buffer.from(r.x, "hex"));
  }
  return out;
}
const packets = mqttPackets(path.join(dir, "mqtt.jsonl"));
const connect = packets.find((x) => x.p.cmd === "connect");
const base = (process.env.Z2M_BASE_TOPIC || "zigbee2mqtt");
const onlineRec = packets.find((x) => x.d === "up" && x.p.cmd === "publish" && x.p.topic === `${base}/bridge/state` &&
  String(x.p.payload).includes("online"));
if (!onlineRec) throw new Error(`no ${base}/bridge/state online publish in the recording (set Z2M_BASE_TOPIC?)`);
const T_READY = onlineRec.t;
// The end of the recorded session: zigbee2mqtt's `bridge/state` offline publish as it stopped (after it, devices
// still sent frames during its shutdown backup that it no longer handled).
const offlineRec = packets.find((x) => x.d === "up" && x.p.cmd === "publish" && x.p.topic === `${base}/bridge/state` &&
  String(x.p.payload).includes("offline") && x.t > T_READY);
const T_END = offlineRec ? offlineRec.t : Infinity;
if (args.includes("--print-steady")) {
  process.stdout.write(`${Math.ceil((T_END === Infinity ? 0 : T_END - T_READY) / 1000)}\n`);
  process.exit(0);
}

// ---- classifying the coordinator's frames -------------------------------------------------------

const AF_REQ = new Set(["AF.dataRequest", "AF.dataRequestExt", "AF.dataRequestSrcRtg"]);
// The ZCL sequence number's index in a ZCL frame (after a manufacturer code when bit 2 of the frame control is set).
const tsnIndex = (zcl) => (zcl[0] & 0x04 ? 3 : 1);
// Where the ZCL frame starts in an AF.incomingMsg(Ext) frame's data.
const incomingDataOffset = (name) => (name === "AF.incomingMsgExt" ? 27 : 17);
const nwkOf = (p) => (typeof p.dstaddr === "string" ? parseInt(p.dstaddr, 16) & 0xffff : p.dstaddr);

const requests = []; // recorded host requests: { f, srsp, caused: [{ f, delay, kind }], used }
const claimed = new Set();
for (let i = 0; i < all.length; i++) {
  const f = all[i];
  if (f.d !== "up") continue;
  const r = { f, srsp: null, caused: [], used: false, key: null };
  if (f.type === "SREQ") {
    for (let j = i + 1; j < all.length; j++) {
      if (all[j].d === "down" && all[j].type === "SRSP") {
        r.srsp = all[j];
        claimed.add(j);
        break;
      }
    }
  }
  const p = f.payload || {};
  const within = (j, ms) => all[j].t - f.t <= ms;
  if (AF_REQ.has(f.name) && p.data) {
    const zcl = Buffer.from(p.data);
    const tsn = zcl.length > tsnIndex(zcl) ? zcl[tsnIndex(zcl)] : -1;
    let confirm = false, reply = false;
    for (let j = i + 1; j < all.length && within(j, 15000); j++) {
      const g = all[j];
      if (g.d !== "down" || g.type !== "AREQ" || claimed.has(j)) continue;
      if (!confirm && g.name === "AF.dataConfirm" && g.payload && g.payload.transid === p.transid) {
        r.caused.push({ f: g, delay: g.t - f.t, kind: "confirm" });
        claimed.add(j);
        confirm = true;
      } else if (!reply && (g.name === "AF.incomingMsg" || g.name === "AF.incomingMsgExt") && g.payload &&
        g.payload.clusterid === p.clusterid && (g.payload.srcaddr & 0xffff) === nwkOf(p)) {
        const gz = g.data.subarray(incomingDataOffset(g.name));
        if (gz.length > tsnIndex(gz) && gz[tsnIndex(gz)] === tsn && (gz[0] & 0x08)) { // server→client
          r.caused.push({ f: g, delay: g.t - f.t, kind: "reply" });
          claimed.add(j);
          reply = true;
        }
      }
      if (confirm && reply) break;
    }
    // Key: destination, cluster, ZCL frame without its sequence number.
    const z2 = Buffer.from(zcl);
    if (tsn >= 0) z2[tsnIndex(z2)] = 0;
    r.key = `${f.name}|${nwkOf(p)}|${p.dstendpoint ?? p.destendpoint}|${p.clusterid}|${z2.toString("hex")}`;
  } else if (f.subsystem === "ZDO" && /Req$/.test(f.name)) {
    const rsp = f.name.replace(/Req$/, "Rsp");
    const addr = p.dstaddr ?? p.nwkaddrofinterest ?? null;
    for (let j = i + 1; j < all.length && within(j, 15000); j++) {
      const g = all[j];
      if (g.d === "down" && g.type === "AREQ" && !claimed.has(j) && g.name === rsp &&
        (addr === null || g.payload?.srcaddr === addr)) {
        r.caused.push({ f: g, delay: g.t - f.t, kind: "zdo" });
        claimed.add(j);
        break;
      }
    }
  } else if (f.name === "SYS.resetReq" || f.name === "ZDO.startupFromApp") {
    const want = f.name === "SYS.resetReq" ? "SYS.resetInd" : "ZDO.stateChangeInd";
    for (let j = i + 1; j < all.length && within(j, 20000); j++) {
      const g = all[j];
      if (g.d === "down" && g.type === "AREQ" && !claimed.has(j) && g.name === want) {
        r.caused.push({ f: g, delay: g.t - f.t, kind: "sys" });
        claimed.add(j);
      }
    }
  }
  if (r.key === null) r.key = `${f.name}|${f.data.toString("hex")}`;
  requests.push(r);
}
const unsolicited = all.filter((f, i) => f.d === "down" && f.type === "AREQ" && !claimed.has(i) && f.t < T_END);
// Middle key for AF requests whose payload carries live data (a time read's answer): destination, cluster, the ZCL
// frame control and command.
function midKey(name, p) {
  if (!AF_REQ.has(name) || !p.data) return null;
  const z = Buffer.from(p.data);
  return `${name}|${nwkOf(p)}|${p.dstendpoint ?? p.destendpoint}|${p.clusterid}|${z[0]}|${z[z[0] & 0x04 ? 4 : 2]}`;
}
const byKey = new Map(), byMid = new Map(), byName = new Map();
for (const r of requests) {
  if (!byKey.has(r.key)) byKey.set(r.key, []);
  byKey.get(r.key).push(r);
  const mk = midKey(r.f.name, r.f.payload || {});
  if (mk !== null) {
    if (!byMid.has(mk)) byMid.set(mk, []);
    byMid.get(mk).push(r);
  }
  if (!byName.has(r.f.name)) byName.set(r.f.name, []);
  byName.get(r.f.name).push(r);
}

// ---- stats --------------------------------------------------------------------------------------

const stats = {
  recording: dir, requestsRecorded: requests.length, unsolicitedRecorded: unsolicited.length,
  unsolicitedAfterReady: unsolicited.filter((f) => f.t >= T_READY).length,
  steadyRecordedS: +((T_END - T_READY) / 1000).toFixed(1), matchedExact: 0, matchedByCommand: 0, matchedByName: 0, unmatched: 0, unmatchedNames: {}, causedSent: 0,
  unsolicitedSent: 0, mqttInSent: 0, retainedPreloaded: 0, anchor: null, startedAt: Date.now(), lastEventAt: null,
};
function writeStats() {
  stats.endedAt = Date.now();
  const s = JSON.stringify(stats, null, 1);
  if (STATS) fs.writeFileSync(STATS, s);
  log(s);
}

// ---- rewriting ------------------------------------------------------------------------------------

function withFcs(raw) {
  let fcs = 0;
  for (let i = 1; i < raw.length - 1; i++) fcs ^= raw[i];
  raw[raw.length - 1] = fcs;
  return raw;
}

// The recorded caused frame, rewritten for the live request.
function rewrite(c, live) {
  const raw = Buffer.from(c.f.raw);
  if (c.kind === "confirm") {
    raw[4 + 2] = live.payload.transid; // status, endpoint, transid
  } else if (c.kind === "reply") {
    const lz = Buffer.from(live.payload.data);
    const off = 4 + incomingDataOffset(c.f.name);
    const z = raw.subarray(off);
    z[tsnIndex(z)] = lz[tsnIndex(lz)];
  }
  return withFcs(raw);
}

// A zero-filled SRSP for a request with no recorded match.
function defaultSrsp(f) {
  const raw = Buffer.from([0xfe, 1, 0x60 | (f.raw[2] & 0x1f), f.cmd, 0, 0]);
  return withFcs(raw);
}

// ---- the fake coordinator ---------------------------------------------------------------------------

let sock = null;
const send = (buf) => {
  if (sock && !sock.destroyed) sock.write(buf);
};

function take(f) {
  const exact = (byKey.get(keyOf(f)) || []).find((r) => !r.used);
  if (exact) {
    stats.matchedExact++;
    return exact;
  }
  const mk = midKey(f.name, f.payload || {});
  const mid = mk === null ? undefined : (byMid.get(mk) || []).find((r) => !r.used);
  if (mid) {
    stats.matchedByCommand++;
    return mid;
  }
  const loose = (byName.get(f.name) || []).find((r) => !r.used);
  if (loose) stats.matchedByName++;
  return loose;
}

function keyOf(f) {
  const p = f.payload || {};
  if (AF_REQ.has(f.name) && p.data) {
    const z2 = Buffer.from(p.data);
    if (z2.length > tsnIndex(z2)) z2[tsnIndex(z2)] = 0;
    return `${f.name}|${nwkOf(p)}|${p.dstendpoint ?? p.destendpoint}|${p.clusterid}|${z2.toString("hex")}`;
  }
  return `${f.name}|${f.data.toString("hex")}`;
}

const { splitter, decode } = require("./znp.cjs");
net.createServer((s) => {
  log("zigbee2mqtt connected");
  sock = s;
  s.setNoDelay(true);
  const split = splitter((raw) => {
    const f = decode(raw, 0, "up");
    const r = take(f);
    if (!r) {
      stats.unmatched++;
      stats.unmatchedNames[f.name] = (stats.unmatchedNames[f.name] || 0) + 1;
      if (f.type === "SREQ") send(defaultSrsp(f));
      return;
    }
    r.used = true;
    if (r.srsp) send(r.srsp.raw);
    for (const c of r.caused) {
      setTimeout(() => {
        stats.causedSent++;
        send(rewrite(c, f));
      }, Math.max(0, c.delay));
    }
  });
  s.on("data", (b) => split(b, 0));
  s.on("error", () => {});
  s.on("close", () => log("zigbee2mqtt disconnected"));
}).listen(PORT, "127.0.0.1", () => log(`coordinator on 127.0.0.1:${PORT}; ${requests.length} requests, ${unsolicited.length} unsolicited frames`));

// ---- MQTT ---------------------------------------------------------------------------------------------

// The messages the broker delivered to zigbee2mqtt: retained ones (preloaded) and live ones that aren't echoes of
// its own publishes.
const delivered = packets.filter((x) => x.d === "down" && x.p.cmd === "publish");
const ownPublishes = packets.filter((x) => x.d === "up" && x.p.cmd === "publish");
const isEcho = (x) => ownPublishes.some((o) => o.p.topic === x.p.topic && o.t <= x.t && x.t - o.t < 2000 &&
  Buffer.compare(Buffer.from(o.p.payload), Buffer.from(x.p.payload)) === 0);
const retained = new Map();
for (const x of delivered) if (x.p.retain) retained.set(x.p.topic, x.p.payload);
const live = delivered.filter((x) => !x.p.retain && !isEcho(x) && x.t < T_END);

const client = mqtt.connect(BROKER, { clientId: "t79-replay", clean: true });
client.on("connect", () => {
  // The broker is fresh: clear nothing, preload the recording's retained messages.
  let pending = retained.size;
  for (const [topic, payload] of retained) {
    client.publish(topic, Buffer.from(payload), { retain: true, qos: 1 }, () => {
      stats.retainedPreloaded++;
      if (--pending === 0) log(`preloaded ${retained.size} retained messages; waiting for ${base}/bridge/state online`);
    });
  }
  if (retained.size === 0) log("no retained messages");
  client.subscribe(`${base}/bridge/state`);
});
client.on("message", (topic, payload, packet) => {
  if (topic !== `${base}/bridge/state` || packet.retain || stats.anchor !== null || !String(payload).includes("online")) return;
  stats.anchor = Date.now();
  log(`anchor: zigbee2mqtt started ${((stats.anchor - stats.startedAt) / 1000).toFixed(1)} s after the replay`);
  for (const f of unsolicited) {
    setTimeout(() => {
      stats.unsolicitedSent++;
      stats.lastEventAt = Date.now();
      send(f.raw);
    }, Math.max(0, f.t - T_READY));
  }
  for (const x of live) {
    setTimeout(() => {
      stats.mqttInSent++;
      client.publish(x.p.topic, Buffer.from(x.p.payload), { qos: x.p.qos, retain: false });
    }, Math.max(0, x.t - T_READY));
  }
  if (SECONDS) setTimeout(() => {
    writeStats();
    process.exit(0);
  }, SECONDS * 1000);
});

for (const s of ["SIGINT", "SIGTERM"]) process.on(s, () => {
  writeStats();
  process.exit(0);
});
log(`recording: ${all.length} frames, ${packets.length} MQTT packets (${retained.size} retained topics, ${live.length} live inbound), ` +
  `ready at ${(T_READY / 1000).toFixed(1)} s; MQTT client ${connect ? connect.p.clientId : "?"}`);
void show;
