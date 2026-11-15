use std::fmt;

use serde_json::{Map, Value};

/// A row that did not make it into the output, with the reason.
#[derive(Debug, Clone, PartialEq)]
pub struct Reject {
    /// 1-based row number in the input (data rows, header excluded).
    pub row: usize,
    /// The step that rejected it.
    pub step: String,
    pub reason: String,
    /// The row as it was when it was rejected.
    pub record: Map<String, Value>,
}

/// What one step did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StepReport {
    pub step: String,
    pub cells_changed: usize,
    pub rows_removed: usize,
    pub rows_added: usize,
    pub rows_rejected: usize,
}

/// What the whole pipeline did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub rows_in: usize,
    pub rows_out: usize,
    pub rejected: usize,
    pub steps: Vec<StepReport>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "rows: {} in, {} out, {} rejected", self.rows_in, self.rows_out, self.rejected)?;
        for (i, s) in self.steps.iter().enumerate() {
            let mut parts = vec![];
            if s.cells_changed > 0 {
                parts.push(format!("{} cells changed", s.cells_changed));
            }
            if s.rows_removed > 0 {
                parts.push(format!("{} rows removed", s.rows_removed));
            }
            if s.rows_added > 0 {
                parts.push(format!("{} rows added", s.rows_added));
            }
            if s.rows_rejected > 0 {
                parts.push(format!("{} rows rejected", s.rows_rejected));
            }
            if parts.is_empty() {
                parts.push("no change".into());
            }
            writeln!(f, "  {:>2}. {:<18} {}", i + 1, s.step, parts.join(", "))?;
        }
        Ok(())
    }
}
