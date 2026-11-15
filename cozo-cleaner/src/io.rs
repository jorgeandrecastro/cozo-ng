//! Reading and writing tables: CSV, TSV, JSON (array of objects) and JSONL.

use std::io::Write;

use serde_json::{Map, Value};

use crate::error::{CleanError, Result};
use crate::report::Reject;
use crate::table::Table;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Csv,
    Tsv,
    /// A JSON array of objects.
    Json,
    /// One JSON object per line.
    Jsonl,
}

impl Format {
    /// From a file name; `None` when the extension is not recognised.
    pub fn from_path(path: &str) -> Option<Format> {
        let p = path.to_lowercase();
        match p.rsplit('.').next()? {
            "csv" => Some(Format::Csv),
            "tsv" => Some(Format::Tsv),
            "json" => Some(Format::Json),
            "jsonl" | "ndjson" => Some(Format::Jsonl),
            _ => None,
        }
    }
}

/// Parses `text`. CSV/TSV cells become strings (use the `cast` step to type them);
/// JSON keeps its types. For JSON the columns are the union of all keys, in first-seen order.
pub fn read_table(text: &str, format: Format) -> Result<Table> {
    match format {
        Format::Csv | Format::Tsv => {
            let delim = if format == Format::Tsv { b'\t' } else { b',' };
            let mut rdr = csv::ReaderBuilder::new().delimiter(delim).from_reader(text.trim_start_matches('\u{feff}').as_bytes());
            let columns: Vec<String> =
                rdr.headers().map_err(|e| CleanError::Parse(e.to_string()))?.iter().map(|h| h.to_string()).collect();
            let mut rows = vec![];
            for (i, rec) in rdr.records().enumerate() {
                let rec = rec.map_err(|e| CleanError::Parse(format!("record {}: {e}", i + 1)))?;
                rows.push(rec.iter().map(|f| Value::String(f.to_string())).collect());
            }
            Ok(Table::new(columns, rows))
        }
        Format::Json => {
            let v: Value = serde_json::from_str(text).map_err(|e| CleanError::Parse(e.to_string()))?;
            match v {
                Value::Array(a) => from_objects(a),
                _ => Err(CleanError::Parse("expected a JSON array of objects".into())),
            }
        }
        Format::Jsonl => {
            let mut objs = vec![];
            for (i, line) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
                objs.push(serde_json::from_str(line).map_err(|e| CleanError::Parse(format!("line {}: {e}", i + 1)))?);
            }
            from_objects(objs)
        }
    }
}

fn from_objects(objs: Vec<Value>) -> Result<Table> {
    let mut columns: Vec<String> = vec![];
    for (i, o) in objs.iter().enumerate() {
        let Value::Object(m) = o else {
            return Err(CleanError::Parse(format!("record {} is not a JSON object", i + 1)));
        };
        for k in m.keys() {
            if !columns.contains(k) {
                columns.push(k.clone());
            }
        }
    }
    let rows = objs
        .iter()
        .map(|o| columns.iter().map(|c| o.get(c).cloned().unwrap_or(Value::Null)).collect())
        .collect();
    Ok(Table::new(columns, rows))
}

fn csv_cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn object(columns: &[String], row: &[Value]) -> Value {
    Value::Object(columns.iter().cloned().zip(row.iter().cloned()).collect::<Map<_, _>>())
}

/// Writes `table`. Nulls are empty cells in CSV/TSV.
pub fn write_table(table: &Table, format: Format, w: &mut dyn Write) -> Result<()> {
    let io = |e: std::io::Error| CleanError::Io(e.to_string());
    match format {
        Format::Csv | Format::Tsv => {
            let delim = if format == Format::Tsv { b'\t' } else { b',' };
            let mut cw = csv::WriterBuilder::new().delimiter(delim).from_writer(w);
            cw.write_record(&table.columns).map_err(|e| CleanError::Io(e.to_string()))?;
            for r in &table.rows {
                cw.write_record(r.iter().map(csv_cell)).map_err(|e| CleanError::Io(e.to_string()))?;
            }
            cw.flush().map_err(io)
        }
        Format::Json => {
            let arr: Vec<Value> = table.rows.iter().map(|r| object(&table.columns, r)).collect();
            writeln!(w, "{}", serde_json::to_string_pretty(&arr).unwrap_or_default()).map_err(io)
        }
        Format::Jsonl => {
            for r in &table.rows {
                writeln!(w, "{}", object(&table.columns, r)).map_err(io)?;
            }
            Ok(())
        }
    }
}

/// Writes rejects as JSONL: `{"row": 3, "step": "require", "reason": "…", "record": {…}}`.
pub fn write_rejects(rejects: &[Reject], w: &mut dyn Write) -> Result<()> {
    for r in rejects {
        let line = serde_json::json!({ "row": r.row, "step": r.step, "reason": r.reason, "record": r.record });
        writeln!(w, "{line}").map_err(|e| CleanError::Io(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_roundtrip_and_bom() {
        let t = read_table("\u{feff}a,b\n1,\"x,y\"\n", Format::Csv).unwrap();
        assert_eq!(t.columns, vec!["a", "b"]);
        let mut out = vec![];
        write_table(&t, Format::Csv, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "a,b\n1,\"x,y\"\n");
    }

    #[test]
    fn jsonl_unions_keys() {
        let t = read_table("{\"a\":1}\n{\"b\":2}\n", Format::Jsonl).unwrap();
        assert_eq!(t.columns, vec!["a", "b"]);
        assert_eq!(t.rows[0][1], Value::Null);
    }

    #[test]
    fn formats_from_paths() {
        assert_eq!(Format::from_path("x.NDJSON"), Some(Format::Jsonl));
        assert_eq!(Format::from_path("x.txt"), None);
    }
}
