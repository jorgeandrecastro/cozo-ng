use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use cozo::DataValue;
use miette::{miette, Result};
use serde_json::Value as JsonValue;

/// Relation / column / index names are interpolated into scripts: only allow safe identifiers.
/// `rel:index` is accepted when `allow_colon` is set.
pub fn ident(s: &str, allow_colon: bool) -> Result<&str> {
    let ok = !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_' || (allow_colon && c == ':'));
    if ok { Ok(s) } else { Err(miette!("invalid identifier: {s:?}")) }
}

/// A CLI value: JSON if it parses as JSON, otherwise a plain string.
pub fn parse_value(s: &str) -> DataValue {
    match serde_json::from_str::<JsonValue>(s) {
        Ok(j) => DataValue::from(j),
        Err(_) => DataValue::Str(s.into()),
    }
}

pub fn parse_params(items: &[String]) -> Result<BTreeMap<String, DataValue>> {
    let mut out = BTreeMap::new();
    for item in items {
        let (k, v) = item
            .split_once('=')
            .ok_or_else(|| miette!("parameter {item:?} must look like NAME=VALUE"))?;
        out.insert(k.trim().to_string(), parse_value(v));
    }
    Ok(out)
}

/// Reads a file, or stdin when `arg` is "-".
pub fn read_input(arg: &str) -> Result<String> {
    if arg == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map_err(|e| miette!("cannot read stdin: {e}"))?;
        Ok(s)
    } else {
        std::fs::read_to_string(arg).map_err(|e| miette!("cannot read {arg}: {e}"))
    }
}

pub fn to_json(v: &DataValue) -> JsonValue {
    JsonValue::from(v.clone())
}

/// Plain-text rendering of one cell.
pub fn cell(v: &DataValue) -> String {
    match v {
        DataValue::Str(s) => s.to_string(),
        DataValue::Null => "null".to_string(),
        other => to_json(other).to_string(),
    }
}

pub fn dir_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::metadata(path) else { return 0 };
    if meta.is_file() {
        return meta.len();
    }
    let Ok(rd) = std::fs::read_dir(path) else { return 0 };
    rd.flatten().map(|e| dir_size(&e.path())).sum()
}

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{v:.1} {}", UNITS[i]) }
}
