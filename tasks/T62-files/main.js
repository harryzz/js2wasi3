// T62 driver: index.js's start() (T70: with its git hash check, not the dist rebuild) and with
// static requires, so the whole graph is linked at build time (import()-only modules turn build
// errors into run-time rejections).
const {onboard} = require("./dist/util/onboarding.js");
const {Controller} = require("./dist/controller.js");
let controller;
async function exit(code, restart = false) { if (!restart) process.exit(code); }
async function restart() { await controller?.stop(true); await start(); }
// T70: index.js's `checkDist()` asks git for the commit hash first (`child_process.exec`); without git (js2wasi3:
// every spawn fails with ENOENT) it gets "unknown" and builds nothing, as node does outside a checkout. Its
// process I/O is also a turn of the event loop before `onboard()` (a restart's old socket closes then).
async function currentHash() {
    return await new Promise((resolve) => {
        require("node:child_process").exec("git rev-parse --short=8 HEAD", (error, stdout) => {
            const commitHash = stdout.trim();
            resolve(error || commitHash === "" ? "unknown" : commitHash);
        });
    });
}
// T70: index.js's `checkDist()` asks git for the commit hash first (`child_process.exec`); without git (js2wasi3:
// every spawn fails with ENOENT) it gets "unknown" and builds nothing, as node does outside a checkout. Its
// process I/O is also a turn of the event loop before `onboard()` (a restart's old socket closes then).
async function currentHash() {
    return await new Promise((resolve) => {
        require("node:child_process").exec("git rev-parse --short=8 HEAD", (error, stdout) => {
            const commitHash = stdout.trim();
            resolve(error || commitHash === "" ? "unknown" : commitHash);
        });
    });
}
async function start() {
    console.log("Starting Zigbee2MQTT without watchdog.");
    await currentHash();
    await currentHash();
    if (!(await onboard())) return await exit(1);
    controller = new Controller(restart, exit);
    await controller.start();
}
void start();
