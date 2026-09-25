//! JS names → WIT names. The rules and their reasons are in `docs/wit-names.md`.

/// Convert a JS identifier to a WIT name: split at `_`, `$` and case boundaries,
/// lowercase every word, join with `-`. `Err` says why the name can't be mapped.
pub fn to_wit(js: &str) -> Result<String, &'static str> {
    if !js.is_ascii() {
        return Err("it contains non-ASCII characters");
    }
    let chars: Vec<char> = js.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' || c == '$' {
            words.push(std::mem::take(&mut cur));
            continue;
        }
        if c.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next = chars.get(i + 1).copied();
            // `sumOdd`, `x2Y` → boundary after a lowercase letter or digit;
            // `HTTPServer` → boundary before the last capital of a run followed by lowercase.
            if prev.is_ascii_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase()))
            {
                words.push(std::mem::take(&mut cur));
            }
        }
        cur.push(c.to_ascii_lowercase());
    }
    words.push(cur);
    words.retain(|w| !w.is_empty());
    match words.first() {
        None => Err("it has no letters or digits besides `_` and `$`"),
        Some(w) if !w.starts_with(|c: char| c.is_ascii_alphabetic()) => {
            Err("it doesn't start with a letter once `_` and `$` are dropped")
        }
        Some(_) => Ok(words.join("-")),
    }
}

/// A valid WIT identifier (without the `%` escape): words joined by single `-`, each word all
/// lowercase or all uppercase ASCII letters/digits, the first word starting with a letter.
pub fn is_wit_name(s: &str) -> bool {
    s.split('-').enumerate().all(|(i, w)| {
        !w.is_empty()
            && w.chars().all(|c| c.is_ascii_alphanumeric())
            && (w.chars().all(|c| !c.is_ascii_uppercase())
                || w.chars().all(|c| !c.is_ascii_lowercase()))
            && (i > 0 || w.starts_with(|c: char| c.is_ascii_alphabetic()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts() {
        for (js, wit) in [
            ("add", "add"),
            ("sumOdd", "sum-odd"),
            ("getHTTPServer", "get-http-server"),
            ("HTTPServer", "http-server"),
            ("x2Y", "x2-y"),
            ("utf8Decode", "utf8-decode"),
            ("HTML5Parser", "html5-parser"),
            ("_private", "private"),
            ("foo_bar", "foo-bar"),
            ("foo$bar", "foo-bar"),
            ("__a__b__", "a-b"),
            ("x_2", "x-2"),
            ("type", "type"),
            ("default", "default"),
            ("ABC", "abc"),
        ] {
            assert_eq!(to_wit(js).as_deref(), Ok(wit), "{js}");
            assert!(is_wit_name(wit), "{wit}");
        }
    }

    #[test]
    fn rejects() {
        for js in ["_", "$", "__", "_1", "$0", "_2fast", "café", "π"] {
            assert!(to_wit(js).is_err(), "{js}");
        }
    }

    #[test]
    fn validates() {
        for ok in [
            "x-2y", "x2y", "HTTP-get", "get-HTTP", "x-Y", "utf8-x", "default", "type",
        ] {
            assert!(is_wit_name(ok), "{ok}");
        }
        for bad in ["getHttp", "a--b", "x-", "-x", "2x", "", "a_b", "%type", "é"] {
            assert!(!is_wit_name(bad), "{bad}");
        }
    }
}
