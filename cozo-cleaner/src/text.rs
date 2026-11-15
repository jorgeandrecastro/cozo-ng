use std::sync::OnceLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::step::{UnicodeForm, Whitespace};

/// Options of [`normalize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizeOptions {
    pub unicode: UnicodeForm,
    pub whitespace: Whitespace,
    /// Remove control characters (except newlines and tabs).
    pub strip_control: bool,
    /// Remove zero-width spaces, word joiners, BOMs and soft hyphens.
    pub strip_zero_width: bool,
    /// Remove HTML tags and decode the common entities.
    pub strip_html: bool,
    pub lowercase: bool,
}

impl Default for NormalizeOptions {
    fn default() -> Self {
        NormalizeOptions {
            unicode: UnicodeForm::Nfc,
            whitespace: Whitespace::Collapse,
            strip_control: true,
            strip_zero_width: true,
            strip_html: false,
            lowercase: false,
        }
    }
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?s)<script.*?</script>|<style.*?</style>|<!--.*?-->|</?([a-zA-Z][a-zA-Z0-9]*)[^>]*>").unwrap()
    })
}

/// Inline tags disappear without a trace (`<b>word</b>.` → `word.`); every other tag is a break.
const INLINE_TAGS: [&str; 14] =
    ["a", "b", "i", "u", "em", "strong", "span", "code", "small", "sub", "sup", "mark", "abbr", "font"];

fn strip_tags(s: &str) -> String {
    tag_re()
        .replace_all(s, |c: &regex::Captures| match c.get(1) {
            Some(name) if INLINE_TAGS.contains(&name.as_str().to_ascii_lowercase().as_str()) => "",
            _ => "\n",
        })
        .into_owned()
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Cleans one string according to `opts`.
pub fn normalize(input: &str, opts: &NormalizeOptions) -> String {
    let mut s = input.replace("\r\n", "\n").replace('\r', "\n");
    if opts.strip_html {
        s = decode_entities(&strip_tags(&s));
    }
    s = match opts.unicode {
        UnicodeForm::Nfc => s.nfc().collect(),
        UnicodeForm::Nfkc => s.nfkc().collect(),
        UnicodeForm::None => s,
    };
    if opts.strip_zero_width {
        s.retain(|c| !matches!(c, '\u{200b}' | '\u{2060}' | '\u{feff}' | '\u{00ad}'));
    }
    if opts.strip_control {
        s.retain(|c| !c.is_control() || matches!(c, '\n' | '\t'));
    }
    s = match opts.whitespace {
        Whitespace::None => s,
        Whitespace::Collapse => s.split_whitespace().collect::<Vec<_>>().join(" "),
        Whitespace::KeepLines => {
            let lines: Vec<String> =
                s.split('\n').map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
            let mut out = String::new();
            let mut blank = 0;
            for l in lines {
                if l.is_empty() {
                    blank += 1;
                    continue;
                }
                if !out.is_empty() {
                    out.push_str(if blank > 0 { "\n\n" } else { "\n" });
                }
                blank = 0;
                out.push_str(&l);
            }
            out
        }
    };
    if opts.lowercase {
        s = s.to_lowercase();
    }
    s
}

/// `"Unit Price (EUR)"` → `"unit_price_eur"`, `"firstName"` → `"first_name"`.
pub fn snake_case(s: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in s.trim().chars() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && prev_lower {
                out.push('_');
            }
            out.extend(c.to_lowercase());
            prev_lower = c.is_lowercase() || c.is_numeric();
        } else {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            prev_lower = false;
        }
    }
    out.trim_matches('_').to_string()
}

/// FNV-1a 64-bit hash as 16 hex digits. Unlike `std`'s hashers it is specified and stable
/// across Rust versions and platforms, so generated ids never change.
pub fn fnv1a_hex(parts: &[&str]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            h ^= 0x1f;
            h = h.wrapping_mul(0x100000001b3);
        }
        for b in p.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_whitespace_and_unicode() {
        let o = NormalizeOptions::default();
        assert_eq!(normalize("  a \t b\n c\u{200b} ", &o), "a b c");
        // "e" + combining acute becomes a single "é"
        assert_eq!(normalize("e\u{301}", &o), "\u{e9}");
        assert_eq!(normalize("\u{fb01}", &NormalizeOptions { unicode: UnicodeForm::Nfkc, ..o.clone() }), "fi");
    }

    #[test]
    fn keeps_paragraphs() {
        let o = NormalizeOptions { whitespace: Whitespace::KeepLines, ..Default::default() };
        assert_eq!(normalize("a  b\n\n\n\nc \r\n d", &o), "a b\n\nc\nd");
    }

    #[test]
    fn strips_html() {
        let o = NormalizeOptions { strip_html: true, ..Default::default() };
        assert_eq!(normalize("<p>Tom &amp; <b>Jerry</b></p><script>x()</script>", &o), "Tom & Jerry");
        assert_eq!(normalize("Graphs model <b>relationships</b>.", &o), "Graphs model relationships.");
        assert_eq!(normalize("<p>a</p><p>b</p>", &o), "a b");
        let keep = NormalizeOptions { whitespace: Whitespace::KeepLines, strip_html: true, ..Default::default() };
        // block tags become line breaks, so paragraphs survive for chunking
        assert_eq!(normalize("<p>a</p><p>b<br>c</p>", &keep), "a\n\nb\nc");
    }

    #[test]
    fn snake_cases() {
        assert_eq!(snake_case("Unit Price (EUR)"), "unit_price_eur");
        assert_eq!(snake_case("firstName"), "first_name");
        assert_eq!(snake_case("  __ID__ "), "id");
    }

    #[test]
    fn fnv_is_stable() {
        assert_eq!(fnv1a_hex(&["a"]), "af63dc4c8601ec8c");
        assert_ne!(fnv1a_hex(&["a", "b"]), fnv1a_hex(&["ab"]));
    }
}
