// T72: zigbee-herdsman 10.9.2's OTA helpers (dist/controller/helpers/ota.js, zigbee2mqtt 2.14.1's), whose `fetch`
// calls are zigbee2mqtt's only ones: the index from Koenkk/zigbee-OTA, then the smallest image with a sha512
// (downloaded, checked, parsed). Run from zigbee2mqtt's package directory (see run-ota.sh).
const ota = require("zigbee-herdsman/dist/controller/helpers/ota");

async function main() {
  const index = await ota.getOtaIndex({});
  console.log("index:", Array.isArray(index), index.length > 100);
  const withHash = index.filter((m) => typeof m.sha512 === "string" && typeof m.fileSize === "number" && /^https:/.test(m.url));
  withHash.sort((a, b) => a.fileSize - b.fileSize || (a.fileName < b.fileName ? -1 : 1));
  const meta = withHash[0];
  console.log("smallest:", meta.fileName, meta.fileSize, meta.url);
  const image = ota.parseOtaImage(await ota.getOtaFirmware(meta.url, meta.sha512));
  const h = image.header;
  console.log("header:", h.manufacturerCode === meta.manufacturerCode, h.imageType === meta.imageType,
    h.fileVersion === meta.fileVersion, h.totalImageSize, image.elements.length, image.raw.length);
  try {
    await ota.getOtaFirmware(meta.url, "0".repeat(128));
  } catch (e) {
    console.log("bad hash:", e.message);
  }
  try {
    await ota.getOtaIndex({ url: "https://raw.githubusercontent.com/Koenkk/zigbee-OTA/master/no-such-index.json" });
  } catch (e) {
    console.log("missing index:", e.message);
  }
  try {
    await ota.getOtaIndex({ url: "https://nonexistent.invalid/index.json" });
  } catch (e) {
    console.log("bad host:", e.message, e.cause && e.cause.code);
  }
}
main();
