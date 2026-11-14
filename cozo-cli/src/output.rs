use std::io::{self, Write};

use comfy_table::{presets::UTF8_FULL_CONDENSED, ContentArrangement, Table};
use cozo::NamedRows;
use serde_json::{Map, Value as JsonValue};

use crate::cli::Format;
use crate::util::{cell, to_json};

#[derive(Copy, Clone)]
pub struct RenderOpts {
    pub format: Format,
    pub truncate: usize,
    /// Print "(N rows)" under tables.
    pub footer: bool,
}

/// Renders a result (and the chain of results that may follow it).
pub fn render(rows: &NamedRows, opts: RenderOpts, w: &mut dyn Write) -> io::Result<()> {
    let mut cur = Some(rows);
    while let Some(r) = cur {
        render_one(r, opts, w)?;
        cur = r.next.as_deref();
    }
    Ok(())
}

fn object(headers: &[String], row: &[cozo::DataValue]) -> JsonValue {
    let mut m = Map::new();
    for (h, v) in headers.iter().zip(row) {
        m.insert(h.clone(), to_json(v));
    }
    JsonValue::Object(m)
}

fn shorten(s: String, max: usize) -> String {
    if max == 0 || s.chars().count() <= max {
        return s;
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn render_one(r: &NamedRows, opts: RenderOpts, w: &mut dyn Write) -> io::Result<()> {
    match opts.format {
        Format::Table => {
            let mut t = Table::new();
            t.load_preset(UTF8_FULL_CONDENSED).set_content_arrangement(ContentArrangement::Dynamic);
            t.set_header(r.headers.iter().map(|h| h.as_str()));
            for row in &r.rows {
                t.add_row(row.iter().map(|v| shorten(cell(v).replace('\n', "⏎"), opts.truncate)));
            }
            writeln!(w, "{t}")?;
            if opts.footer {
                let n = r.rows.len();
                writeln!(w, "({n} row{})", if n == 1 { "" } else { "s" })?;
            }
        }
        Format::Json => {
            let arr: Vec<JsonValue> = r.rows.iter().map(|row| object(&r.headers, row)).collect();
            writeln!(w, "{}", serde_json::to_string_pretty(&arr).unwrap_or_default())?;
        }
        Format::Ndjson => {
            for row in &r.rows {
                writeln!(w, "{}", object(&r.headers, row))?;
            }
        }
        Format::Csv => {
            let mut cw = csv::Writer::from_writer(&mut *w);
            cw.write_record(&r.headers).map_err(io::Error::other)?;
            for row in &r.rows {
                cw.write_record(row.iter().map(cell)).map_err(io::Error::other)?;
            }
            cw.flush()?;
        }
    }
    Ok(())
}
