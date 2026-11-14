use miette::{miette, Result};

use super::Ctx;
use crate::backend::Backend;
use crate::cli::SchemaCmd;
use crate::util::{cell, ident};

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub is_key: bool,
    pub ty: String,
    /// Default expression as printed by Cozo, if any.
    pub default_expr: Option<String>,
}

fn col_index(headers: &[String], name: &str) -> Result<usize> {
    headers.iter().position(|h| h == name).ok_or_else(|| miette!("unexpected result shape: no `{name}` column"))
}

/// Names of the stored relations (index relations such as `rel:idx` are skipped).
pub fn relation_names(b: &dyn Backend) -> Result<Vec<String>> {
    let res = b.run("::relations", Default::default(), false)?;
    let i = col_index(&res.headers, "name")?;
    Ok(res.rows.iter().map(|r| cell(&r[i])).filter(|n| !n.contains(':')).collect())
}

pub fn columns(b: &dyn Backend, relation: &str) -> Result<Vec<Column>> {
    ident(relation, false)?;
    let res = b.run(&format!("::columns {relation}"), Default::default(), false)?;
    let (c, k, t, d) = (
        col_index(&res.headers, "column")?,
        col_index(&res.headers, "is_key")?,
        col_index(&res.headers, "type")?,
        col_index(&res.headers, "default_expr")?,
    );
    Ok(res
        .rows
        .iter()
        .map(|r| Column {
            name: cell(&r[c]),
            is_key: cell(&r[k]) == "true",
            ty: cell(&r[t]),
            default_expr: match &r[d] {
                cozo::DataValue::Null => None,
                v => Some(cell(v)),
            },
        })
        .collect())
}

pub fn run(ctx: &Ctx, cmd: SchemaCmd) -> Result<()> {
    match cmd {
        SchemaCmd::Show { relation: None } => ctx.emit(&ctx.read("::relations")?),
        SchemaCmd::Show { relation: Some(r) } => {
            ident(&r, false)?;
            ctx.emit(&ctx.read(&format!("::columns {r}"))?)
        }
        SchemaCmd::Dump { relation } => {
            let names = match relation {
                Some(r) => vec![r],
                None => relation_names(&*ctx.backend)?,
            };
            print!("{}", dump(&*ctx.backend, &names)?);
            Ok(())
        }
    }
}

/// `:create` statements, one `{ … }` block per relation, runnable with `cozo-ng run`.
pub fn dump(b: &dyn Backend, names: &[String]) -> Result<String> {
    let mut out = String::new();
    for name in names {
        let cols = columns(b, name)?;
        let fmt = |c: &Column| match &c.default_expr {
            Some(d) => format!("{}: {} default {}", c.name, c.ty, d),
            None => format!("{}: {}", c.name, c.ty),
        };
        let keys: Vec<_> = cols.iter().filter(|c| c.is_key).map(fmt).collect();
        let vals: Vec<_> = cols.iter().filter(|c| !c.is_key).map(fmt).collect();
        let body = if vals.is_empty() { keys.join(", ") } else { format!("{} => {}", keys.join(", "), vals.join(", ")) };
        out.push_str(&format!("# relation {name}\n{{ :create {name} {{ {body} }} }}\n"));
        out.push('\n');
    }
    Ok(out)
}
