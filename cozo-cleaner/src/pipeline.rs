use serde::{Deserialize, Serialize};

use crate::error::{CleanError, Result};
use crate::report::{Reject, Report};
use crate::step::{State, Step};
use crate::table::Table;

/// Result of running a [`Pipeline`].
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    /// The clean table.
    pub table: Table,
    /// Rows that were set aside, with the reason.
    pub rejects: Vec<Reject>,
    pub report: Report,
}

/// An ordered list of cleaning steps.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Pipeline {
    #[serde(default)]
    pub steps: Vec<Step>,
}

impl Pipeline {
    pub fn new() -> Self {
        Pipeline::default()
    }

    /// A sensible default for messy text data: normalize text, turn placeholders into nulls,
    /// drop empty rows and exact duplicates.
    pub fn standard() -> Self {
        Pipeline::new()
            .step(Step::normalize_text())
            .step(Step::Nulls { columns: vec![], values: None })
            .step(Step::DropEmptyRows)
            .step(Step::Dedup { by: vec![], keep: Default::default() })
    }

    /// Appends a step (builder style).
    pub fn step(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }

    pub fn from_toml(text: &str) -> Result<Self> {
        let p: Pipeline = toml::from_str(text).map_err(|e| CleanError::Config(e.to_string()))?;
        p.check()?;
        Ok(p)
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| CleanError::Config(e.to_string()))
    }

    /// Validates every step's parameters (without data).
    pub fn check(&self) -> Result<()> {
        self.steps.iter().try_for_each(Step::check)
    }

    /// Runs the pipeline. Rows are never silently lost: each one is either in the output table,
    /// counted as removed (empty rows, duplicates) or listed in `rejects` with a reason.
    pub fn run(&self, table: Table) -> Result<Output> {
        self.check()?;
        let width = table.columns.len();
        if let Some(bad) = table.rows.iter().position(|r| r.len() != width) {
            return Err(CleanError::Parse(format!(
                "row {} has {} cells, expected {width}",
                bad + 1,
                table.rows[bad].len()
            )));
        }
        let rows_in = table.rows.len();
        let mut state = State {
            origin: (1..=rows_in).collect(),
            columns: table.columns,
            rows: table.rows,
            rejects: vec![],
        };
        let mut report = Report { rows_in, ..Default::default() };
        for step in &self.steps {
            report.steps.push(state.apply(step)?);
        }
        report.rows_out = state.rows.len();
        report.rejected = state.rejects.len();
        Ok(Output { table: Table::new(state.columns, state.rows), rejects: state.rejects, report })
    }
}

/// A complete, commented example, printed by `cozo-ng clean --example-config`.
pub const EXAMPLE_CONFIG: &str = r#"# Cleaning pipeline: steps run from top to bottom.
# Every step is a [[steps]] table with `op = "<name>"`.

[[steps]]
op = "snake_case_headers"          # "Unit Price (EUR)" -> "unit_price_eur"

[[steps]]
op = "normalize_text"              # Unicode NFC, collapse whitespace, strip control/zero-width chars
# columns = ["title", "body"]      # default: every column
# whitespace = "keep_lines"        # keep paragraph breaks (use this before `chunk`)
# strip_html = true
# lowercase = true

[[steps]]
op = "nulls"                       # "", "NA", "N/A", "null", "none", "nan", "-" become real nulls
# values = ["", "n.a.", "?"]       # or choose your own placeholders

[[steps]]
op = "drop_empty_rows"

[[steps]]
op = "cast"                        # strings -> typed values
types = { id = "int", price = "float", in_stock = "bool" }
on_error = "null"                  # null | reject | keep
# decimal_comma = true             # read 1.234,5 as 1234.5

[[steps]]
op = "fill"
values = { country = "unknown" }

[[steps]]
op = "require"                     # rows without an id or title go to the rejects file
columns = ["id", "title"]

[[steps]]
op = "validate"
rules = [
  { column = "price", min = 0 },
  { column = "title", max_len = 300 },
  { column = "email", pattern = '^[^@\s]+@[^@\s]+$' },
]

[[steps]]
op = "dedup"
by = ["id"]                        # empty = whole row
keep = "first"                     # first | last

# RAG preparation: split long texts into overlapping chunks, then give each chunk a stable id.
# [[steps]]
# op = "chunk"
# column = "body"
# size = 800
# overlap = 100
#
# [[steps]]
# op = "add_id"
# column = "chunk_id"
# from = ["id", "chunk_index"]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_parses() {
        let p = Pipeline::from_toml(EXAMPLE_CONFIG).unwrap();
        assert!(p.steps.len() >= 8);
    }

    #[test]
    fn unknown_fields_and_ops_are_errors() {
        assert!(Pipeline::from_toml("[[steps]]\nop = \"trim\"\ncolumnz = [\"a\"]").is_err());
        assert!(Pipeline::from_toml("[[steps]]\nop = \"frobnicate\"").is_err());
        assert!(Pipeline::from_toml("[[steps]]\nop = \"chunk\"\ncolumn = \"t\"\nsize = 10\noverlap = 10").is_err());
        assert!(Pipeline::from_toml("[[steps]]\nop = \"validate\"\nrules = [{ column = \"a\", pattern = \"(\" }]").is_err());
    }

    #[test]
    fn toml_roundtrip() {
        let p = Pipeline::standard();
        assert_eq!(Pipeline::from_toml(&p.to_toml().unwrap()).unwrap(), p);
    }
}
