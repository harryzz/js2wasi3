// Property access like zigbee2mqtt's: instances with > 8 fields (hashed), methods through the prototype chain,
// option objects read by name, missing properties.
class Base { describe() { return this.name.length; } }
class Device extends Base {
  constructor(i) {
    super();
    this.ieeeAddr = "0x" + i; this.networkAddress = i; this.manufacturerID = i & 7; this.modelID = "m" + (i & 15);
    this.powerSource = "Mains"; this.interviewCompleted = true; this.name = "dev" + i; this.endpoints = [];
    this.lastSeen = i; this.linkquality = i & 255;
  }
  score(opts) { return this.linkquality + (opts.bonus ?? 0) + (this.missing === undefined ? 1 : 0) + this.describe(); }
}
const devices = [];
for (let i = 0; i < 1000; i++) devices.push(new Device(i));
const opts = { bonus: 2, verbose: false, retain: true };
const t0 = Date.now();
let sum = 0;
for (let r = 0; r < 300; r++) for (const d of devices) sum += d.score(opts) + d.networkAddress + (d.interviewCompleted ? 1 : 0);
console.log(sum, Date.now() - t0 >= 0);
