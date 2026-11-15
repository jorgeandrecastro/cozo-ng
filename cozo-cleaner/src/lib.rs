//! # cozo-ng-cleaner
//!
//! Data cleaning and preparation pipelines for [Cozo-NG](https://github.com/jorgeandrecastro/cozo-ng),
//! AI agents and GraphRAG. The crate is independent of the database engine: it turns messy tabular
//! data (CSV, JSON, JSONL) into clean, typed, deduplicated, validated rows, and can split long texts
//! into overlapping chunks ready for embedding.
//!
//! A [`Pipeline`] is an ordered list of [`Step`]s. Running it on a [`Table`] gives an [`Output`]: the
//! clean table, the rejected rows with the reason for each, and a [`Report`].
//!
//! ```
//! use cozo_ng_cleaner::{io, Pipeline};
//!
//! let csv = "id,name,age\n1,  Ada  Lovelace ,36\n1,  Ada  Lovelace ,36\n2,NA,x\n";
//! let table = io::read_table(csv, io::Format::Csv).unwrap();
//!
//! let pipeline = Pipeline::from_toml(r#"
//!     [[steps]]
//!     op = "normalize_text"
//!     [[steps]]
//!     op = "nulls"
//!     [[steps]]
//!     op = "cast"
//!     types = { id = "int", age = "int" }
//!     on_error = "null"
//!     [[steps]]
//!     op = "dedup"
//!     by = ["id"]
//! "#).unwrap();
//!
//! let out = pipeline.run(table).unwrap();
//! assert_eq!(out.table.rows.len(), 2);
//! assert_eq!(out.report.rows_in, 3);
//! ```

mod chunk;
mod error;
pub mod io;
mod pipeline;
mod report;
mod step;
mod table;
mod text;

pub use chunk::chunk_text;
pub use error::{CleanError, Result};
pub use pipeline::{Output, Pipeline, EXAMPLE_CONFIG};
pub use report::{Reject, Report, StepReport};
pub use step::{
    CastType, Keep, OnError, Rule, Step, UnicodeForm, Whitespace, DEFAULT_NULLS,
};
pub use table::Table;
pub use text::{fnv1a_hex, normalize, snake_case, NormalizeOptions};
