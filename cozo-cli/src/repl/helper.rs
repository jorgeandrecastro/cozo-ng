use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rustyline::completion::{Completer, Pair};
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Context, Helper};

use super::validate::is_complete;

/// Relations and their columns, refreshed after schema changes.
#[derive(Default)]
pub struct Cache {
    pub relations: Vec<String>,
    pub columns: HashMap<String, Vec<String>>,
}

pub struct CozoHelper {
    cache: Rc<RefCell<Cache>>,
    hinter: HistoryHinter,
}

impl CozoHelper {
    pub fn new(cache: Rc<RefCell<Cache>>) -> Self {
        Self { cache, hinter: HistoryHinter::new() }
    }
}

const DOT: &[&str] = &[".help", ".relations", ".schema", ".timer", ".format", ".read", ".output", ".quit"];
const SYSOPS: &[&str] = &[
    "::relations", "::columns", "::indices", "::describe", "::remove", "::rename", "::explain", "::running",
    "::kill", "::compact", "::access_level", "::hnsw create", "::hnsw drop", "::fts create", "::fts drop",
    "::lsh create", "::lsh drop", "::index create", "::index drop", "::show_triggers", "::set_triggers",
];
const OPTIONS: &[&str] = &[
    ":create", ":replace", ":put", ":insert", ":update", ":rm", ":delete", ":ensure", ":ensure_not",
    ":limit", ":offset", ":order", ":timeout", ":returning", ":sleep", ":assert",
];
const KEYWORDS: &[&str] = &[
    "count", "count_unique", "sum", "mean", "min", "max", "collect", "choice", "shortest", "union", "intersection",
    "vec", "l2_dist", "cos_dist", "ip_dist", "rand_vec", "length", "append", "concat", "to_string", "to_int",
    "to_float", "starts_with", "ends_with", "lowercase", "uppercase", "trim", "regex_matches", "now", "uuid",
    "PageRank", "ShortestPathDijkstra", "ConnectedComponents", "StronglyConnectedComponents",
    "CommunityDetectionLouvain", "BetweennessCentrality", "ClosenessCentrality", "DegreeCentrality",
    "BreadthFirstSearch", "DepthFirstSearch", "TopSort", "MinimumSpanningForestKruskal", "ReorderSort",
    "CsvReader", "JsonReader", "Constant", "default", "null", "true", "false", "not", "or", "and", "is_null",
];

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | ':' | '.')
}

fn pairs<'a>(items: impl Iterator<Item = &'a str>, prefix: &str) -> Vec<Pair> {
    let lower = prefix.to_lowercase();
    items
        .filter(|i| i.to_lowercase().starts_with(&lower))
        .map(|i| Pair { display: i.to_string(), replacement: i.to_string() })
        .collect()
}

impl Completer for CozoHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before = &line[..pos];
        let start = before.rfind(|c: char| !is_word(c)).map(|i| i + before[i..].chars().next().map_or(1, |c| c.len_utf8())).unwrap_or(0);
        let word = &before[start..];
        let cache = self.cache.borrow();

        // `.commands` at the start of the line
        if before.trim_start().starts_with('.') && !before.contains(' ') {
            return Ok((start, pairs(DOT.iter().copied(), word)));
        }
        // `.schema rel`
        if before.trim_start().starts_with(".schema ") {
            return Ok((start, pairs(cache.relations.iter().map(|s| s.as_str()), word)));
        }
        // `*rel` → relation names
        if before[..start].ends_with('*') {
            return Ok((start, pairs(cache.relations.iter().map(|s| s.as_str()), word)));
        }
        // `*rel{ a, b` → column names of rel
        if let Some(open) = before.rfind('{') {
            if !before[open..].contains('}') {
                if let Some(star) = before[..open].rfind('*') {
                    let rel = before[star + 1..open].trim();
                    if let Some(cols) = cache.columns.get(rel) {
                        return Ok((start, pairs(cols.iter().map(|s| s.as_str()), word)));
                    }
                }
            }
        }
        if word.starts_with("::") {
            return Ok((start, pairs(SYSOPS.iter().copied(), word)));
        }
        if word.starts_with(':') {
            return Ok((start, pairs(OPTIONS.iter().copied(), word)));
        }
        if word.is_empty() {
            return Ok((start, vec![]));
        }
        Ok((start, pairs(KEYWORDS.iter().copied(), word)))
    }
}

impl Hinter for CozoHelper {
    type Hint = String;
    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Validator for CozoHelper {
    fn validate(&self, ctx: &mut ValidationContext<'_>) -> rustyline::Result<ValidationResult> {
        Ok(if is_complete(ctx.input()) { ValidationResult::Valid(None) } else { ValidationResult::Incomplete })
    }
}

const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const MAGENTA: &str = "\x1b[35m";
const DIM: &str = "\x1b[90m";
const BOLD: &str = "\x1b[1m";

impl Highlighter for CozoHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len() + 16);
        let mut i = 0;
        let paint = |out: &mut String, color: &str, s: &str| {
            out.push_str(color);
            out.push_str(s);
            out.push_str(RESET);
        };
        while i < chars.len() {
            let c = chars[i];
            let rest: String = chars[i..].iter().collect();
            if c == '\'' || c == '"' {
                let mut j = i + 1;
                while j < chars.len() && chars[j] != c {
                    if chars[j] == '\\' {
                        j += 1;
                    }
                    j += 1;
                }
                let end = (j + 1).min(chars.len());
                paint(&mut out, GREEN, &chars[i..end].iter().collect::<String>());
                i = end;
            } else if c == '#' {
                paint(&mut out, DIM, &rest);
                break;
            } else if c == '*' && chars.get(i + 1).is_some_and(|n| n.is_alphabetic() || *n == '_') {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                    j += 1;
                }
                paint(&mut out, YELLOW, &chars[i..j].iter().collect::<String>());
                i = j;
            } else if c == ':' && chars.get(i + 1).is_some_and(|n| n.is_alphabetic() || *n == ':') && (i == 0 || chars[i - 1].is_whitespace()) {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == ':') {
                    j += 1;
                }
                paint(&mut out, MAGENTA, &chars[i..j].iter().collect::<String>());
                i = j;
            } else if c.is_ascii_digit() && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')) {
                let mut j = i;
                while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                    j += 1;
                }
                paint(&mut out, CYAN, &chars[i..j].iter().collect::<String>());
                i = j;
            } else if c == '?' {
                paint(&mut out, BOLD, "?");
                i += 1;
            } else if c == '.' && i == 0 {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_alphabetic() {
                    j += 1;
                }
                paint(&mut out, MAGENTA, &chars[i..j].iter().collect::<String>());
                i = j;
            } else {
                out.push(c);
                i += 1;
            }
        }
        Cow::Owned(out)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(format!("{DIM}{hint}{RESET}"))
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        true
    }
}

impl Helper for CozoHelper {}
