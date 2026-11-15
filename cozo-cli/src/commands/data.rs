use std::fs::File;
use std::io::{BufWriter, Write};

use cozo::DataValue;
use miette::{miette, IntoDiagnostic, Result};
use serde_json::Value as JsonValue;

use super::schema::{columns, Column};
use super::Ctx;
use crate::cli::{ExportArgs, ImportArgs, InputFormat, PutOp};
use crate::output::render;
use crate::util::{ident, read_input};

fn detect(file: &str) -> InputFormat {
    let f = file.to_lowercase();
    if f.ends_with(".csv") || f.ends_with(".tsv") {
        InputFormat::Csv
    } else if f.ends_with(".jsonl") || f.ends_with(".ndjson") {
        InputFormat::Jsonl
    } else if f.ends_with(".json") {
        InputFormat::Json
    } else {
        InputFormat::Jsonl
    }
}

/// Turns a CSV field into JSON according to the declared column type.
fn coerce(s: &str, ty: &str) -> JsonValue {
    let nullable = ty.ends_with('?');
    let base = ty.trim_end_matches('?');
    if nullable && (s.is_empty() || s == "null") {
        return JsonValue::Null;
    }
    match base {
        "String" => JsonValue::String(s.to_string()),
        "Int" => s.trim().parse::<i64>().map(JsonValue::from).unwrap_or_else(|_| JsonValue::String(s.into())),
        "Float" => s.trim().parse::<f64>().map(JsonValue::from).unwrap_or_else(|_| JsonValue::String(s.into())),
        "Bool" => match s.trim().to_lowercase().as_str() {
            "true" | "1" | "yes" => JsonValue::Bool(true),
            "false" | "0" | "no" => JsonValue::Bool(false),
            _ => JsonValue::String(s.into()),
        },
        // lists, vectors (`<F32; 384>`), json, any…: expect JSON text
        _ => serde_json::from_str(s).unwrap_or_else(|_| JsonValue::String(s.into())),
    }
}

pub struct Records {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<JsonValue>>,
}

fn read_csv(text: &str, args: &ImportArgs, cols: &[Column]) -> Result<Records> {
    let delim = if args.file.to_lowercase().ends_with(".tsv") && args.delimiter == ',' { '\t' } else { args.delimiter };
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim as u8)
        .has_headers(!args.no_header)
        .flexible(false)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = if args.no_header {
        cols.iter().map(|c| c.name.clone()).collect()
    } else {
        rdr.headers().into_diagnostic()?.iter().map(|h| h.trim().to_string()).collect()
    };
    let types: Vec<&str> = headers
        .iter()
        .map(|h| cols.iter().find(|c| &c.name == h).map(|c| c.ty.as_str()).unwrap_or("Any"))
        .collect();
    let mut rows = vec![];
    for (i, rec) in rdr.records().enumerate() {
        let rec = rec.map_err(|e| miette!("CSV error near record {}: {e}", i + 1))?;
        rows.push(rec.iter().zip(&types).map(|(f, t)| coerce(f, t)).collect());
    }
    Ok(Records { headers, rows })
}

fn from_json_values(values: Vec<JsonValue>, cols: &[Column]) -> Result<Records> {
    let mut headers: Option<Vec<String>> = None;
    let mut rows = vec![];
    for (i, v) in values.into_iter().enumerate() {
        match v {
            JsonValue::Object(m) => {
                let h = headers.get_or_insert_with(|| {
                    // relation column order, restricted to the keys of the first object
                    let known: Vec<String> =
                        cols.iter().filter(|c| m.contains_key(&c.name)).map(|c| c.name.clone()).collect();
                    known
                });
                if let Some(unknown) = m.keys().find(|k| !cols.iter().any(|c| &c.name == *k)) {
                    return Err(miette!("record {}: `{unknown}` is not a column of the relation", i + 1));
                }
                rows.push(h.iter().map(|k| m.get(k).cloned().unwrap_or(JsonValue::Null)).collect());
            }
            JsonValue::Array(a) => {
                headers.get_or_insert_with(|| cols.iter().map(|c| c.name.clone()).collect());
                rows.push(a);
            }
            other => return Err(miette!("record {}: expected an object or an array, got {other}", i + 1)),
        }
    }
    Ok(Records { headers: headers.unwrap_or_default(), rows })
}

pub fn import(ctx: &Ctx, args: ImportArgs) -> Result<()> {
    let rel = ident(&args.relation, false)?;
    let cols = columns(&*ctx.backend, rel)?;
    if cols.is_empty() {
        return Err(miette!("relation `{rel}` does not exist (create it first with `:create`)"));
    }
    let fmt = args.input_format.unwrap_or_else(|| detect(&args.file));
    let text = read_input(&args.file)?;
    let recs = match fmt {
        InputFormat::Csv => read_csv(&text, &args, &cols)?,
        InputFormat::Jsonl => {
            let vals = text
                .lines()
                .enumerate()
                .filter(|(_, l)| !l.trim().is_empty())
                .map(|(i, l)| serde_json::from_str(l).map_err(|e| miette!("line {}: {e}", i + 1)))
                .collect::<Result<Vec<JsonValue>>>()?;
            from_json_values(vals, &cols)?
        }
        InputFormat::Json => match serde_json::from_str::<JsonValue>(&text).into_diagnostic()? {
            JsonValue::Array(a) => from_json_values(a, &cols)?,
            _ => return Err(miette!("a JSON import file must contain an array of objects or arrays")),
        },
    };
    put_records(ctx, rel, &cols, recs, args.op, args.batch, fmt_name(fmt))
}

/// Strings that sit in a typed column (`Int`, `Float`, `Bool`, lists, vectors) are converted first,
/// so CSV-like sources and cleaned tables can be written without a schema of their own.
fn coerce_cell(v: JsonValue, ty: &str) -> JsonValue {
    match v {
        JsonValue::String(s) => {
            let base = ty.trim_end_matches('?');
            if matches!(base, "Int" | "Float" | "Bool") || base.starts_with('<') || base.starts_with('[') {
                coerce(&s, ty)
            } else {
                JsonValue::String(s)
            }
        }
        other => other,
    }
}

/// Writes records into `rel` in batches of `batch` rows (one transaction per batch).
pub fn put_records(
    ctx: &Ctx,
    rel: &str,
    cols: &[Column],
    recs: Records,
    op: PutOp,
    batch: usize,
    source: &str,
) -> Result<()> {
    if recs.rows.is_empty() {
        eprintln!("nothing to import");
        return Ok(());
    }
    for h in &recs.headers {
        ident(h, false)?;
        if !cols.iter().any(|c| &c.name == h) {
            return Err(miette!("`{h}` is not a column of `{rel}`"));
        }
    }
    let types: Vec<&str> = recs
        .headers
        .iter()
        .map(|h| cols.iter().find(|c| &c.name == h).map(|c| c.ty.as_str()).unwrap_or("Any"))
        .collect();
    let list = recs.headers.join(", ");
    let script = format!("?[{list}] <- $data\n:{} {rel} {{ {list} }}", op.keyword());
    let total = recs.rows.len();
    let mut done = 0usize;
    for chunk in recs.rows.chunks(batch.max(1)) {
        let data = DataValue::List(
            chunk
                .iter()
                .map(|r| {
                    DataValue::List(
                        r.iter().zip(&types).map(|(v, t)| DataValue::from(coerce_cell(v.clone(), t))).collect(),
                    )
                })
                .collect(),
        );
        ctx.backend
            .run(&script, [("data".to_string(), data)].into(), true)
            .map_err(|e| e.wrap_err(format!("import failed after {done} of {total} rows (earlier batches are committed)")))?;
        done += chunk.len();
        if ctx.tty {
            eprint!("\r{done}/{total} rows");
        }
    }
    if ctx.tty {
        eprintln!();
    }
    eprintln!("{} {total} rows into `{rel}` ({source})", op.keyword());
    Ok(())
}

fn fmt_name(f: InputFormat) -> &'static str {
    match f {
        InputFormat::Csv => "csv",
        InputFormat::Json => "json",
        InputFormat::Jsonl => "jsonl",
    }
}

pub fn export(ctx: &Ctx, args: ExportArgs) -> Result<()> {
    let rel = ident(&args.relation, false)?;
    let all = columns(&*ctx.backend, rel)?;
    if all.is_empty() {
        return Err(miette!("relation `{rel}` does not exist"));
    }
    let names: Vec<String> = if args.columns.is_empty() {
        all.iter().map(|c| c.name.clone()).collect()
    } else {
        for c in &args.columns {
            ident(c, false)?;
            if !all.iter().any(|a| &a.name == c) {
                return Err(miette!("`{c}` is not a column of `{rel}`"));
            }
        }
        args.columns.clone()
    };
    let list = names.join(", ");
    let mut script = format!("?[{list}] := *{rel}{{ {list} }}");
    if let Some(n) = args.limit {
        script.push_str(&format!("\n:limit {n}"));
    }
    let res = ctx.read(&script)?;
    match &args.output {
        None => ctx.emit(&res),
        Some(path) => {
            let mut w = BufWriter::new(File::create(path).map_err(|e| miette!("cannot create {}: {e}", path.display()))?);
            let mut opts = ctx.opts();
            opts.footer = false;
            opts.truncate = 0;
            render(&res, opts, &mut w).into_diagnostic()?;
            w.flush().into_diagnostic()?;
            eprintln!("{} rows written to {}", res.rows.len(), path.display());
            Ok(())
        }
    }
}
