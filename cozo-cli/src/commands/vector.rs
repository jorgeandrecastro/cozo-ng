use cozo::DataValue;
use miette::{miette, Result};
use serde_json::Value as JsonValue;

use super::schema::columns;
use super::Ctx;
use crate::cli::{Distance, Dtype};
use crate::util::{ident, read_input};

#[allow(clippy::too_many_arguments)]
pub fn create_index(
    ctx: &Ctx,
    relation: &str,
    name: &str,
    field: &str,
    dim: u32,
    dtype: Dtype,
    distance: Distance,
    m: u32,
    ef_construction: u32,
) -> Result<()> {
    ident(relation, false)?;
    ident(name, false)?;
    ident(field, false)?;
    let dtype = match dtype {
        Dtype::F32 => "F32",
        Dtype::F64 => "F64",
    };
    let distance = match distance {
        Distance::L2 => "L2",
        Distance::Cosine => "Cosine",
        Distance::Ip => "IP",
    };
    let script = format!(
        "::hnsw create {relation}:{name} {{ dim: {dim}, m: {m}, dtype: {dtype}, fields: [{field}], \
         distance: {distance}, ef_construction: {ef_construction} }}"
    );
    ctx.emit(&ctx.backend.run(&script, Default::default(), true)?)
}

/// Accepts a JSON array, or plain numbers separated by whitespace and/or commas.
fn parse_vector(text: &str) -> Result<Vec<f64>> {
    let t = text.trim();
    if t.starts_with('[') {
        let v: JsonValue = serde_json::from_str(t).map_err(|e| miette!("invalid vector: {e}"))?;
        return v
            .as_array()
            .ok_or_else(|| miette!("the vector must be an array"))?
            .iter()
            .map(|x| x.as_f64().ok_or_else(|| miette!("vector entries must be numbers")))
            .collect();
    }
    t.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<f64>().map_err(|_| miette!("invalid number in vector: {s:?}")))
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn search(
    ctx: &Ctx,
    relation: &str,
    index: &str,
    vector: Option<String>,
    vector_file: Option<String>,
    k: u32,
    ef: u32,
    radius: Option<f64>,
    filter: Option<String>,
    select: &[String],
) -> Result<()> {
    ident(relation, false)?;
    ident(index, false)?;
    let text = match (vector, vector_file) {
        (Some(v), _) => v,
        (None, Some(f)) => read_input(&f)?,
        (None, None) => return Err(miette!("give the query with --vector '[…]' or --vector-file <file>")),
    };
    let q = parse_vector(&text)?;
    if q.is_empty() {
        return Err(miette!("the query vector is empty"));
    }

    let all = columns(&*ctx.backend, relation)?;
    if all.is_empty() {
        return Err(miette!("relation `{relation}` does not exist"));
    }
    let cols: Vec<String> = if select.is_empty() {
        // vector columns are huge and rarely useful in the output
        all.iter().filter(|c| !c.ty.trim_start_matches('[').starts_with('<')).map(|c| c.name.clone()).collect()
    } else {
        for c in select {
            ident(c, false)?;
        }
        select.to_vec()
    };
    if cols.is_empty() {
        return Err(miette!("no column left to return; pass --columns"));
    }
    let list = cols.join(", ");
    let mut opts = String::from("query: q, k: $k, ef: $ef, bind_distance: dist");
    if let Some(r) = radius {
        opts.push_str(&format!(", radius: {r}"));
    }
    if let Some(f) = &filter {
        opts.push_str(&format!(", filter: {f}"));
    }
    let script = format!(
        "?[dist, {list}] := q = vec($q), ~{relation}:{index}{{ {list} | {opts} }}\n:order dist"
    );
    let params = [
        ("q".to_string(), DataValue::List(q.into_iter().map(DataValue::from).collect())),
        ("k".to_string(), DataValue::from(k as i64)),
        ("ef".to_string(), DataValue::from(ef as i64)),
    ]
    .into();
    ctx.emit(&ctx.backend.run(&script, params, false)?)
}
