// T76 micro-benchmark: JSON.stringify of Home Assistant discovery-like payloads (nested objects, string-heavy, a few
// numbers and arrays), a few with characters to escape; JSON.parse of the result. Checksums on stdout, times on stderr.
function time(name, f) {
  const t = performance.now();
  const r = f();
  console.error(`${name} ${(performance.now() - t).toFixed(0)} ms`);
  console.log(name, r);
}
const payloads = [];
for (let i = 0; i < 50; i++) {
  payloads.push({
    availability: [{ topic: "zigbee2mqtt/bridge/state", value_template: "{{ value_json.state }}" }],
    device: { identifiers: ["zigbee2mqtt_0x00158d00" + i], manufacturer: "Tuya", model: "TS0201 temperature & humidity", name: "sensor " + i, via_device: "zigbee2mqtt_bridge_0x00124b" },
    device_class: "temperature",
    enabled_by_default: true,
    object_id: "sensor_" + i + "_temperature",
    origin: { name: "Zigbee2MQTT", sw: "2.14.1", url: "https://www.zigbee2mqtt.io" },
    state_class: "measurement",
    state_topic: "zigbee2mqtt/sensor " + i,
    unique_id: "0x00158d00" + i + "_temperature_zigbee2mqtt",
    unit_of_measurement: "°C",
    value_template: i % 10 === 0 ? "{{ value_json.temperature }}\n\"quoted\"" : "{{ value_json.temperature }}",
    exposes: [{ type: "numeric", name: "temperature", property: "temperature", access: 1, unit: "°C", value_min: -40, value_max: 125, value_step: 0.1 }],
  });
}
let text = "";
time("stringify", () => {
  let n = 0;
  for (let r = 0; r < 100; r++) for (const p of payloads) { text = JSON.stringify(p); n += text.length; }
  return n;
});
time("indent", () => {
  let n = 0;
  for (let r = 0; r < 20; r++) n += JSON.stringify(payloads, null, 2).length;
  return n;
});
time("parse", () => {
  let n = 0;
  for (let r = 0; r < 100; r++) for (const p of payloads) n += JSON.parse(JSON.stringify(p)).object_id.length;
  return n;
});
const names = ["local_temperature_calibration", "temperature", "max_temperature_limit", "child_lock", "away_preset_days", "power_on_behavior"];
time("replace", () => {
  let n = 0;
  for (let r = 0; r < 5000; r++)
    for (const name of names) {
      const label = name.replace(/_/g, " ");
      n += (label.length === 0 ? label : label[0].toUpperCase() + label.slice(1)).length;
    }
  return n;
});
