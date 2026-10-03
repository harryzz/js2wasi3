// node's `url` module (T64): `parse`, `format`, `resolve`, file URLs, `urlToHttpOptions`, `domainTo*`.
import url from "node:url";
import { fileURLToPath, pathToFileURL, urlToHttpOptions, URL as U2 } from "node:url";
console.log(url.parse("mqtt://user:pw@Host.com:1883/a/b?x=1&y=2&x=3#h", true));
console.log(url.parse("http://a.com/p?q=1"));
console.log(url.parse("//host/p", false, true).host, url.parse("//host/p").host, url.parse("/just/a/path?x").pathname);
console.log(url.parse("HTTP://[::1]:80/x").host, url.parse("http://bücher.de/").hostname, url.parse("javascript:alert(1)").pathname);
console.log(url.parse("http://a.com:80\\b\\c").href, url.parse("  http://x.com/ y ").href, url.parse("mailto:a@b.c").href);
console.log(url.parse("tcp://192.168.1.2:6638").port, url.parse("ws://h:9/").protocol, url.parse("a b").href);
try { url.parse(5); } catch (e) { console.log(e.name, e.code, e.message); }
console.log(url.format({ protocol: "https", hostname: "ex.com", pathname: "/a b", query: { x: "1 2", y: ["a", "b"] } }));
console.log(url.format({ protocol: "mqtt:", host: "h:1", auth: "u:p@", hash: "x", search: "s=1" }));
console.log(url.format(url.parse("http://u:p@a.com/x?y#z")), url.format("http://A.com/b"));
const w = new URL("https://u:p@bücher.de/x?y=1#z");
console.log(url.format(w), url.format(w, { fragment: false, unicode: true, search: false, auth: false }));
console.log(url.resolve("/one/two/three", "four"), url.resolve("http://example.com/", "/one"), url.resolve("http://example.com/one", "../two"));
console.log(url.resolve("http://a/b/c/d;p?q", "g?y#s"), url.resolve("mqtt://h:1/x", "//other/y"));
console.log(fileURLToPath("file:///tmp/a%20b/c"), fileURLToPath(new URL("file:///x/%E2%82%AC")));
for (const bad of ["http://x/", "file://host/x", "file:///a%2Fb"]) {
  try { fileURLToPath(bad); } catch (e) { console.log(e.name, e.code, e.message); }
}
try { fileURLToPath(5); } catch (e) { console.log(e.name, e.code, e.message); }
console.log(pathToFileURL("/tmp/a b/%/#?/c\\d").href, pathToFileURL("/tmp/dir/").href, pathToFileURL("/").href);
console.log(urlToHttpOptions(new URL("https://us%20er:p%40ss@[::1]:8443/p/a?q=1#h")));
console.log(urlToHttpOptions(new URL("tcp://192.168.1.10:6638")));
console.log(url.domainToASCII("español.com"), url.domainToASCII("xn--iñvalid.com"), url.domainToUnicode("xn--espaol-zwa.com"));
console.log(U2 === URL, url.URLSearchParams === URLSearchParams, typeof url.Url, Object.keys(url).filter((k) => k !== "URLPattern").sort().join());
