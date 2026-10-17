use super::*;

/// The site of kind `cat` whose span starts at the first `needle` in `src`: (groups, tags) of the first operand.
fn site(src: &str, needle: &str, cat: &str) -> (usize, Vec<String>) {
    let off = src.find(needle).unwrap_or_else(|| panic!("{needle} not in source")) as u32;
    let r = analyze_source(src).unwrap();
    let s = r.sites.iter().find(|s| s.offset == off && s.cat == cat).unwrap_or_else(|| {
        panic!("no {cat} site at {needle}: {:?}", r.sites.iter().map(|s| (s.offset, s.cat)).collect::<Vec<_>>())
    });
    (s.groups.0, s.tags.0.clone())
}

fn tag_of(src: &str, needle: &str) -> String {
    format!("{}", src.find(needle).unwrap())
}

#[test]
fn constructor_fields_and_methods() {
    let src = "function V(x) { this.x = x; }\n\
               V.prototype.get = function () { return this.x; };\n\
               var v = new V(1);\n\
               var s = v.get() + 1;";
    let f = tag_of(src, "function V");
    assert_eq!(site(src, "this.x;", "get"), (1, vec![format!("inst@proto@{f}")]));
    assert_eq!(site(src, "v.get", "method"), (1, vec![format!("inst@proto@{f}")]));
    assert_eq!(site(src, "v.get() + 1", "arith").1, vec!["num".to_string()]);
}

#[test]
fn replaced_prototype_has_no_default() {
    let src = "function C() { this.a = 1; }\n\
               var o = new C();\n\
               C.prototype = { m: function () { return this.a; } };\n\
               var p = new C(); p.m();";
    let lit = tag_of(src, "{ m:");
    assert_eq!(site(src, "this.a;", "get"), (1, vec![format!("inst@site@{lit}")]));
}

#[test]
fn typeof_narrows() {
    let src = "function f(a) { if (typeof a === \"number\") return a * 2; return a.length; }\n\
               f(1); f(\"x\");";
    assert_eq!(site(src, "a * 2", "arith").1, vec!["num".to_string()]);
    assert_eq!(site(src, "a.length", "get").1, vec!["string".to_string()]);
}

#[test]
fn constructor_returning_object() {
    let src = "function T(l) { return { l: l }; }\n\
               var t = new T(null); var u = new T(t); u.l;";
    let lit = tag_of(src, "{ l:");
    assert_eq!(site(src, "u.l", "get"), (1, vec![format!("site@{lit}")]));
}

#[test]
fn factory_classes_stay_apart() {
    let src = "var Class = { create: function () { return function () { this.init.apply(this, arguments); }; } };\n\
               var A = Class.create(); A.prototype = { init: function (x) { this.x = x; } };\n\
               var B = Class.create(); B.prototype = { init: function (y) { this.y = y; } };\n\
               var a = new A(1); var b = new B(2); a.x; b.y;";
    let la = tag_of(src, "{ init: function (x)");
    let lb = tag_of(src, "{ init: function (y)");
    assert_eq!(site(src, "a.x", "get"), (1, vec![format!("inst@site@{la}")]));
    assert_eq!(site(src, "b.y", "get"), (1, vec![format!("inst@site@{lb}")]));
}

#[test]
fn define_property_on_object_prototype() {
    let src = "Object.defineProperty(Object.prototype, \"tag\", { value: function () { return this.k; } });\n\
               function K() { this.k = 1; }\n\
               var k = new K(); k.tag();";
    let f = tag_of(src, "function K");
    assert_eq!(site(src, "this.k;", "get"), (1, vec![format!("inst@proto@{f}")]));
}

#[test]
fn async_result_through_then() {
    let src = "async function f() { return [1, 2]; }\n\
               f().then(a => a.length);";
    assert_eq!(site(src, "a.length", "get").1, vec!["array".to_string()]);
}

#[test]
fn unknown_is_not_typed() {
    let src = "var x = require(\"zlib\").something(); x.y;";
    assert_eq!(site(src, "x.y", "get").0, 99);
}
