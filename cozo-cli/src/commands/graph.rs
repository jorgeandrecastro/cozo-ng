use miette::{miette, Result};

use super::schema::columns;
use super::Ctx;
use crate::util::{ident, parse_value};

/// Edge rule over the first two columns of the relation, whatever they are called.
fn edges(ctx: &Ctx, relation: &str) -> Result<String> {
    ident(relation, false)?;
    let cols = columns(&*ctx.backend, relation)?;
    if cols.len() < 2 {
        return Err(miette!("`{relation}` needs at least two columns (from, to) to be used as a graph"));
    }
    Ok(format!("edges[a, b] := *{relation}{{ {}: a, {}: b }}\n", cols[0].name, cols[1].name))
}

pub fn pagerank(ctx: &Ctx, relation: &str, top: usize) -> Result<()> {
    let script = format!(
        "{}?[node, rank] <~ PageRank(edges[])\n:order -rank\n:limit {top}",
        edges(ctx, relation)?
    );
    ctx.emit(&ctx.read(&script)?)
}

pub fn neighbors(ctx: &Ctx, relation: &str, node: &str, hops: u32) -> Result<()> {
    if hops == 0 {
        return Err(miette!("--hops must be at least 1"));
    }
    let script = format!(
        "{}reach[x, min(h)] := edges[$node, x], h = 1\n\
         reach[x, min(h)] := reach[m, h0], h0 < $hops, edges[m, x], h = h0 + 1\n\
         ?[node, hops] := reach[node, hops]\n:order hops, node",
        edges(ctx, relation)?
    );
    let params = [
        ("node".to_string(), parse_value(node)),
        ("hops".to_string(), cozo::DataValue::from(hops as i64)),
    ]
    .into();
    ctx.emit(&ctx.backend.run(&script, params, false)?)
}

pub fn path(ctx: &Ctx, relation: &str, from: &str, to: &str) -> Result<()> {
    let script = format!(
        "{}start[x] := x = $from\ngoal[x] := x = $to\n\
         ?[src, dst, distance, path] <~ ShortestPathDijkstra(edges[], start[], goal[])",
        edges(ctx, relation)?
    );
    let params = [("from".to_string(), parse_value(from)), ("to".to_string(), parse_value(to))].into();
    ctx.emit(&ctx.backend.run(&script, params, false)?)
}
