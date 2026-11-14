//! `cozo-ng`: the command-line tool for Cozo-NG.
//!
//! One binary, one URI scheme: talk to an embedded database (`mem:`, `sqlite:`,
//! `rocksdb:`) or to a remote `cozo-bin` server (`http://…`) with the same commands.

mod backend;
mod cli;
mod commands;
mod config;
mod output;
mod repl;
mod target;
mod util;

use clap::Parser;

fn main() -> miette::Result<()> {
    let cli = cli::Cli::parse();
    commands::dispatch(cli)
}
