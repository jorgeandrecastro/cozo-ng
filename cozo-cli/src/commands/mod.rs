mod admin;
#[cfg(feature = "cleaner")]
mod clean;
mod data;
mod graph;
mod query;
pub mod schema;
mod vector;

use std::io::{IsTerminal, Write};

use clap::CommandFactory;
use cozo::NamedRows;
use miette::{miette, Result};

use crate::backend::{self, Backend};
use crate::cli::{Cli, Cmd, Format, GraphCmd, VecCmd};
use crate::config::Config;
use crate::output::{render, RenderOpts};
use crate::target::Target;

/// Shared state handed to every command.
pub struct Ctx {
    pub backend: Box<dyn Backend>,
    pub target: Target,
    pub format: Format,
    pub truncate: usize,
    pub tty: bool,
}

impl Ctx {
    pub fn opts(&self) -> RenderOpts {
        RenderOpts { format: self.format, truncate: self.truncate, footer: self.tty }
    }

    /// Prints a result on stdout.
    pub fn emit(&self, rows: &NamedRows) -> Result<()> {
        let mut out = std::io::stdout().lock();
        match render(rows, self.opts(), &mut out) {
            // `cozo-ng … | head` closes the pipe early: that is not an error.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(miette!("cannot write output: {e}")),
            Ok(()) => out.flush().map_err(|e| miette!("{e}")),
        }
    }

    pub fn read(&self, script: &str) -> Result<NamedRows> {
        self.backend.run(script, Default::default(), false)
    }
}

pub fn dispatch(cli: Cli) -> Result<()> {
    if let Cmd::Completions { shell } = &cli.cmd {
        let mut cmd = Cli::command();
        clap_complete::generate(*shell, &mut cmd, "cozo-ng", &mut std::io::stdout());
        return Ok(());
    }

    let cfg = Config::load();
    let uri = cli
        .db
        .clone()
        .or_else(|| cfg.default_db.clone())
        .map(|d| cfg.resolve(&d))
        .unwrap_or_else(|| "mem:".to_string());
    let target = Target::parse(&uri);
    let token = cli.token.clone().or(cfg.token.clone());

    let tty = std::io::stdout().is_terminal();
    let format = cli.format.unwrap_or(if tty { Format::Table } else { Format::Ndjson });
    let backend = backend::connect(&target, token)?;
    let ctx = Ctx { backend, target, format, truncate: cli.truncate, tty };

    match cli.cmd {
        Cmd::Query(a) => query::query(&ctx, &a.script, &a.params, a.immutable, a.timer),
        Cmd::Run(a) => query::run_file(&ctx, &a.file, &a.params, a.immutable, a.timer),
        Cmd::Explain(a) => query::explain(&ctx, &a.script),
        #[cfg(feature = "cleaner")]
        Cmd::Clean(a) => clean::run(&ctx, a),
        Cmd::Repl => crate::repl::run(ctx),
        Cmd::Relations => ctx.emit(&ctx.read("::relations")?),
        Cmd::Schema(s) => schema::run(&ctx, s),
        Cmd::Import(a) => data::import(&ctx, a),
        Cmd::Export(a) => data::export(&ctx, a),
        Cmd::Backup { path } => admin::backup(&ctx, &path.to_string_lossy()),
        Cmd::Restore { path } => admin::restore(&ctx, &path.to_string_lossy()),
        Cmd::Doctor => admin::doctor(&ctx),
        Cmd::Vec(VecCmd::CreateIndex { relation, name, field, dim, dtype, distance, m, ef_construction }) => {
            vector::create_index(&ctx, &relation, &name, &field, dim, dtype, distance, m, ef_construction)
        }
        Cmd::Vec(VecCmd::Search { relation, index, vector, vector_file, k, ef, radius, filter, columns }) => {
            vector::search(&ctx, &relation, &index, vector, vector_file, k, ef, radius, filter, &columns)
        }
        Cmd::Graph(GraphCmd::Pagerank { relation, top }) => graph::pagerank(&ctx, &relation, top),
        Cmd::Graph(GraphCmd::Neighbors { relation, node, hops }) => graph::neighbors(&ctx, &relation, &node, hops),
        Cmd::Graph(GraphCmd::Path { relation, from, to }) => graph::path(&ctx, &relation, &from, &to),
        Cmd::Completions { .. } => unreachable!(),
    }
}
