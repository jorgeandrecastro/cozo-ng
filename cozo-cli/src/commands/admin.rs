use std::time::Instant;

use miette::Result;

use super::schema::{columns, relation_names};
use super::Ctx;
use crate::target::Target;
use crate::util::{cell, dir_size, human_bytes, ident};

pub fn backup(ctx: &Ctx, path: &str) -> Result<()> {
    ctx.backend.backup(path)?;
    match ctx.target {
        Target::Remote { .. } => eprintln!("backup written on the server at {path}"),
        _ => eprintln!("backup written to {path}"),
    }
    Ok(())
}

pub fn restore(ctx: &Ctx, path: &str) -> Result<()> {
    ctx.backend.restore(path)?;
    eprintln!("restored from {path}");
    Ok(())
}

pub fn doctor(ctx: &Ctx) -> Result<()> {
    println!("cozo-ng-cli  {}", env!("CARGO_PKG_VERSION"));
    println!("target       {}", ctx.target.describe());

    let t = Instant::now();
    let rels = ctx.read("::relations")?;
    println!("connection   ok ({:.1?})", t.elapsed());

    if let Target::Embedded { engine, path } = &ctx.target {
        if engine != "mem" {
            let size = dir_size(std::path::Path::new(path));
            println!("on disk      {}", human_bytes(size));
        }
    }

    let names = relation_names(&*ctx.backend)?;
    let index_count = rels.rows.len().saturating_sub(names.len());
    println!("relations    {}{}", names.len(), if index_count > 0 { format!(" (+{index_count} index relations)") } else { String::new() });
    if names.is_empty() {
        return Ok(());
    }

    let mut table = comfy_table::Table::new();
    table.load_preset(comfy_table::presets::UTF8_FULL_CONDENSED);
    table.set_header(["relation", "columns", "rows"]);
    for n in &names {
        ident(n, false)?;
        let cols = columns(&*ctx.backend, n)?;
        let rows = match cols.first() {
            Some(c) => {
                let r = ctx.read(&format!("?[count({c})] := *{n}{{ {c} }}", c = c.name))?;
                r.rows.first().map(|row| cell(&row[0])).unwrap_or_else(|| "0".into())
            }
            None => "0".into(),
        };
        table.add_row([n.clone(), cols.len().to_string(), rows]);
    }
    println!("{table}");
    Ok(())
}
