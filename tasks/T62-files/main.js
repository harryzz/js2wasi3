// T62 driver: index.js's start() without the dist hash check (git via child_process) and with
// static requires, so the whole graph is linked at build time (import()-only modules turn build
// errors into run-time rejections).
const {onboard} = require("./dist/util/onboarding.js");
const {Controller} = require("./dist/controller.js");
let controller;
async function exit(code, restart = false) { if (!restart) process.exit(code); }
async function restart() { await controller?.stop(true); await start(); }
async function start() {
    console.log("Starting Zigbee2MQTT without watchdog.");
    if (!(await onboard())) return await exit(1);
    controller = new Controller(restart, exit);
    await controller.start();
}
void start();
