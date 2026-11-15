use std::collections::{BTreeMap, HashSet};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use crate::chunk::chunk_text;
use crate::error::{CleanError, Result};
use crate::report::{Reject, StepReport};
use crate::text::{fnv1a_hex, normalize, snake_case, NormalizeOptions};

/// Strings treated as "no value" by the `nulls` step (compared after trimming, ignoring case).
pub const DEFAULT_NULLS: [&str; 8] = ["", "null", "none", "na", "n/a", "nan", "nil", "-"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnicodeForm {
    /// Canonical composition (default): `e` + combining accent → `é`.
    #[default]
    Nfc,
    /// Compatibility composition: also folds ligatures and full-width forms (`ﬁ` → `fi`).
    Nfkc,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Whitespace {
    /// Every run of whitespace becomes one space; the text is trimmed (default).
    #[default]
    Collapse,
    /// Collapse spaces and tabs inside lines but keep line and paragraph breaks (use before `chunk`).
    KeepLines,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CastType {
    String,
    Int,
    Float,
    Bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    /// Replace the value with null (default).
    #[default]
    Null,
    /// Reject the whole row.
    Reject,
    /// Leave the value as it is.
    Keep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Keep {
    #[default]
    First,
    Last,
}

/// One validation rule of the `validate` step. Null cells are skipped (use `require` for those).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub column: String,
    /// Inclusive lower bound (numbers).
    #[serde(default)]
    pub min: Option<f64>,
    /// Inclusive upper bound (numbers).
    #[serde(default)]
    pub max: Option<f64>,
    /// Minimum number of characters (strings).
    #[serde(default)]
    pub min_len: Option<usize>,
    #[serde(default)]
    pub max_len: Option<usize>,
    /// Regular expression the whole string must match somewhere (use `^…$` to anchor).
    #[serde(default)]
    pub pattern: Option<String>,
    /// Allowed values.
    #[serde(default)]
    pub one_of: Option<Vec<Value>>,
}

fn yes() -> bool {
    true
}
fn default_id() -> String {
    "id".into()
}
fn default_size() -> usize {
    1000
}
fn default_overlap() -> usize {
    100
}
fn default_index() -> String {
    "chunk_index".into()
}

/// A cleaning step. In TOML: `[[steps]]` with `op = "<snake_case name>"` and the fields below.
/// Steps taking `columns` apply to every column when it is empty or missing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Remove leading and trailing whitespace of string cells.
    Trim {
        #[serde(default)]
        columns: Vec<String>,
    },
    /// Unicode normalization, whitespace cleanup, control / zero-width / HTML stripping, lowercasing.
    NormalizeText {
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        unicode: UnicodeForm,
        #[serde(default)]
        whitespace: Whitespace,
        #[serde(default = "yes")]
        strip_control: bool,
        #[serde(default = "yes")]
        strip_zero_width: bool,
        #[serde(default)]
        strip_html: bool,
        #[serde(default)]
        lowercase: bool,
    },
    /// Turn placeholder strings (`""`, `NA`, `null`, `-`…) into real nulls.
    Nulls {
        #[serde(default)]
        columns: Vec<String>,
        /// Placeholders; defaults to [`DEFAULT_NULLS`].
        #[serde(default)]
        values: Option<Vec<String>>,
    },
    /// Remove rows whose cells are all null or blank.
    DropEmptyRows,
    /// Rename columns: `map = { old = "new" }`.
    Rename { map: BTreeMap<String, String> },
    /// Keep only these columns, in this order.
    Select { columns: Vec<String> },
    /// Remove these columns.
    Drop { columns: Vec<String> },
    /// Rewrite every column name as `snake_case`.
    SnakeCaseHeaders,
    /// Convert column types: `types = { age = "int", price = "float" }`.
    Cast {
        types: BTreeMap<String, CastType>,
        #[serde(default)]
        on_error: OnError,
        /// Read `1.234,5` as 1234.5 instead of `1,234.5`.
        #[serde(default)]
        decimal_comma: bool,
    },
    /// Replace nulls: `values = { country = "unknown", score = 0 }`.
    Fill { values: BTreeMap<String, Value> },
    /// Reject rows that have a null (or blank string) in any of these columns.
    Require { columns: Vec<String> },
    /// Reject rows that break a rule.
    Validate { rules: Vec<Rule> },
    /// Remove duplicate rows, comparing `by` (all columns when empty).
    Dedup {
        #[serde(default)]
        by: Vec<String>,
        #[serde(default)]
        keep: Keep,
    },
    /// Add a deterministic id column: FNV-1a 64 of the `from` columns (all when empty).
    AddId {
        #[serde(default = "default_id")]
        column: String,
        #[serde(default)]
        from: Vec<String>,
        #[serde(default)]
        prefix: Option<String>,
    },
    /// Split a long text column into overlapping chunks (one output row per chunk).
    Chunk {
        column: String,
        /// Maximum characters per chunk.
        #[serde(default = "default_size")]
        size: usize,
        /// Characters repeated at the start of the next chunk.
        #[serde(default = "default_overlap")]
        overlap: usize,
        /// Name of the added 0-based chunk counter column; empty string for none.
        #[serde(default = "default_index")]
        index_column: String,
    },
}

impl Step {
    pub fn name(&self) -> &'static str {
        match self {
            Step::Trim { .. } => "trim",
            Step::NormalizeText { .. } => "normalize_text",
            Step::Nulls { .. } => "nulls",
            Step::DropEmptyRows => "drop_empty_rows",
            Step::Rename { .. } => "rename",
            Step::Select { .. } => "select",
            Step::Drop { .. } => "drop",
            Step::SnakeCaseHeaders => "snake_case_headers",
            Step::Cast { .. } => "cast",
            Step::Fill { .. } => "fill",
            Step::Require { .. } => "require",
            Step::Validate { .. } => "validate",
            Step::Dedup { .. } => "dedup",
            Step::AddId { .. } => "add_id",
            Step::Chunk { .. } => "chunk",
        }
    }

    /// `normalize_text` with the defaults.
    pub fn normalize_text() -> Step {
        Step::NormalizeText {
            columns: vec![],
            unicode: UnicodeForm::Nfc,
            whitespace: Whitespace::Collapse,
            strip_control: true,
            strip_zero_width: true,
            strip_html: false,
            lowercase: false,
        }
    }

    /// Checks parameters that can be checked without data.
    pub fn check(&self) -> Result<()> {
        match self {
            Step::Chunk { size, overlap, .. } => {
                if *size == 0 || overlap >= size {
                    return Err(CleanError::Config(format!(
                        "chunk: need size > 0 and overlap < size (got size = {size}, overlap = {overlap})"
                    )));
                }
            }
            Step::Validate { rules } => {
                for r in rules {
                    if let Some(p) = &r.pattern {
                        Regex::new(p).map_err(|e| CleanError::Config(format!("validate: bad pattern {p:?}: {e}")))?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Working copy of the table while the pipeline runs.
pub(crate) struct State {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    /// 1-based input row number of every current row.
    pub origin: Vec<usize>,
    pub rejects: Vec<Reject>,
}

fn is_blank(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        _ => false,
    }
}

impl State {
    fn cols(&self, step: &str, names: &[String]) -> Result<Vec<usize>> {
        if names.is_empty() {
            return Ok((0..self.columns.len()).collect());
        }
        names.iter().map(|n| self.col(step, n)).collect()
    }

    fn col(&self, step: &str, name: &str) -> Result<usize> {
        self.columns
            .iter()
            .position(|c| c == name)
            .ok_or_else(|| CleanError::UnknownColumn { step: step.into(), column: name.into() })
    }

    fn record(&self, row: &[Value]) -> Map<String, Value> {
        self.columns.iter().cloned().zip(row.iter().cloned()).collect()
    }

    /// Moves the rows with a reason into the rejects. Returns how many were rejected.
    fn partition(&mut self, step: &str, reasons: Vec<Option<String>>) -> usize {
        debug_assert_eq!(reasons.len(), self.rows.len());
        let rows = std::mem::take(&mut self.rows);
        let origin = std::mem::take(&mut self.origin);
        let mut rejected = 0;
        for ((row, org), reason) in rows.into_iter().zip(origin).zip(reasons) {
            match reason {
                None => {
                    self.rows.push(row);
                    self.origin.push(org);
                }
                Some(reason) => {
                    rejected += 1;
                    let record = self.record(&row);
                    self.rejects.push(Reject { row: org, step: step.into(), reason, record });
                }
            }
        }
        rejected
    }

    fn keep_where(&mut self, keep: Vec<bool>) -> usize {
        let before = self.rows.len();
        let mut it = keep.iter();
        self.rows.retain(|_| *it.next().unwrap());
        let mut it = keep.iter();
        self.origin.retain(|_| *it.next().unwrap());
        before - self.rows.len()
    }

    pub fn apply(&mut self, step: &Step) -> Result<StepReport> {
        step.check()?;
        let name = step.name();
        let mut rep = StepReport { step: name.into(), ..Default::default() };
        match step {
            Step::Trim { columns } => {
                for c in self.cols(name, columns)? {
                    for row in &mut self.rows {
                        if let Value::String(s) = &row[c] {
                            let t = s.trim();
                            if t.len() != s.len() {
                                row[c] = Value::String(t.to_string());
                                rep.cells_changed += 1;
                            }
                        }
                    }
                }
            }
            Step::NormalizeText { columns, unicode, whitespace, strip_control, strip_zero_width, strip_html, lowercase } => {
                let opts = NormalizeOptions {
                    unicode: *unicode,
                    whitespace: *whitespace,
                    strip_control: *strip_control,
                    strip_zero_width: *strip_zero_width,
                    strip_html: *strip_html,
                    lowercase: *lowercase,
                };
                for c in self.cols(name, columns)? {
                    for row in &mut self.rows {
                        if let Value::String(s) = &row[c] {
                            let n = normalize(s, &opts);
                            if &n != s {
                                row[c] = Value::String(n);
                                rep.cells_changed += 1;
                            }
                        }
                    }
                }
            }
            Step::Nulls { columns, values } => {
                let set: HashSet<String> = match values {
                    Some(v) => v.iter().map(|s| s.trim().to_lowercase()).collect(),
                    None => DEFAULT_NULLS.iter().map(|s| s.to_string()).collect(),
                };
                for c in self.cols(name, columns)? {
                    for row in &mut self.rows {
                        if let Value::String(s) = &row[c] {
                            if set.contains(&s.trim().to_lowercase()) {
                                row[c] = Value::Null;
                                rep.cells_changed += 1;
                            }
                        }
                    }
                }
            }
            Step::DropEmptyRows => {
                let keep = self.rows.iter().map(|r| !r.iter().all(is_blank)).collect();
                rep.rows_removed = self.keep_where(keep);
            }
            Step::Rename { map } => {
                for (old, new) in map {
                    let i = self.col(name, old)?;
                    if self.columns.iter().enumerate().any(|(j, c)| j != i && c == new) {
                        return Err(CleanError::Config(format!("rename: `{new}` already exists")));
                    }
                    self.columns[i] = new.clone();
                }
            }
            Step::Select { columns } => {
                let idx = columns.iter().map(|c| self.col(name, c)).collect::<Result<Vec<_>>>()?;
                self.rows = self.rows.iter().map(|r| idx.iter().map(|&i| r[i].clone()).collect()).collect();
                self.columns = columns.clone();
            }
            Step::Drop { columns } => {
                let idx = columns.iter().map(|c| self.col(name, c)).collect::<Result<HashSet<_>>>()?;
                let keep: Vec<usize> = (0..self.columns.len()).filter(|i| !idx.contains(i)).collect();
                self.rows = self.rows.iter().map(|r| keep.iter().map(|&i| r[i].clone()).collect()).collect();
                self.columns = keep.iter().map(|&i| self.columns[i].clone()).collect();
            }
            Step::SnakeCaseHeaders => {
                let new: Vec<String> = self.columns.iter().map(|c| snake_case(c)).collect();
                let mut seen = HashSet::new();
                for (old, n) in self.columns.iter().zip(&new) {
                    if n.is_empty() {
                        return Err(CleanError::Config(format!("snake_case_headers: `{old}` has no usable characters")));
                    }
                    if !seen.insert(n.clone()) {
                        return Err(CleanError::Config(format!("snake_case_headers: two columns become `{n}`")));
                    }
                }
                rep.cells_changed = self.columns.iter().zip(&new).filter(|(a, b)| a != b).count();
                self.columns = new;
            }
            Step::Cast { types, on_error, decimal_comma } => {
                let plan: Vec<(usize, CastType)> =
                    types.iter().map(|(c, t)| Ok((self.col(name, c)?, *t))).collect::<Result<_>>()?;
                let mut reasons: Vec<Option<String>> = vec![None; self.rows.len()];
                for (r, row) in self.rows.iter_mut().enumerate() {
                    for &(c, t) in &plan {
                        match cast_value(&row[c], t, *decimal_comma) {
                            Ok(v) => {
                                if v != row[c] {
                                    row[c] = v;
                                    rep.cells_changed += 1;
                                }
                            }
                            Err(msg) => match on_error {
                                OnError::Null => {
                                    row[c] = Value::Null;
                                    rep.cells_changed += 1;
                                }
                                OnError::Keep => {}
                                OnError::Reject => {
                                    reasons[r] = Some(format!("column `{}`: {msg}", self.columns[c]));
                                    break;
                                }
                            },
                        }
                    }
                }
                rep.rows_rejected = self.partition(name, reasons);
            }
            Step::Fill { values } => {
                for (col, val) in values {
                    let c = self.col(name, col)?;
                    for row in &mut self.rows {
                        if row[c].is_null() {
                            row[c] = val.clone();
                            rep.cells_changed += 1;
                        }
                    }
                }
            }
            Step::Require { columns } => {
                let idx = columns.iter().map(|c| self.col(name, c)).collect::<Result<Vec<_>>>()?;
                let reasons = self
                    .rows
                    .iter()
                    .map(|row| {
                        idx.iter().find(|&&i| is_blank(&row[i])).map(|&i| format!("column `{}` is empty", self.columns[i]))
                    })
                    .collect();
                rep.rows_rejected = self.partition(name, reasons);
            }
            Step::Validate { rules } => {
                let compiled: Vec<(usize, &Rule, Option<Regex>)> = rules
                    .iter()
                    .map(|r| {
                        let re = r.pattern.as_deref().map(Regex::new).transpose().expect("checked");
                        Ok((self.col(name, &r.column)?, r, re))
                    })
                    .collect::<Result<_>>()?;
                let reasons = self
                    .rows
                    .iter()
                    .map(|row| compiled.iter().find_map(|(i, rule, re)| check_rule(&row[*i], rule, re.as_ref())))
                    .collect();
                rep.rows_rejected = self.partition(name, reasons);
            }
            Step::Dedup { by, keep } => {
                let idx = self.cols(name, by)?;
                let key = |row: &Vec<Value>| -> String {
                    serde_json::to_string(&idx.iter().map(|&i| &row[i]).collect::<Vec<_>>()).unwrap_or_default()
                };
                let mut seen = HashSet::new();
                let mut flags = vec![true; self.rows.len()];
                let order: Vec<usize> = match keep {
                    Keep::First => (0..self.rows.len()).collect(),
                    Keep::Last => (0..self.rows.len()).rev().collect(),
                };
                for i in order {
                    flags[i] = seen.insert(key(&self.rows[i]));
                }
                rep.rows_removed = self.keep_where(flags);
            }
            Step::AddId { column, from, prefix } => {
                let idx = self.cols(name, from)?;
                let existing = self.columns.iter().position(|c| c == column);
                for row in &mut self.rows {
                    let parts: Vec<String> = idx
                        .iter()
                        .map(|&i| match &row[i] {
                            Value::String(s) => s.clone(),
                            Value::Null => String::new(),
                            other => other.to_string(),
                        })
                        .collect();
                    let refs: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
                    let id = format!("{}{}", prefix.as_deref().unwrap_or(""), fnv1a_hex(&refs));
                    match existing {
                        Some(c) => row[c] = Value::String(id),
                        None => row.push(Value::String(id)),
                    }
                    rep.cells_changed += 1;
                }
                if existing.is_none() {
                    self.columns.push(column.clone());
                }
            }
            Step::Chunk { column, size, overlap, index_column } => {
                let c = self.col(name, column)?;
                if !index_column.is_empty() && self.columns.contains(index_column) {
                    return Err(CleanError::Config(format!("chunk: column `{index_column}` already exists")));
                }
                let before = self.rows.len();
                let rows = std::mem::take(&mut self.rows);
                let origin = std::mem::take(&mut self.origin);
                for (row, org) in rows.into_iter().zip(origin) {
                    let chunks = match &row[c] {
                        Value::String(s) => chunk_text(s, *size, *overlap),
                        _ => vec![],
                    };
                    if chunks.is_empty() {
                        rep.rows_rejected += 1;
                        let record = self.record(&row);
                        self.rejects.push(Reject {
                            row: org,
                            step: name.into(),
                            reason: format!("column `{column}` has no text to chunk"),
                            record,
                        });
                        continue;
                    }
                    for (n, piece) in chunks.into_iter().enumerate() {
                        let mut r = row.clone();
                        r[c] = Value::String(piece);
                        if !index_column.is_empty() {
                            r.push(Value::from(n as u64));
                        }
                        self.rows.push(r);
                        self.origin.push(org);
                    }
                }
                if !index_column.is_empty() {
                    self.columns.push(index_column.clone());
                }
                let produced = self.rows.len();
                let consumed = before - rep.rows_rejected;
                rep.rows_added = produced.saturating_sub(consumed);
            }
        }
        Ok(rep)
    }
}

fn cast_value(v: &Value, to: CastType, decimal_comma: bool) -> std::result::Result<Value, String> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    match to {
        CastType::String => Ok(match v {
            Value::String(_) => v.clone(),
            Value::Bool(b) => Value::String(b.to_string()),
            Value::Number(n) => Value::String(n.to_string()),
            other => Value::String(other.to_string()),
        }),
        CastType::Int => match v {
            Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    Ok(v.clone())
                } else {
                    let f = n.as_f64().unwrap_or(f64::NAN);
                    if f.fract() == 0.0 && f.abs() < 9e15 { Ok(Value::from(f as i64)) } else { Err(format!("{n} is not a whole number")) }
                }
            }
            Value::Bool(b) => Ok(Value::from(*b as i64)),
            Value::String(s) => {
                let t = clean_number(s, decimal_comma);
                if let Ok(i) = t.parse::<i64>() {
                    return Ok(Value::from(i));
                }
                match t.parse::<f64>() {
                    Ok(f) if f.is_finite() && f.fract() == 0.0 && f.abs() < 9e15 => Ok(Value::from(f as i64)),
                    _ => Err(format!("cannot read {s:?} as an integer")),
                }
            }
            other => Err(format!("cannot read {other} as an integer")),
        },
        CastType::Float => match v {
            Value::Number(n) => n
                .as_f64()
                .and_then(Number::from_f64)
                .map(Value::Number)
                .ok_or_else(|| format!("{n} is not a finite number")),
            Value::String(s) => clean_number(s, decimal_comma)
                .parse::<f64>()
                .ok()
                .filter(|f| f.is_finite())
                .and_then(Number::from_f64)
                .map(Value::Number)
                .ok_or_else(|| format!("cannot read {s:?} as a number")),
            other => Err(format!("cannot read {other} as a number")),
        },
        CastType::Bool => match v {
            Value::Bool(_) => Ok(v.clone()),
            Value::Number(n) if n.as_i64() == Some(1) => Ok(Value::Bool(true)),
            Value::Number(n) if n.as_i64() == Some(0) => Ok(Value::Bool(false)),
            Value::String(s) => match s.trim().to_lowercase().as_str() {
                "true" | "t" | "yes" | "y" | "1" | "oui" | "o" => Ok(Value::Bool(true)),
                "false" | "f" | "no" | "n" | "0" | "non" => Ok(Value::Bool(false)),
                _ => Err(format!("cannot read {s:?} as a boolean")),
            },
            other => Err(format!("cannot read {other} as a boolean")),
        },
    }
}

/// `" 1 234,5 "` / `"1,234.5"` / `"+7"` → something `str::parse` understands.
fn clean_number(s: &str, decimal_comma: bool) -> String {
    let t: String = s.trim().chars().filter(|c| !c.is_whitespace() && *c != '_' && *c != '\u{202f}').collect();
    let t = t.strip_prefix('+').unwrap_or(&t).to_string();
    if decimal_comma { t.replace('.', "").replace(',', ".") } else { t.replace(',', "") }
}

fn check_rule(v: &Value, r: &Rule, re: Option<&Regex>) -> Option<String> {
    if v.is_null() {
        return None;
    }
    let col = &r.column;
    if r.min.is_some() || r.max.is_some() {
        let Some(n) = v.as_f64() else {
            return Some(format!("column `{col}`: {v} is not a number"));
        };
        if let Some(min) = r.min {
            if n < min {
                return Some(format!("column `{col}`: {n} is below the minimum {min}"));
            }
        }
        if let Some(max) = r.max {
            if n > max {
                return Some(format!("column `{col}`: {n} is above the maximum {max}"));
            }
        }
    }
    if r.min_len.is_some() || r.max_len.is_some() || re.is_some() {
        let Some(s) = v.as_str() else {
            return Some(format!("column `{col}`: {v} is not a string"));
        };
        let len = s.chars().count();
        if let Some(m) = r.min_len {
            if len < m {
                return Some(format!("column `{col}`: {len} characters, at least {m} required"));
            }
        }
        if let Some(m) = r.max_len {
            if len > m {
                return Some(format!("column `{col}`: {len} characters, at most {m} allowed"));
            }
        }
        if let Some(re) = re {
            if !re.is_match(s) {
                return Some(format!("column `{col}`: {s:?} does not match {}", re.as_str()));
            }
        }
    }
    if let Some(allowed) = &r.one_of {
        if !allowed.contains(v) {
            return Some(format!("column `{col}`: {v} is not one of the allowed values"));
        }
    }
    None
}
