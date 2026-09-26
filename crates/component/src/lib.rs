//! Component: pick/generate the WIT world, embed it into the core module and encode
//! a WASI 0.3 component (wit-component). No JS semantics here: the input is plain export
//! signatures ([`ExportSig`]). Naming rules: `docs/wit-names.md`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, bail};
use wit_component::{ComponentEncoder, StringEncoding, embed_component_metadata};
use wit_parser::Resolve;

pub mod names;

/// A WIT value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    F64,
    String,
}

/// One exported function, as the entry module declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSig {
    /// The JS export name (`default` for `export default`).
    pub name: String,
    /// `name` was a string literal (`export { f as "my-f" }`): used as is, not converted.
    pub string_name: bool,
    /// The local function name (for `help:` suggestions).
    pub local: String,
    pub params: Vec<(String, ValType)>,
    pub result: Option<ValType>,
    /// The function may throw: its WIT result is `result<T, js-error>` ([`JS_ERROR`]).
    pub throws: bool,
    /// 1-based source position of the export, for diagnostics.
    pub line: u32,
    pub col: u32,
}

/// The WIT type a throwing export's error has (declared in the world when used).
pub const JS_ERROR: &str =
    "record js-error { name: string, message: string, stack: option<string> }";

/// A naming error, positioned at the export it's about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameError {
    pub line: u32,
    pub col: u32,
    pub message: String,
    pub help: Option<String>,
}

/// `namespace:package/world`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldName {
    pub namespace: String,
    pub package: String,
    pub world: String,
}

impl WorldName {
    /// The default name: `js2wasi3:<stem>/<stem>`, from the entry file's stem (`.` counts as `_`),
    /// or `js2wasi3:app/app` if the stem can't be mapped.
    pub fn from_path(path: &Path) -> Self {
        let stem = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| {
                s.strip_suffix(".js")
                    .or(s.strip_suffix(".mjs"))
                    .unwrap_or(s)
            })
            .unwrap_or("");
        let name = names::to_wit(&stem.replace('.', "_")).unwrap_or_else(|_| "app".into());
        WorldName {
            namespace: "js2wasi3".into(),
            package: name.clone(),
            world: name,
        }
    }

    /// Parse `--world <namespace>:<package>/<world>`. Each part must be a WIT identifier.
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        let err = || {
            format!(
                "invalid --world {s:?}: expected <namespace>:<package>/<world>, e.g. acme:math/calc"
            )
        };
        let (namespace, rest) = s.split_once(':').with_context(err)?;
        let (package, world) = rest.split_once('/').with_context(err)?;
        for part in [namespace, package, world] {
            if !names::is_wit_name(part) {
                bail!(
                    "{}: {part:?} is not a WIT identifier (kebab-case words, the first starting with a letter)",
                    err()
                );
            }
        }
        Ok(WorldName {
            namespace: namespace.into(),
            package: package.into(),
            world: world.into(),
        })
    }
}

impl std::fmt::Display for WorldName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}/{}", self.namespace, self.package, self.world)
    }
}

/// A generated world: the WIT text and the WIT name of each export (same order as the input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    pub name: WorldName,
    pub export_names: Vec<String>,
    pub wit: String,
}

/// Map the export names to WIT names and generate the world's WIT text.
pub fn world(name: WorldName, exports: &[ExportSig]) -> Result<World, Vec<NameError>> {
    let mut errs = Vec::new();
    let mut export_names = Vec::new();
    // Lowercased WIT name → index of the export that took it.
    let mut taken: HashMap<String, usize> = HashMap::new();
    for (i, e) in exports.iter().enumerate() {
        let error = |message: String| NameError {
            line: e.line,
            col: e.col,
            message,
            help: Some(if e.string_name {
                "WIT names are kebab-case words, e.g. \"my-name\" or \"get-HTTP\"".to_string()
            } else {
                format!(
                    "give it a WIT name with a string export name: `export {{ {} as \"my-name\" }}`",
                    e.local
                )
            }),
        };
        let wit = if e.string_name {
            if names::is_wit_name(&e.name) {
                e.name.clone()
            } else {
                errs.push(error(format!(
                    "export name {:?} is not a valid WIT name (kebab-case words, the first starting with a letter)",
                    e.name
                )));
                continue;
            }
        } else {
            match names::to_wit(&e.name) {
                Ok(w) => w,
                Err(why) => {
                    errs.push(error(format!("export `{}` has no WIT name: {why}", e.name)));
                    continue;
                }
            }
        };
        if wit.eq_ignore_ascii_case("js-error") && exports.iter().any(|e| e.throws) {
            errs.push(error(format!(
                "export `{}` maps to the WIT name `js-error`, which is the error type of throwing exports",
                e.name
            )));
            continue;
        }
        match taken.get(&wit.to_ascii_lowercase()) {
            Some(&j) => {
                let first = &exports[j];
                let mut err = error(format!(
                    "export `{}` and export `{}` (at {}:{}) both map to the WIT name `{wit}`",
                    e.name, first.name, first.line, first.col
                ));
                err.help = Some(format!(
                    "rename one, or give it an explicit WIT name: `export {{ {} as \"other-name\" }}`",
                    e.local
                ));
                errs.push(err);
            }
            None => {
                taken.insert(wit.to_ascii_lowercase(), i);
                export_names.push(wit);
            }
        }
    }
    if !errs.is_empty() {
        return Err(errs);
    }

    let mut wit = format!(
        "package %{}:%{};\n\nworld %{} {{\n",
        name.namespace, name.package, name.world
    );
    if exports.iter().any(|e| e.throws) {
        let _ = writeln!(wit, "  {JS_ERROR}");
    }
    for (e, wit_name) in exports.iter().zip(&export_names) {
        let result = match (e.result, e.throws) {
            (None, false) => String::new(),
            (Some(t), false) => format!(" -> {}", ty(t)),
            (None, true) => " -> result<_, js-error>".into(),
            (Some(t), true) => format!(" -> result<{}, js-error>", ty(t)),
        };
        let _ = writeln!(
            wit,
            "  export %{wit_name}: func({}){result};",
            params(&e.params).join(", "),
        );
    }
    wit.push_str("}\n");
    Ok(World {
        name,
        export_names,
        wit,
    })
}

/// Param names: converted like export names; invalid or duplicate ones become `p<index>`.
fn params(ps: &[(String, ValType)]) -> Vec<String> {
    let mut seen = Vec::new();
    ps.iter()
        .enumerate()
        .map(|(i, (n, t))| {
            let name = match names::to_wit(n) {
                Ok(w) if !seen.contains(&w) => w,
                _ => format!("p{i}"),
            };
            seen.push(name.clone());
            format!("%{name}: {}", ty(*t))
        })
        .collect()
}

fn ty(t: ValType) -> &'static str {
    match t {
        ValType::F64 => "f64",
        ValType::String => "string",
    }
}

/// The vendored WASI 0.3.1 packages, dependencies first (`wit/README.md`).
const WASI: [(&str, &str); 5] = [
    (
        "wasi-clocks-0.3.1.wit",
        include_str!("../wit/wasi-clocks-0.3.1.wit"),
    ),
    (
        "wasi-random-0.3.1.wit",
        include_str!("../wit/wasi-random-0.3.1.wit"),
    ),
    (
        "wasi-filesystem-0.3.1.wit",
        include_str!("../wit/wasi-filesystem-0.3.1.wit"),
    ),
    (
        "wasi-sockets-0.3.1.wit",
        include_str!("../wit/wasi-sockets-0.3.1.wit"),
    ),
    (
        "wasi-cli-0.3.1.wit",
        include_str!("../wit/wasi-cli-0.3.1.wit"),
    ),
];

/// The world of a command: it exports `wasi:cli/run` and imports stdout, stderr and (T19, timers)
/// the monotonic clock (the core module's imports are fixed, see the backend's `host` module).
pub fn command_world(name: WorldName) -> World {
    let wit = format!(
        "package %{}:%{};\n\nworld %{} {{\n  \
         import wasi:cli/stdout@0.3.1;\n  \
         import wasi:cli/stderr@0.3.1;\n  \
         import wasi:clocks/monotonic-clock@0.3.1;\n  \
         export wasi:cli/run@0.3.1;\n}}\n",
        name.namespace, name.package, name.world
    );
    World {
        name,
        export_names: Vec::new(),
        wit,
    }
}

/// Embed `world` into the core module and encode a component. The core module must export
/// each function under its WIT name ([`World::export_names`]).
pub fn package(core: &[u8], world: &World) -> anyhow::Result<Vec<u8>> {
    let mut resolve = Resolve::default();
    // Only a command world uses them; unused packages cost nothing in the component.
    for (path, text) in WASI {
        resolve
            .push_source(path, text)
            .with_context(|| format!("vendored WIT {path}"))?;
    }
    let pkg = resolve
        .push_source("generated.wit", &world.wit)
        .with_context(|| format!("generated WIT is invalid:\n{}", world.wit))?;
    // `%` escapes names that are WIT keywords (e.g. an entry file `stream.js`).
    let world_id = resolve.select_world(&[pkg], Some(&format!("%{}", world.name.world)))?;
    let mut bytes = core.to_vec();
    embed_component_metadata(&mut bytes, &resolve, world_id, StringEncoding::UTF8)?;
    ComponentEncoder::default()
        .module(&bytes)?
        .validate(true)
        .encode()
        .context("encoding the component")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(name: &str, string_name: bool, params: &[&str]) -> ExportSig {
        ExportSig {
            name: name.into(),
            string_name,
            local: name.into(),
            params: params
                .iter()
                .map(|p| (p.to_string(), ValType::F64))
                .collect(),
            result: Some(ValType::F64),
            throws: false,
            line: 1,
            col: 1,
        }
    }

    #[test]
    fn world_names() {
        let n = |p: &str| WorldName::from_path(Path::new(p)).to_string();
        assert_eq!(n("tests/add.js"), "js2wasi3:add/add");
        assert_eq!(n("sumOdd.js"), "js2wasi3:sum-odd/sum-odd");
        assert_eq!(n("my_lib.test.js"), "js2wasi3:my-lib-test/my-lib-test");
        assert_eq!(n("_.js"), "js2wasi3:app/app");
        assert_eq!(n("1.mjs"), "js2wasi3:app/app");
        assert_eq!(
            WorldName::parse("acme:math/calc").unwrap().to_string(),
            "acme:math/calc"
        );
        for bad in ["acme", "acme:math", "acme:Math-x/calc", ":a/b", "a:b/c_d"] {
            assert!(WorldName::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn generates_wit() {
        let w = world(
            WorldName::from_path(Path::new("m.js")),
            &[
                // Both params map to `a`, so the second one becomes `p1`.
                sig("sumOdd", false, &["a", "_a"]),
                sig("my-x", true, &[]),
                sig("type", false, &["_", "type", "a"]),
            ],
        )
        .unwrap();
        assert_eq!(w.export_names, ["sum-odd", "my-x", "type"]);
        assert_eq!(
            w.wit,
            "package %js2wasi3:%m;\n\nworld %m {\n\
             \x20 export %sum-odd: func(%a: f64, %p1: f64) -> f64;\n\
             \x20 export %my-x: func() -> f64;\n\
             \x20 export %type: func(%p0: f64, %type: f64, %a: f64) -> f64;\n}\n"
        );
    }

    #[test]
    fn throwing_exports_return_results() {
        let mut f = sig("f", false, &["x"]);
        f.throws = true;
        let mut g = sig("g", false, &[]);
        (g.throws, g.result) = (true, None);
        let mut h = sig("h", false, &[]);
        (h.throws, h.result) = (true, Some(ValType::String));
        let w = world(WorldName::from_path(Path::new("m.js")), &[f, g, h]).unwrap();
        assert_eq!(
            w.wit,
            format!(
                "package %js2wasi3:%m;\n\nworld %m {{\n  {JS_ERROR}\n\
                 \x20 export %f: func(%x: f64) -> result<f64, js-error>;\n\
                 \x20 export %g: func() -> result<_, js-error>;\n\
                 \x20 export %h: func() -> result<string, js-error>;\n}}\n"
            )
        );
        // The name is taken only when an export throws.
        let mut e = sig("jsError", false, &[]);
        assert!(world(WorldName::from_path(Path::new("m.js")), &[e.clone()]).is_ok());
        e.throws = true;
        let errs = world(WorldName::from_path(Path::new("m.js")), &[e]).unwrap_err();
        assert!(
            errs[0].message.contains("`js-error`"),
            "{}",
            errs[0].message
        );
    }

    #[test]
    fn name_errors() {
        let errs = world(
            WorldName::from_path(Path::new("m.js")),
            &[
                sig("_1", false, &[]),
                sig("fooBar", false, &[]),
                sig("foo_bar", false, &[]),
                sig("FOO-BAR", true, &[]),
                sig("not valid", true, &[]),
            ],
        )
        .unwrap_err();
        let msgs: Vec<&str> = errs.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(msgs.len(), 4, "{msgs:#?}");
        assert!(msgs[0].contains("`_1` has no WIT name"), "{}", msgs[0]);
        assert!(
            msgs[1].contains("`foo_bar` and export `fooBar`"),
            "{}",
            msgs[1]
        );
        assert!(
            msgs[2].contains("`FOO-BAR` and export `fooBar`"),
            "{}",
            msgs[2]
        );
        assert!(
            msgs[3].contains("\"not valid\" is not a valid WIT name"),
            "{}",
            msgs[3]
        );
    }
}
