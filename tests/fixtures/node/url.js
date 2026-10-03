// WHATWG `URL` (T64): parsing, serializing, getters and setters, `URL.canParse`/`URL.parse`, inspect.
const show = (u) => [u.href, u.origin, u.protocol, u.username, u.password, u.host, u.hostname, u.port,
  u.pathname, u.search, u.hash].join(" | ");

const inputs = [
  ["https://user:pa ss@EXAMPLE.com:443/a/./b/../c d?q=1 2&r=é#frag ment"],
  ["HTTP://example.com:80/"],
  ["http://example.com:8080"],
  ["tcp://192.168.1.10:6638"],
  ["tcp://host:1234/path"],
  ["mqtt://broker.local"],
  ["ws://[::1]:8080/socket"],
  ["http://[2001:DB8:0:0:0:0:0:1]/"],
  ["http://[::ffff:192.168.0.1]/"],
  ["http://[1:0:0:2:0:0:0:3]/"],
  ["http://0x7f.1/"],
  ["http://0300.0250.0.1/"],
  ["http://4294967295/"],
  ["http://192.168.257/"],
  ["file:///etc/passwd"],
  ["file://localhost/etc/x"],
  ["file://server/share/x"],
  ["file:c:/windows/x"],
  ["file:///C|/foo/../../bar"],
  ["mailto:someone@example.com"],
  ["data:text/plain,hello world"],
  ["javascript:alert(1)"],
  ["urn:isbn:0451450523"],
  ["web+demo:/.//not-a-host/"],
  ["web+demo://host/a b"],
  ["foo://"],
  ["foo:/"],
  ["sc:\\../"],
  ["https://example.com/%7Efoo/%2e%2E/bar"],
  ["https://ex.com/a?b#c?d#e"],
  ["  \t https://ex\nample.com/\r\n  "],
  ["https://ex.com/{}|^`[]\"<>"],
  ["https://ex.com/?'\"<>`{}"],
  ["foo://ex.com/?'\"<>`{}"],
  ["https://ex.com/#\"<>` {}"],
  ["https://@@ex.com"],
  ["https://u@ex.com"],
  ["https://:p@ex.com"],
  ["https://a:b:c@ex.com"],
  ["https://ex.com:/"],
  ["https://ex.com:0080/"],
  ["blob:https://example.com/uuid"],
  ["blob:foo"],
  ["https://EXAMPLE.COM./"],
  ["https://xn--nxasmq6b.com/"],
  ["https://bücher.de/"],
  ["https://Ｅｘａｍｐｌｅ。com/"],
  ["//foo/bar", "https://base.com/x"],
  ["../c?q#h", "https://base.com/a/b/x"],
  ["?q", "https://base.com/a/b?old#h"],
  ["#h2", "https://base.com/a?q#h"],
  ["", "https://base.com/a?q#h"],
  ["/abs", "file:///C:/dir/x"],
  ["x", "mailto:a@b"],
  ["#x", "mailto:a@b"],
  ["\\\\server\\share", "file:///"],
  ["http:foo", "http://base.com/a/"],
  ["http:/foo", "https://base.com/a/"],
];
for (const [input, base] of inputs) {
  try {
    console.log(JSON.stringify(input), show(base === undefined ? new URL(input) : new URL(input, base)));
  } catch (e) {
    console.log(JSON.stringify(input), e.name, e.code, e.message, e.input, e.base);
  }
}

for (const bad of ["", "foo", "http://", "https://ex ample.com", "http://[::1", "http://a:99999",
  "http://1.2.3.4.5", "http://256.0.0.0", "http://ex%41mple.com", "http://exa%00mple.com", "https://xn--a.com",
  "http://[1::2::3]/", "http://[1:2:3:4:5:6:7:8:9]/", "http://u@/", "http://a<b/"]) {
  try { console.log(new URL(bad).href); } catch (e) { console.log(JSON.stringify(bad), e.name, e.code, e.message); }
}
try { new URL("/x", "not a base"); } catch (e) { console.log(e.code, e.message, e.input, e.base); }
try { new URL(); } catch (e) { console.log(e.name, e.code, e.message); }

console.log(URL.canParse("https://a"), URL.canParse("a"), URL.canParse("a", "https://b"), URL.canParse.length);
console.log(URL.parse("nope"), URL.parse("b", "https://a/").href, URL.length, URL.name);

// Setters.
const u = new URL("https://user:pass@example.com:8080/p/a/t/h?query=string#hash");
u.protocol = "http"; console.log(u.href);
u.protocol = "foo"; console.log(u.href);
u.protocol = "wss:x"; console.log(u.href);
u.username = "us er@"; u.password = "p:w/"; console.log(u.href, u.username, u.password);
u.host = "other.org:99"; console.log(u.href);
u.host = "third.org"; console.log(u.href);
u.hostname = "h.org:12"; console.log(u.href);
u.hostname = ""; console.log(u.href);
u.port = "443"; console.log(u.href, JSON.stringify(u.port));
u.port = "8443abc"; console.log(u.href);
u.port = ""; console.log(u.href);
u.port = "70000"; console.log(u.href);
u.port = 81; console.log(u.href);
u.pathname = "/x y/../z?#"; console.log(u.href, u.pathname);
u.pathname = "rel"; console.log(u.href);
u.search = "a=1&b= 2#"; console.log(u.href, u.search);
u.search = "?"; console.log(u.href, JSON.stringify(u.search));
u.search = ""; console.log(u.href);
u.hash = "frag ment"; console.log(u.href, u.hash);
u.hash = "#"; console.log(u.href, JSON.stringify(u.hash));
u.hash = ""; console.log(u.href);
u.href = "mqtt://new.host:1883/"; console.log(u.href, u.port);
try { u.href = "nope"; } catch (e) { console.log(e.name, e.code, e.message); }
const m = new URL("mailto:a@b?x");
m.host = "h"; m.pathname = "p"; m.port = "1"; m.username = "u"; console.log(m.href);
m.search = ""; console.log(m.href);
const sp = new URL("data:space ?q#f");
sp.hash = ""; console.log(JSON.stringify(sp.href)); sp.search = ""; console.log(JSON.stringify(sp.href), JSON.stringify(sp.pathname));
const f = new URL("file:///C:/a/b");
f.host = "server"; console.log(f.href); f.port = "1"; f.username = "u"; console.log(f.href);
f.protocol = "http"; console.log(f.href);
const h = new URL("http://u:p@h.com:81/");
h.protocol = "file"; console.log(h.href);
h.host = ""; console.log(h.href);
const nh = new URL("foo://h/x"); nh.host = ""; console.log(nh.href); nh.protocol = "http"; console.log(nh.href);

// searchParams stays in sync with the URL.
const s = new URL("https://ex.com/?a=1&b=2&a=3#h");
const params = s.searchParams;
console.log(params.get("a"), params.getAll("a"), params.size, s.searchParams === params);
params.append("c", "x y"); console.log(s.href, s.search);
params.delete("a"); console.log(s.href);
s.search = "?z=9"; console.log(params.get("z"), params.size, [...params.keys()]);
params.delete("z"); console.log(s.href, JSON.stringify(s.search));
s.href = "https://ex.com/?k=v"; console.log(params.get("k"));

console.log(new URL("https://a.com/p?q=1#h"));
console.log([new URL("foo:bar")], { u: new URL("https://u:p@a.com:1/") });
console.log(String(new URL("https://a.com")), JSON.stringify({ u: new URL("https://a.com/x") }), `${new URL("http://b")}`);
console.log(Object.prototype.toString.call(u), Object.keys(u), new URL("https://a.com") instanceof URL);
console.log(Object.getOwnPropertyNames(URL.prototype).join());
console.log(new URL(new URL("https://obj.com/x")).href, new URL({ toString() { return "https://ts.com"; } }).href);
