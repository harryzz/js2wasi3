// T71: a frontend client for the zigbee2mqtt runs: GETs and a WebSocket on /api; prints what it got (masked
// where it changes from run to run). Usage: node client.cjs <port> <ws-package-dir>
const http = require("http");
const WebSocket = require(process.argv[3]);
const port = Number(process.argv[2]);
const get = (path, headers = {}) => new Promise((resolve, reject) => {
  http.get({ host: "127.0.0.1", port, path, headers }, (res) => {
    const chunks = [];
    res.on("data", (d) => chunks.push(d));
    res.on("end", () => resolve([res, Buffer.concat(chunks)]));
  }).on("error", reject);
});
async function main() {
  for (const [path, headers] of [["/", {}], ["/", { "accept-encoding": "br" }], ["/index.html", {}], ["/missing", {}],
    ["/device_icons/x.png", {}]]) {
    const [res, body] = await get(path, headers);
    const h = res.headers;
    console.log(path, JSON.stringify(headers), res.statusCode, h["content-type"], h["content-encoding"], h["content-length"],
      h["cache-control"], body.length, JSON.stringify(body.toString("latin1").slice(0, 60)));
  }
  // An asset named in index.html.
  const [, html] = await get("/");
  const asset = /assets\/[^"]+\.js/.exec(html.toString())[0];
  const [res, body] = await get(`/${asset}`, { "accept-encoding": "gzip, br" });
  console.log("asset", res.statusCode, res.headers["content-type"], res.headers["content-encoding"], body.length > 0);
  const ws = new WebSocket(`ws://127.0.0.1:${port}/api`);
  const topics = new Map();
  ws.on("message", (data) => {
    const m = JSON.parse(data.toString());
    topics.set(m.topic, (topics.get(m.topic) || 0) + 1);
  });
  await new Promise((resolve, reject) => {
    ws.on("open", resolve);
    ws.on("error", reject);
  });
  console.log("ws open");
  await new Promise((r) => setTimeout(r, 3000));
  ws.send(JSON.stringify({ topic: "bridge/request/health_check", payload: {} }));
  await new Promise((r) => setTimeout(r, 2000));
  console.log("ws topics", [...topics.keys()].sort().join(" "));
  ws.close();
  await new Promise((r) => ws.on("close", r));
  console.log("ws closed");
}
main().catch((e) => {
  console.log("client error", e.message);
  process.exitCode = 1;
});
