// T79: Z-Stack (UNPI/MT) frames of a tcp-record.cjs recording, decoded with zigbee-herdsman's own definitions
// (from the zigbee2mqtt tree: $Z2M_PKG, default ~/js2wasi3-prod-run/zp/package).
//   node znp.cjs <zigbee.jsonl> [--json]     prints one line per frame: time, direction, type, name, payload
// As a module: frames(path) → [{t, d: "up"|"down", type: "SREQ"|"AREQ"|"SRSP", subsystem, cmd, name, data
// (Buffer: the frame's data), raw (Buffer: the whole frame), payload (decoded, or null)}].
"use strict";
const fs = require("node:fs");
const path = require("node:path");

const PKG = process.env.Z2M_PKG || path.join(process.env.HOME, "js2wasi3-prod-run/zp/package");
const ZS = path.join(PKG, "node_modules/zigbee-herdsman/dist/adapter/z-stack");
const { ZpiObject } = require(path.join(ZS, "znp/zpiObject"));
const { Frame } = require(path.join(ZS, "unpi/frame"));
const { Subsystem } = require(path.join(ZS, "unpi/constants"));

const TYPES = { 0: "POLL", 1: "SREQ", 2: "AREQ", 3: "SRSP" };
const SUBSYSTEMS = Object.fromEntries(Object.entries(Subsystem).filter(([, v]) => typeof v === "number").map(([k, v]) => [v, k]));

// Splits a byte stream into frames (bytes before a SOF, e.g. the bootloader skip, are dropped).
function splitter(onFrame) {
  let buf = Buffer.alloc(0);
  return (chunk, t) => {
    buf = Buffer.concat([buf, chunk]);
    for (;;) {
      const sof = buf.indexOf(0xfe);
      if (sof < 0) {
        buf = Buffer.alloc(0);
        return;
      }
      if (sof > 0) buf = buf.subarray(sof);
      if (buf.length < 5) return;
      const len = buf[1];
      if (buf.length < len + 5) return;
      const raw = buf.subarray(0, len + 5);
      buf = buf.subarray(len + 5);
      let fcs = 0;
      for (let i = 1; i < len + 4; i++) fcs ^= raw[i];
      if (fcs !== raw[len + 4]) continue;
      onFrame(Buffer.from(raw), t);
    }
  };
}

function decode(raw, t, d) {
  const type = (raw[2] & 0xe0) >> 5, subsystem = raw[2] & 0x1f, cmd = raw[3];
  const data = raw.subarray(4, raw.length - 1);
  const f = { t, d, type: TYPES[type], subsystem: SUBSYSTEMS[subsystem] ?? subsystem, cmd, name: `${subsystem}.${cmd}`, data, raw, payload: null };
  try {
    const z = ZpiObject.fromUnpiFrame(new Frame(type, subsystem, cmd, Buffer.from(data)));
    f.name = `${f.subsystem}.${z.command.name}`;
    f.payload = z.payload;
  } catch {}
  return f;
}

function frames(file) {
  const out = [];
  const conns = new Map();
  for (const line of fs.readFileSync(file, "utf8").split("\n")) {
    if (!line) continue;
    const r = JSON.parse(line);
    if (!r.x) continue;
    const key = `${r.c}:${r.d}`;
    if (!conns.has(key)) conns.set(key, splitter((raw, t) => out.push(decode(raw, t, r.d))));
    conns.get(key)(Buffer.from(r.x, "hex"), r.t);
  }
  return out;
}

const show = (v) => JSON.stringify(v, (k, x) => (typeof x === "bigint" ? String(x) : x && x.type === "Buffer" ? Buffer.from(x.data).toString("hex") : x));

if (require.main === module) {
  const [file, opt] = process.argv.slice(2);
  for (const f of frames(file)) {
    if (opt === "--json") console.log(show({ t: f.t, d: f.d, type: f.type, name: f.name, payload: f.payload }));
    else console.log(`${(f.t / 1000).toFixed(3).padStart(9)} ${f.d === "up" ? ">>" : "<<"} ${f.type} ${f.name} ${show(f.payload).slice(0, 200)}`);
  }
}

module.exports = { frames, decode, splitter, show };
