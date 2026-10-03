// T68 probe: mqtt.js round-trip latency on a test topic (not retained). In $S/zp/package:
//   Z2M_MQTT=mqtt://<broker>:1883 node mqtt-probe.js; js2wasi3 build --command mqtt-probe.js -o p.wasm and
//   wasmtime run -Sp3 -S inherit-network -S allow-ip-name-lookup --env Z2M_MQTT=… p.wasm
const mqtt = require("mqtt");
const t0 = Date.now();
const lap = (what) => console.log(what, Date.now() - t0, "ms");
(async () => {
  const c = await mqtt.connectAsync(process.env.Z2M_MQTT, { clientId: "zigbee2mqtt-js2wasi3-probe" });
  lap("connected");
  await c.publishAsync("zigbee2mqtt-js2wasi3/probe", "1", { qos: 1 });
  lap("publish qos1");
  await c.subscribeAsync("zigbee2mqtt-js2wasi3/probe");
  lap("subscribe");
  const got = new Promise((r) => c.once("message", r));
  await c.publishAsync("zigbee2mqtt-js2wasi3/probe", "2", { qos: 0 });
  await got;
  lap("echo");
  for (let i = 0; i < 5; i++) await c.publishAsync("zigbee2mqtt-js2wasi3/probe", "x", { qos: 1 });
  lap("5 x qos1");
  await c.endAsync();
  lap("end");
})();
