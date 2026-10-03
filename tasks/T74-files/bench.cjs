// T74 benchmark: zigbee2mqtt's start-up work that is property access heavy, offline and deterministic.
// 1. load zigbee-herdsman-converters (its 390 device modules build their definitions),
// 2. find the definition of each of 52 device types (devices.json: model, manufacturer, clusters of a real
//    installation; no addresses),
// 3. build each one's exposes and serialize them (as Home Assistant discovery does), ROUNDS times.
// Run from a zigbee2mqtt package directory (zigbee-herdsman-converters in node_modules); prints a checksum
// and per-phase times (stderr), so node and js2wasi3 can be compared.
const ROUNDS = Number(process.env.BENCH_ROUNDS || 200);
const t0 = performance.now();
const zhc = require("zigbee-herdsman-converters");
const quiet = () => {};
zhc.setLogger({ debug: quiet, info: quiet, warning: quiet, error: quiet });
const devices = require("./devices.json");
const t1 = performance.now();

function fakeDevice(d, i) {
  const endpoints = d.endpoints.map((e) => ({
    ID: e.ID,
    profileID: e.profileID,
    deviceID: e.deviceID,
    inputClusters: e.inputClusters,
    outputClusters: e.outputClusters,
    clusters: {},
    binds: [],
    configuredReportings: [],
    supportsInputCluster: (c) => e.inputClusters.includes(typeof c === "number" ? c : -1),
    supportsOutputCluster: (c) => e.outputClusters.includes(typeof c === "number" ? c : -1),
    getInputClusters: () => [],
    getOutputClusters: () => [],
    getDevice: () => device,
  }));
  const device = {
    ieeeAddr: "0x" + (0x1000 + i).toString(16).padStart(16, "0"),
    networkAddress: i + 1,
    type: d.type,
    manufacturerName: d.manufacturerName ?? undefined,
    modelID: d.modelID ?? undefined,
    powerSource: d.powerSource,
    manufacturerID: d.manufacturerID,
    applicationVersion: d.applicationVersion,
    stackVersion: d.stackVersion,
    hardwareVersion: d.hardwareVersion,
    dateCode: d.dateCode,
    softwareBuildID: d.softwareBuildID ?? undefined,
    zclVersion: d.zclVersion,
    interviewCompleted: true,
    meta: {},
    endpoints,
    getEndpoint: (id) => endpoints.find((e) => e.ID === id),
    customClusters: {},
  };
  return device;
}

async function main() {
  const fakes = devices.map(fakeDevice);
  const defs = [];
  for (const d of fakes) defs.push(await zhc.findDefinition(d, true));
  const t2 = performance.now();
  let sum = 0;
  let found = 0;
  for (let r = 0; r < ROUNDS; r++) {
    for (let i = 0; i < defs.length; i++) {
      const def = defs[i];
      if (!def) continue;
      if (r === 0) found++;
      let exposes = def.exposes;
      if (typeof exposes === "function") {
        try {
          exposes = exposes(fakes[i], {});
        } catch {
          exposes = [];
        }
      }
      const text = JSON.stringify({ model: def.model, vendor: def.vendor, description: def.description, exposes });
      for (let k = 0; k < text.length; k += 7) sum = (sum * 31 + text.charCodeAt(k)) | 0;
    }
  }
  const t3 = performance.now();
  console.log("definitions found:", found, "of", defs.length, "checksum:", sum);
  console.error(`load ${(t1 - t0).toFixed(0)} ms, find ${(t2 - t1).toFixed(0)} ms, exposes x${ROUNDS} ${(t3 - t2).toFixed(0)} ms`);
}
main();
