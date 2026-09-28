//! Bare specifiers (T41 slice 5): node's ESM resolution of packages, at build time.
//!
//! `pkg`, `pkg/sub` and `@scope/pkg/sub` are looked up in `node_modules` from the importer's
//! directory upwards (PACKAGE_RESOLVE), or in the importer's own package when it has that name
//! (PACKAGE_SELF_RESOLVE); `#name` in the nearest `package.json`'s `imports`. A package's `exports`
//! (strings, arrays of fallbacks, condition objects, `*` patterns, `null`) are matched with node's
//! conditions (`node`, `import`, `module-sync`, `default`, in the object's key order). Without
//! `exports`, `pkg` is its `main` (tried as is, with `.js`/`.json`, as a directory's `index.js`) or
//! `index.js`, and `pkg/sub` the file itself. The `module` field is ignored, as node does.

use crate::link::Host;

/// node's conditions for `import` (node 24).
const CONDITIONS: &[&str] = &["node", "import", "module-sync", "node-addons", "default"];

/// node's built-in modules that need no `node:` prefix.
const BUILTINS: &[&str] = &[
    "assert", "async_hooks", "buffer", "child_process", "cluster", "console", "constants", "crypto", "dgram",
    "diagnostics_channel", "dns", "domain", "events", "fs", "http", "http2", "https", "inspector", "module", "net",
    "os", "path", "perf_hooks", "process", "punycode", "querystring", "readline", "repl", "stream", "string_decoder",
    "sys", "timers", "tls", "trace_events", "tty", "url", "util", "v8", "vm", "wasi", "worker_threads", "zlib",
];

/// A JSON value (object keys in their order).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(o) => o.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// A JSON text's value and the spans of its `"__proto__"` keys, or where it isn't JSON and why.
pub(crate) fn parse_json(text: &str) -> Result<(Json, Vec<(usize, usize)>), (usize, String)> {
    let mut p = P { s: text.as_bytes(), at: 0, proto: Vec::new() };
    let v = p.value()?;
    p.ws();
    if p.at < text.len() {
        return p.fail("the end of the text");
    }
    Ok((v, p.proto))
}

struct P<'s> {
    s: &'s [u8],
    at: usize,
    proto: Vec<(usize, usize)>,
}

impl P<'_> {
    fn ws(&mut self) {
        while self.at < self.s.len() && matches!(self.s[self.at], b' ' | b'\t' | b'\n' | b'\r') {
            self.at += 1;
        }
    }
    fn fail<T>(&self, what: &str) -> Result<T, (usize, String)> {
        let found = match self.s.get(self.at) {
            None => "the end of the text".to_string(),
            Some(_) => {
                let c = std::str::from_utf8(&self.s[self.at..]).ok().and_then(|t| t.chars().next());
                format!("`{}`", c.unwrap_or('?'))
            }
        };
        Err((self.at, format!("expected {what}, found {found}")))
    }
    fn eat(&mut self, c: u8) -> bool {
        let ok = self.s.get(self.at) == Some(&c);
        self.at += ok as usize;
        ok
    }
    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.s.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        self.at - start
    }
    fn string(&mut self) -> Result<String, (usize, String)> {
        let mut value = Vec::new();
        self.at += 1;
        loop {
            match self.s.get(self.at) {
                None => return self.fail("`\"`"),
                Some(b'"') => break,
                Some(c) if *c < 0x20 => return self.fail("a character (control characters must be escaped)"),
                Some(b'\\') => {
                    self.at += 1;
                    match self.s.get(self.at) {
                        Some(b'u') => {
                            let hex = self.s.get(self.at + 1..self.at + 5).and_then(|h| std::str::from_utf8(h).ok());
                            let Some(u) = hex
                                .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
                                .and_then(|h| u16::from_str_radix(h, 16).ok())
                            else {
                                self.at += 1;
                                return self.fail("four hex digits");
                            };
                            value.push(u);
                            self.at += 5;
                        }
                        Some(c) if b"\"\\/bfnrt".contains(c) => {
                            value.push(match c {
                                b'b' => 8,
                                b'f' => 12,
                                b'n' => 10,
                                b'r' => 13,
                                b't' => 9,
                                c => *c as u16,
                            });
                            self.at += 1;
                        }
                        _ => return self.fail("an escape (`\\\"`, `\\n`, `\\u0041`, …)"),
                    }
                }
                Some(_) => {
                    let rest = std::str::from_utf8(&self.s[self.at..]).unwrap_or("");
                    let c = rest.chars().next().unwrap_or('\u{fffd}');
                    let mut buf = [0u16; 2];
                    value.extend_from_slice(c.encode_utf16(&mut buf));
                    self.at += c.len_utf8().max(1);
                }
            }
        }
        self.at += 1;
        Ok(String::from_utf16_lossy(&value))
    }
    fn value(&mut self) -> Result<Json, (usize, String)> {
        self.ws();
        match self.s.get(self.at) {
            Some(b'{') => {
                self.at += 1;
                self.ws();
                let mut obj = Vec::new();
                if self.eat(b'}') {
                    return Ok(Json::Obj(obj));
                }
                loop {
                    self.ws();
                    if self.s.get(self.at) != Some(&b'"') {
                        return self.fail("a property name in double quotes");
                    }
                    let start = self.at;
                    let key = self.string()?;
                    if key == "__proto__" {
                        self.proto.push((start, self.at));
                    }
                    self.ws();
                    if !self.eat(b':') {
                        return self.fail("`:`");
                    }
                    obj.push((key, self.value()?));
                    self.ws();
                    if self.eat(b'}') {
                        return Ok(Json::Obj(obj));
                    }
                    if !self.eat(b',') {
                        return self.fail("`,` or `}`");
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                self.ws();
                let mut arr = Vec::new();
                if self.eat(b']') {
                    return Ok(Json::Arr(arr));
                }
                loop {
                    arr.push(self.value()?);
                    self.ws();
                    if self.eat(b']') {
                        return Ok(Json::Arr(arr));
                    }
                    if !self.eat(b',') {
                        return self.fail("`,` or `]`");
                    }
                }
            }
            Some(b'"') => self.string().map(Json::Str),
            Some(b'-' | b'0'..=b'9') => {
                let start = self.at;
                self.eat(b'-');
                if !self.eat(b'0') && self.digits() == 0 {
                    return self.fail("a digit");
                }
                if self.eat(b'.') && self.digits() == 0 {
                    return self.fail("a digit");
                }
                if self.eat(b'e') || self.eat(b'E') {
                    let _ = self.eat(b'+') || self.eat(b'-');
                    if self.digits() == 0 {
                        return self.fail("a digit");
                    }
                }
                let text = std::str::from_utf8(&self.s[start..self.at]).unwrap_or("0");
                Ok(Json::Num(text.parse().unwrap_or(f64::NAN)))
            }
            _ => {
                for (word, v) in [("true", Json::Bool(true)), ("false", Json::Bool(false)), ("null", Json::Null)] {
                    if self.s[self.at..].starts_with(word.as_bytes()) {
                        self.at += word.len();
                        return Ok(v);
                    }
                }
                self.fail("a JSON value")
            }
        }
    }
}

/// Why a bare specifier doesn't resolve: node's error code and a message.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Failure {
    pub code: &'static str,
    pub message: String,
}

fn fail<T>(code: &'static str, message: String) -> Result<T, Failure> {
    Err(Failure { code, message })
}

/// Is `spec` a node built-in (`fs`, `path/posix`) without its `node:` prefix?
pub(crate) fn is_builtin(spec: &str) -> bool {
    BUILTINS.contains(&spec.split('/').next().unwrap_or(spec))
}

/// Resolve a bare specifier (`pkg`, `pkg/sub`, `@scope/pkg`) or a package import (`#name`) from
/// directory `dir` (ends with `/`, or empty for the working directory): the module's path.
pub(crate) fn resolve(host: &mut dyn Host, dir: &str, spec: &str) -> Result<String, Failure> {
    if spec.starts_with('#') {
        return imports(host, dir, spec);
    }
    // The package name and the subpath (`.` or `./sub`).
    let mut parts = spec.splitn(if spec.starts_with('@') { 3 } else { 2 }, '/');
    let name = if spec.starts_with('@') {
        match (parts.next(), parts.next()) {
            (Some(scope), Some(n)) if !n.is_empty() => format!("{scope}/{n}"),
            _ => return fail("ERR_INVALID_MODULE_SPECIFIER", format!("Invalid module \"{spec}\" is not a valid package name")),
        }
    } else {
        parts.next().unwrap_or_default().to_string()
    };
    if name.is_empty() || name.starts_with('.') || name.contains('\\') || name.contains('%') {
        return fail("ERR_INVALID_MODULE_SPECIFIER", format!("Invalid module \"{spec}\" is not a valid package name"));
    }
    let sub = match parts.next() {
        Some(rest) => format!("./{rest}"),
        None => ".".to_string(),
    };
    // PACKAGE_SELF_RESOLVE: the importer's own package by its name.
    if let Some((root, pj)) = nearest_package(host, dir)
        && pj.get("name").and_then(Json::str) == Some(name.as_str())
        && let Some(exports) = pj.get("exports")
    {
        return exports_resolve(&root, &sub, exports);
    }
    for base in ancestors(dir) {
        let root = format!("{base}node_modules/{name}/");
        let pj = match host.read(&format!("{root}package.json")) {
            Ok(text) => Some(package_json(&text, &root)?),
            // A package directory without `package.json`.
            Err(_) if host.list(root.trim_end_matches('/')).is_ok() => None,
            Err(_) => continue,
        };
        if let Some(exports) = pj.as_ref().and_then(|p| p.get("exports")) {
            return exports_resolve(&root, &sub, exports);
        }
        if sub == "." {
            return legacy_main(host, &root, pj.as_ref(), spec);
        }
        return Ok(format!("{root}{}", &sub[2..]));
    }
    fail("ERR_MODULE_NOT_FOUND", format!("Cannot find package '{name}'"))
}

/// The directories from `dir` up to the file system's root (`dir` itself first).
fn ancestors(dir: &str) -> Vec<String> {
    let abs = crate::link::absolute_dir(dir);
    let mut out = Vec::new();
    let mut d = abs.as_str();
    loop {
        out.push(d.to_string());
        let trimmed = d.trim_end_matches('/');
        match trimmed.rfind('/') {
            Some(k) => d = &d[..=k],
            None => break,
        }
        if trimmed.is_empty() {
            break;
        }
    }
    out
}

/// The nearest `package.json` from `dir` up: its directory and contents.
fn nearest_package(host: &mut dyn Host, dir: &str) -> Option<(String, Json)> {
    for base in ancestors(dir) {
        if base.ends_with("/node_modules/") {
            continue;
        }
        if let Ok(text) = host.read(&format!("{base}package.json")) {
            return package_json(&text, &base).ok().map(|j| (base, j));
        }
    }
    None
}

/// The "type" of the nearest `package.json` (for CommonJS detection).
pub(crate) fn package_type(host: &mut dyn Host, dir: &str) -> Option<String> {
    nearest_package(host, dir).and_then(|(_, j)| j.get("type").and_then(Json::str).map(str::to_string))
}

fn package_json(text: &str, root: &str) -> Result<Json, Failure> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    match parse_json(text) {
        Ok((j, _)) => Ok(j),
        Err((_, what)) => fail("ERR_INVALID_PACKAGE_CONFIG", format!("Invalid package config {root}package.json: {what}")),
    }
}

/// LEGACY_MAIN_RESOLVE: `main` as a file, with `.js`/`.json`, as a directory's index; else `index.js`.
fn legacy_main(host: &mut dyn Host, root: &str, pj: Option<&Json>, spec: &str) -> Result<String, Failure> {
    let mut tries = Vec::new();
    if let Some(main) = pj.and_then(|p| p.get("main")).and_then(Json::str) {
        let main = crate::link::normalize(&format!("{root}{main}"));
        for suffix in ["", ".js", ".json", ".node", "/index.js", "/index.json", "/index.node"] {
            tries.push(format!("{main}{suffix}"));
        }
    }
    tries.extend(["index.js", "index.json", "index.node"].map(|f| format!("{root}{f}")));
    for t in tries {
        if host.read(&t).is_ok() {
            return Ok(t);
        }
    }
    fail("ERR_MODULE_NOT_FOUND", format!("Cannot find package '{root}' (no `main`, no index.js) for '{spec}'"))
}

/// PACKAGE_EXPORTS_RESOLVE.
fn exports_resolve(root: &str, sub: &str, exports: &Json) -> Result<String, Failure> {
    // Sugar: a string, an array or a conditions object is the `"."` export.
    let dotted = matches!(exports, Json::Obj(o) if o.first().is_some_and(|(k, _)| k.starts_with('.')));
    let not_exported = || {
        let what = if sub == "." { "No \"exports\" main defined".to_string() } else { format!("Package subpath '{sub}' is not defined by \"exports\"") };
        Failure { code: "ERR_PACKAGE_PATH_NOT_EXPORTED", message: format!("{what} in {root}package.json") }
    };
    let map: Vec<(String, Json)> = if dotted {
        match exports {
            Json::Obj(o) => o.clone(),
            _ => unreachable!(),
        }
    } else {
        vec![(".".to_string(), exports.clone())]
    };
    let found = match_key(&map, sub);
    let Some((target, star)) = found else { return Err(not_exported()) };
    target_resolve(root, &target, star.as_deref(), false)?.ok_or_else(not_exported)
}

/// PACKAGE_IMPORTS_RESOLVE: `#name` in the nearest `package.json`'s `imports`.
fn imports(host: &mut dyn Host, dir: &str, spec: &str) -> Result<String, Failure> {
    let not_defined = |root: &str| Failure {
        code: "ERR_PACKAGE_IMPORT_NOT_DEFINED",
        message: format!("Package import specifier \"{spec}\" is not defined in package {root}package.json"),
    };
    let Some((root, pj)) = nearest_package(host, dir) else {
        return Err(not_defined(dir));
    };
    let Some(Json::Obj(map)) = pj.get("imports") else { return Err(not_defined(&root)) };
    let Some((target, star)) = match_key(map, spec) else { return Err(not_defined(&root)) };
    match target_resolve(&root, &target, star.as_deref(), true)? {
        Some(path) if path.starts_with("#bare:") => resolve(host, &root, &path["#bare:".len()..]),
        Some(path) => Ok(path),
        None => Err(not_defined(&root)),
    }
}

/// The entry of an `exports`/`imports` map for `key`: exact, or the best `*` pattern (the longest
/// prefix; PATTERN_KEY_COMPARE), with what `*` matched.
fn match_key(map: &[(String, Json)], key: &str) -> Option<(Json, Option<String>)> {
    if let Some((_, v)) = map.iter().rev().find(|(k, _)| k == key && !k.contains('*')) {
        return Some((v.clone(), None));
    }
    let mut best: Option<(&str, &Json, String)> = None;
    for (k, v) in map {
        let Some((prefix, suffix)) = k.split_once('*') else { continue };
        if key.len() >= k.len() && key.starts_with(prefix) && key.ends_with(suffix) && key != prefix {
            let better = best.as_ref().is_none_or(|(b, ..)| {
                let (bp, _) = b.split_once('*').unwrap_or((b, ""));
                prefix.len() > bp.len() || (prefix.len() == bp.len() && k.len() > b.len())
            });
            if better {
                best = Some((k, v, key[prefix.len()..key.len() - suffix.len()].to_string()));
            }
        }
    }
    best.map(|(_, v, s)| (v.clone(), Some(s)))
}

/// PACKAGE_TARGET_RESOLVE: a target string (`./dist/x.js`, with `*` replaced), a fallback array or
/// a conditions object. `None`: `null` or no condition matched. In `imports`, a target that isn't
/// `./…` is a bare specifier (returned as `#bare:<spec>`).
fn target_resolve(root: &str, target: &Json, star: Option<&str>, imports: bool) -> Result<Option<String>, Failure> {
    match target {
        Json::Str(t) => {
            let t = match star {
                Some(s) => t.replace('*', s),
                None => t.clone(),
            };
            if !t.starts_with("./") {
                if imports && !t.starts_with("../") && !t.starts_with('/') {
                    return Ok(Some(format!("#bare:{t}")));
                }
                return fail("ERR_INVALID_PACKAGE_TARGET", format!("Invalid \"exports\" target \"{t}\" in {root}package.json"));
            }
            let path = crate::link::normalize(&format!("{root}{}", &t[2..]));
            if !path.starts_with(root.trim_end_matches('/')) || path.contains("/node_modules/") && !root.contains("/node_modules/") {
                return fail("ERR_INVALID_PACKAGE_TARGET", format!("Invalid \"exports\" target \"{t}\" in {root}package.json"));
            }
            Ok(Some(path))
        }
        Json::Arr(items) => {
            let mut last = Ok(None);
            for item in items {
                match target_resolve(root, item, star, imports) {
                    Ok(Some(p)) => return Ok(Some(p)),
                    Ok(None) => last = Ok(None),
                    Err(e) => last = Err(e),
                }
            }
            last
        }
        Json::Obj(conditions) => {
            for (k, v) in conditions {
                if CONDITIONS.contains(&k.as_str())
                    && let Some(p) = target_resolve(root, v, star, imports)?
                {
                    return Ok(Some(p));
                }
            }
            Ok(None)
        }
        Json::Null => Ok(None),
        _ => fail("ERR_INVALID_PACKAGE_TARGET", format!("Invalid \"exports\" target in {root}package.json")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_values_keep_key_order() {
        let (j, proto) = parse_json(r#"{"b": [1, -2.5e1, true, null], "a": "A\n", "__proto__": {}}"#).unwrap();
        assert_eq!(
            j,
            Json::Obj(vec![
                ("b".into(), Json::Arr(vec![Json::Num(1.0), Json::Num(-25.0), Json::Bool(true), Json::Null])),
                ("a".into(), Json::Str("A\n".into())),
                ("__proto__".into(), Json::Obj(vec![])),
            ])
        );
        assert_eq!(proto.len(), 1);
    }

    #[test]
    fn exports_patterns_and_conditions() {
        let (map, _) = parse_json(
            r#"{".": {"browser": "./b.js", "import": "./i.js"}, "./f/*": "./lib/*.js", "./f/x/*": "./x/*.js", "./p/*": null}"#,
        )
        .unwrap();
        let Json::Obj(map) = map else { panic!() };
        let r = |sub: &str| {
            let (t, s) = match_key(&map, sub)?;
            target_resolve("/n/", &t, s.as_deref(), false).ok().flatten()
        };
        assert_eq!(r(".").as_deref(), Some("/n/i.js"));
        assert_eq!(r("./f/a").as_deref(), Some("/n/lib/a.js"));
        assert_eq!(r("./f/x/y").as_deref(), Some("/n/x/y.js"));
        assert_eq!(r("./p/q"), None);
        assert_eq!(r("./nope"), None);
    }
}
