//! `cozo-ng clean`: prepare data with `cozo-ng-cleaner`, then write a file or import into a relation.

use std::fs::File;
use std::io::{BufWriter, Write};

use cozo_ng_cleaner::{io, Pipeline, EXAMPLE_CONFIG};
use miette::{miette, IntoDiagnostic, Result};

use super::data::{put_records, Records};
use super::schema::columns;
use super::Ctx;
use crate::cli::{CleanArgs, Format, InputFormat};
use crate::util::{ident, read_input};

fn cleaner_format(f: InputFormat) -> io::Format {
    match f {
        InputFormat::Csv => io::Format::Csv,
        InputFormat::Json => io::Format::Json,
        InputFormat::Jsonl => io::Format::Jsonl,
    }
}

fn e(err: cozo_ng_cleaner::CleanError) -> miette::Report {
    miette!("{err}")
}

pub fn run(ctx: &Ctx, args: CleanArgs) -> Result<()> {
    if args.example_config {
        print!("{EXAMPLE_CONFIG}");
        return Ok(());
    }
    let file = args.file.as_deref().ok_or_else(|| miette!("give a file to clean (or \"-\" for stdin)"))?;

    // 1. read
    let in_format = match args.input_format {
        Some(f) => cleaner_format(f),
        None => io::Format::from_path(file).unwrap_or(io::Format::Jsonl),
    };
    let table = io::read_table(&read_input(file)?, in_format).map_err(e)?;

    // 2. pipeline
    let pipeline = match &args.config {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|err| miette!("cannot read {}: {err}", path.display()))?;
            Pipeline::from_toml(&text).map_err(|err| miette!("{}: {err}", path.display()))?
        }
        None => Pipeline::standard(),
    };

    // 3. relation checked before any work is thrown away
    let relation = match &args.into {
        Some(rel) => {
            ident(rel, false)?;
            let cols = columns(&*ctx.backend, rel)?;
            if cols.is_empty() {
                return Err(miette!("relation `{rel}` does not exist (create it first with `:create`)"));
            }
            Some((rel.as_str(), cols))
        }
        None => None,
    };

    let out = pipeline.run(table).map_err(e)?;

    if !args.quiet {
        eprint!("{}", out.report);
    }

    // 4. rejects
    if let Some(path) = &args.rejects {
        if !args.dry_run {
            let mut w = BufWriter::new(File::create(path).map_err(|err| miette!("cannot create {}: {err}", path.display()))?);
            io::write_rejects(&out.rejects, &mut w).map_err(e)?;
            w.flush().into_diagnostic()?;
        }
        eprintln!("{} rejected rows {} {}", out.rejects.len(), if args.dry_run { "would go to" } else { "written to" }, path.display());
    } else if !out.rejects.is_empty() {
        eprintln!("{} rows were rejected; use --rejects <file> to see them", out.rejects.len());
    }

    if args.dry_run {
        eprintln!("dry run: nothing written");
        return Ok(());
    }

    // 5. destination
    if let Some((rel, cols)) = relation {
        let recs = Records { headers: out.table.columns, rows: out.table.rows };
        return put_records(ctx, rel, &cols, recs, args.op, args.batch, "cleaned");
    }
    match &args.output {
        Some(path) => {
            let fmt = io::Format::from_path(&path.to_string_lossy()).unwrap_or(io::Format::Jsonl);
            let mut w = BufWriter::new(File::create(path).map_err(|err| miette!("cannot create {}: {err}", path.display()))?);
            io::write_table(&out.table, fmt, &mut w).map_err(e)?;
            w.flush().into_diagnostic()?;
            eprintln!("{} rows written to {}", out.table.rows.len(), path.display());
        }
        None => {
            let fmt = match ctx.format {
                Format::Csv => io::Format::Csv,
                Format::Json => io::Format::Json,
                _ => io::Format::Jsonl,
            };
            let mut stdout = std::io::stdout().lock();
            match io::write_table(&out.table, fmt, &mut stdout) {
                Ok(()) | Err(cozo_ng_cleaner::CleanError::Io(_)) => {}
                Err(err) => return Err(e(err)),
            }
        }
    }
    Ok(())
}
