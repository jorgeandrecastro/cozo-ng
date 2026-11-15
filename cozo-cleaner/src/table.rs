use serde_json::Value;

/// A rectangular table: named columns and rows of JSON values.
///
/// CSV input yields strings; JSON input keeps its types. The `cast` step turns strings into numbers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    pub fn new(columns: Vec<String>, rows: Vec<Vec<Value>>) -> Self {
        Table { columns, rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
}
