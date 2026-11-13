/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::BTreeMap;

use itertools::Itertools;
use miette::{miette, Result};
use serde_json::json;

use crate::data::json::JsonValue;
use crate::data::tuple::Tuple;
use crate::data::value::DataValue;

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Debug, Clone, Default)]
/// Rows in a relation, together with headers for the fields.
pub struct NamedRows {
    /// The headers
    pub headers: Vec<String>,
    /// The rows
    pub rows: Vec<Tuple>,
    /// Contains the next named rows, if exists
    pub next: Option<Box<NamedRows>>,
}

impl IntoIterator for NamedRows {
    type Item = Tuple;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.rows.into_iter()
    }
}

impl NamedRows {
    /// create a named rows with the given headers and rows
    pub fn new(headers: Vec<String>, rows: Vec<Tuple>) -> Self {
        Self {
            headers,
            rows,
            next: None,
        }
    }

    /// If there are more named rows after the current one
    pub fn has_more(&self) -> bool {
        self.next.is_some()
    }

    /// convert a chain of named rows to individual named rows
    pub fn flatten(self) -> Vec<Self> {
        let mut collected = vec![];
        let mut current = self;
        loop {
            let nxt = current.next.take();
            collected.push(current);
            if let Some(n) = nxt {
                current = *n;
            } else {
                break;
            }
        }
        collected
    }

    /// Convert to a JSON object
    pub fn into_json(self) -> JsonValue {
        let nxt = match self.next {
            None => json!(null),
            Some(more) => more.into_json(),
        };
        let rows = self
            .rows
            .into_iter()
            .map(|row| row.into_iter().map(JsonValue::from).collect::<JsonValue>())
            .collect::<JsonValue>();
        json!({
            "headers": self.headers,
            "rows": rows,
            "next": nxt,
        })
    }
    /// Make named rows from JSON
    pub fn from_json(value: &JsonValue) -> Result<Self> {
        let headers = value
            .get("headers")
            .ok_or_else(|| miette!("NamedRows requires 'headers' field"))?;
        let headers = headers
            .as_array()
            .ok_or_else(|| miette!("'headers' field must be an array"))?;
        let headers = headers
            .iter()
            .map(|h| -> Result<String> {
                let h = h
                    .as_str()
                    .ok_or_else(|| miette!("'headers' field must be an array of strings"))?;
                Ok(h.to_string())
            })
            .try_collect()?;
        let rows = value
            .get("rows")
            .ok_or_else(|| miette!("NamedRows requires 'rows' field"))?;
        let rows = rows
            .as_array()
            .ok_or_else(|| miette!("'rows' field must be an array"))?;
        let rows = rows
            .iter()
            .map(|row| -> Result<Vec<DataValue>> {
                let row = row
                    .as_array()
                    .ok_or_else(|| miette!("'rows' field must be an array of arrays"))?;
                Ok(row.iter().map(DataValue::from).collect_vec())
            })
            .try_collect()?;
        Ok(Self {
            headers,
            rows,
            next: None,
        })
    }

    /// Create a query and parameters to apply an operation (insert, put, delete, rm) to a stored
    /// relation with the named rows.
    pub fn into_payload(self, relation: &str, op: &str) -> Payload {
        let cols_str = self.headers.join(", ");
        let query = format!("?[{cols_str}] <- $data :{op} {relation} {{ {cols_str} }}");
        let data = DataValue::List(self.rows.into_iter().map(|r| DataValue::List(r)).collect());
        (query, [("data".to_string(), data)].into())
    }
}

/// The query and parameters.
pub type Payload = (String, BTreeMap<String, DataValue>);

/// Commands to be sent to a multi-transaction
#[derive(Eq, PartialEq, Debug)]
pub enum TransactionPayload {
    /// Commit the current transaction
    Commit,
    /// Abort the current transaction
    Abort,
    /// Run a query inside the transaction
    Query(Payload),
}