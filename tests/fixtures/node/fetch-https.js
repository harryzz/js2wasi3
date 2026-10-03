// `fetch` over HTTPS to public hosts (T72; needs the network): only the stable parts are printed.
// zigbee-herdsman's OTA check reads this index the same way (`response.ok`, `json()`).
const r = await fetch("https://raw.githubusercontent.com/Koenkk/zigbee-OTA/master/index.json");
console.log(r.status, r.statusText, r.ok, r.type, r.url, r.redirected, r.body !== null);
console.log(r.headers.get("content-type"));
const index = await r.json();
console.log(Array.isArray(index), index.length > 100, typeof index[0].fileName, typeof index[0].url);

// A redirect from http to https.
const e = await fetch("http://github.com/Koenkk/zigbee-OTA", { method: "HEAD" });
console.log(e.status, e.ok, e.redirected, e.url, e.body);

const bytes = Buffer.from(await (await fetch("https://example.com/")).arrayBuffer());
console.log(bytes.includes("<title>Example Domain</title>"));

try {
  await fetch("https://nonexistent.invalid/");
} catch (err) {
  console.log(err.message, err.cause.code, err.cause.message);
}
