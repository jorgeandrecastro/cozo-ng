//! Interactive shell.

mod helper;
pub mod validate;

use std::cell::RefCell;
use std::fs::File;
use std::io::{IsTerminal, Write};
use std::rc::Rc;
use std::time::Instant;

use miette::{miette, IntoDiagnostic, Result};
use rustyline::error::ReadlineError;
use rustyline::{Config, Editor};

use crate::cli::Format;
use crate::commands::schema::{columns, dump, relation_names};
use crate::commands::Ctx;
use crate::output::{render, RenderOpts};
use helper::{Cache, CozoHelper};

const HELP: &str = "\
Enter a CozoScript query. It runs as soon as it is complete:
  - an entry rule (`?[…] := …`) with balanced brackets runs on Enter;
  - end a line with `\\` to keep typing (e.g. to add `:order` / `:limit`);
  - a blank line always submits.

Commands:
  .help                 this help
  .relations            list relations
  .schema [REL]         show the CREATE statement of REL (or of every relation)
  .timer on|off         show query timings
  .format F             table | json | ndjson | csv
  .read FILE            run a script file
  .output FILE|off      send results to a file
  .quit                 leave (Ctrl-D also works)";

struct Session {
    ctx: Ctx,
    fmt: Format,
    timer: bool,
    out: Option<File>,
    cache: Rc<RefCell<Cache>>,
}

enum Flow {
    Continue,
    Quit,
}

impl Session {
    fn refresh_cache(&self) {
        let mut cache = Cache::default();
        if let Ok(names) = relation_names(&*self.ctx.backend) {
            for n in names.iter().take(300) {
                if let Ok(cols) = columns(&*self.ctx.backend, n) {
                    cache.columns.insert(n.clone(), cols.into_iter().map(|c| c.name).collect());
                }
            }
            cache.relations = names;
        }
        *self.cache.borrow_mut() = cache;
    }

    fn print(&mut self, rows: &cozo::NamedRows) {
        let opts = RenderOpts { format: self.fmt, truncate: self.ctx.truncate, footer: true };
        let res = match &mut self.out {
            Some(f) => render(rows, RenderOpts { footer: false, truncate: 0, ..opts }, f),
            None => render(rows, opts, &mut std::io::stdout().lock()),
        };
        if let Err(e) = res {
            eprintln!("output error: {e}");
        }
    }

    fn script(&mut self, text: &str) {
        let start = Instant::now();
        match self.ctx.backend.run(text, Default::default(), true) {
            Ok(rows) => {
                self.print(&rows);
                if self.timer {
                    eprintln!("time: {:.1?}", start.elapsed());
                }
                if text.contains(':') {
                    self.refresh_cache();
                }
            }
            Err(e) => eprintln!("{e:?}"),
        }
    }

    fn dot(&mut self, line: &str) -> Flow {
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next();
        match cmd {
            ".quit" | ".exit" | ".q" => return Flow::Quit,
            ".help" | ".h" => println!("{HELP}"),
            ".relations" => self.script("::relations"),
            ".schema" => {
                let names = match arg {
                    Some(a) => Ok(vec![a.to_string()]),
                    None => relation_names(&*self.ctx.backend),
                };
                match names.and_then(|n| dump(&*self.ctx.backend, &n)) {
                    Ok(s) => print!("{s}"),
                    Err(e) => eprintln!("{e:?}"),
                }
            }
            ".timer" => self.timer = matches!(arg, Some("on") | Some("1") | None) && arg != Some("off"),
            ".format" => match arg {
                Some("table") => self.fmt = Format::Table,
                Some("json") => self.fmt = Format::Json,
                Some("ndjson") => self.fmt = Format::Ndjson,
                Some("csv") => self.fmt = Format::Csv,
                _ => eprintln!("usage: .format table|json|ndjson|csv"),
            },
            ".read" => match arg {
                Some(path) => match std::fs::read_to_string(path) {
                    Ok(text) => self.script(&text),
                    Err(e) => eprintln!("cannot read {path}: {e}"),
                },
                None => eprintln!("usage: .read FILE"),
            },
            ".output" => match arg {
                None | Some("off") | Some("stdout") => self.out = None,
                Some(path) => match File::create(path) {
                    Ok(f) => self.out = Some(f),
                    Err(e) => eprintln!("cannot create {path}: {e}"),
                },
            },
            other => eprintln!("unknown command {other}; try .help"),
        }
        Flow::Continue
    }

    fn exec(&mut self, input: &str) -> Flow {
        let text = input.trim().trim_end_matches('\\').trim().to_string();
        if text.is_empty() {
            return Flow::Continue;
        }
        if text.starts_with('.') && !text.contains('\n') {
            return self.dot(&text);
        }
        self.script(&text.replace("\\\n", "\n"));
        Flow::Continue
    }
}

pub fn run(ctx: Ctx) -> Result<()> {
    let interactive = std::io::stdin().is_terminal();
    let cache = Rc::new(RefCell::new(Cache::default()));
    let fmt = if interactive { Format::Table } else { ctx.format };
    let mut session = Session { fmt, ctx, timer: false, out: None, cache };
    session.refresh_cache();

    if !interactive {
        return run_piped(session);
    }

    println!("cozo-ng {} — {}  (.help for help, Ctrl-D to quit)", env!("CARGO_PKG_VERSION"), session.ctx.target.describe());
    let cfg = Config::builder().auto_add_history(false).history_ignore_dups(true).into_diagnostic()?.build();
    let mut rl: Editor<CozoHelper, rustyline::history::FileHistory> = Editor::with_config(cfg).into_diagnostic()?;
    rl.set_helper(Some(CozoHelper::new(session.cache.clone())));
    let history = crate::config::history_path();
    if let Some(h) = &history {
        let _ = rl.load_history(h);
    }
    loop {
        match rl.readline("cozo> ") {
            Ok(line) => {
                if !line.trim().is_empty() {
                    let _ = rl.add_history_entry(line.as_str());
                }
                if let Flow::Quit = session.exec(&line) {
                    break;
                }
            }
            Err(ReadlineError::Interrupted) => println!("^C (use .quit or Ctrl-D to leave)"),
            Err(ReadlineError::Eof) => break,
            Err(e) => return Err(miette!("readline error: {e}")),
        }
        let _ = std::io::stdout().flush();
    }
    if let Some(h) = &history {
        let _ = rl.save_history(h);
    }
    Ok(())
}

/// Non-interactive stdin: same completeness rules, no editor.
fn run_piped(mut session: Session) -> Result<()> {
    let mut buf = String::new();
    for line in std::io::stdin().lines() {
        let line = line.into_diagnostic()?;
        // a `.command` always starts a new statement
        if line.trim_start().starts_with('.') && !buf.trim().is_empty() {
            if let Flow::Quit = session.exec(&buf) {
                return Ok(());
            }
            buf.clear();
        }
        if !buf.is_empty() {
            buf.push('\n');
        }
        buf.push_str(&line);
        if line.trim().is_empty() && !buf.trim().is_empty() || validate::is_complete(&buf) {
            if let Flow::Quit = session.exec(&buf) {
                return Ok(());
            }
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        session.exec(&buf);
    }
    Ok(())
}
