use std::time::Instant;

use miette::Result;

use super::Ctx;
use crate::util::{parse_params, read_input};

pub fn query(ctx: &Ctx, script: &str, params: &[String], immutable: bool, timer: bool) -> Result<()> {
    let script = read_input_or_literal(script)?;
    execute(ctx, &script, params, immutable, timer)
}

pub fn run_file(ctx: &Ctx, file: &str, params: &[String], immutable: bool, timer: bool) -> Result<()> {
    let script = read_input(file)?;
    execute(ctx, &script, params, immutable, timer)
}

pub fn explain(ctx: &Ctx, script: &str) -> Result<()> {
    let script = read_input_or_literal(script)?;
    let res = ctx.backend.run(&format!("::explain {{ {script} }}"), Default::default(), false)?;
    ctx.emit(&res)
}

fn read_input_or_literal(s: &str) -> Result<String> {
    if s == "-" { read_input("-") } else { Ok(s.to_string()) }
}

fn execute(ctx: &Ctx, script: &str, params: &[String], immutable: bool, timer: bool) -> Result<()> {
    let params = parse_params(params)?;
    let start = Instant::now();
    let res = ctx.backend.run(script, params, !immutable)?;
    ctx.emit(&res)?;
    if timer {
        eprintln!("time: {:.1?}", start.elapsed());
    }
    Ok(())
}
