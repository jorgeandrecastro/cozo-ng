/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Python bindings for the Cozo-NG embedded database.
//!
//! The crates.io package is `cozo-ng-python`; the native extension module is
//! `cozo_embedded`. The Python API and installation instructions are in the
//! crate README.

use std::collections::{BTreeMap, BTreeSet};

use miette::{IntoDiagnostic, Report, Result};
use pyo3::exceptions::{PyException, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyByteArray, PyBytes, PyDict, PyList, PyLong, PyString, PyTuple};
use serde_json::json;

use cozo::*;

const DB_CLOSED_MSG: &str = "database closed";

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

/// Converts a miette report into a plain Python exception (text message).
fn report2py(r: Report) -> PyErr {
    PyException::new_err(r.to_string())
}

/// Converts a Cozo error into a Python exception whose first argument is the
/// decoded JSON diagnostics (as produced by `format_error_as_json`).
fn report_to_pyerr(py: Python<'_>, err: Report, query: &str) -> PyErr {
    let reports = format_error_as_json(err, Some(query)).to_string();
    match py
        .import("json")
        .and_then(|m| m.getattr("loads"))
        .and_then(|f| f.call1((reports.as_str(),)))
    {
        Ok(msg) => PyException::new_err(PyObject::from(msg)),
        Err(e) => e,
    }
}

fn closed_err() -> PyErr {
    PyException::new_err(DB_CLOSED_MSG)
}

fn ensure_callable(obj: &PyAny, what: &str) -> PyResult<()> {
    if obj.is_callable() {
        Ok(())
    } else {
        Err(PyTypeError::new_err(format!("{what} must be callable")))
    }
}

// ---------------------------------------------------------------------------
// Python -> Cozo conversions
// ---------------------------------------------------------------------------

fn py_to_rows(ob: &PyAny) -> PyResult<Vec<Vec<DataValue>>> {
    let rows = ob.extract::<Vec<Vec<&PyAny>>>()?;
    rows.into_iter()
        .map(|row| row.into_iter().map(py_to_value).collect::<PyResult<_>>())
        .collect()
}

fn py_to_named_rows(ob: &PyAny) -> PyResult<NamedRows> {
    let d = ob.downcast::<PyDict>()?;
    let rows = d
        .get_item("rows")?
        .ok_or_else(|| PyException::new_err("named rows must contain 'rows'"))?;
    let rows = py_to_rows(rows)?;
    let headers = d
        .get_item("headers")?
        .ok_or_else(|| PyException::new_err("named rows must contain 'headers'"))?;
    let headers = headers.extract::<Vec<String>>()?;
    Ok(NamedRows::new(headers, rows))
}

fn py_to_value(ob: &PyAny) -> PyResult<DataValue> {
    Ok(if ob.is_none() {
        DataValue::Null
    } else if let Ok(b) = ob.downcast::<PyBool>() {
        DataValue::from(b.is_true())
    } else if let Ok(i) = ob.extract::<i64>() {
        DataValue::from(i)
    } else if ob.is_instance_of::<PyLong>() {
        // An int that does not fit in i64: refuse instead of silently
        // degrading to a float.
        return Err(PyValueError::new_err(format!(
            "integer {ob} is out of the supported i64 range"
        )));
    } else if let Ok(f) = ob.extract::<f64>() {
        DataValue::from(f)
    } else if let Ok(s) = ob.extract::<String>() {
        DataValue::from(s)
    } else if let Ok(b) = ob.downcast::<PyBytes>() {
        DataValue::Bytes(b.as_bytes().to_vec())
    } else if let Ok(b) = ob.downcast::<PyByteArray>() {
        // Safe copy: no reference into a buffer that Python may mutate.
        DataValue::Bytes(b.to_vec())
    } else if let Ok(l) = ob.downcast::<PyTuple>() {
        let mut coll = Vec::with_capacity(l.len());
        for el in l {
            coll.push(py_to_value(el)?);
        }
        DataValue::List(coll)
    } else if let Ok(l) = ob.downcast::<PyList>() {
        let mut coll = Vec::with_capacity(l.len());
        for el in l {
            coll.push(py_to_value(el)?);
        }
        DataValue::List(coll)
    } else if let Ok(d) = ob.downcast::<PyDict>() {
        let mut coll = serde_json::Map::default();
        for (k, v) in d {
            let k = serde_json::Value::from(py_to_value(k)?);
            let k = match k {
                serde_json::Value::String(s) => s,
                s => s.to_string(),
            };
            let v = serde_json::Value::from(py_to_value(v)?);
            coll.insert(k, v);
        }
        DataValue::Json(JsonData(json!(coll)))
    } else {
        return Err(PyTypeError::new_err(format!(
            "Cannot convert {ob} into Cozo value"
        )));
    })
}

fn convert_params(ob: &PyDict) -> PyResult<BTreeMap<String, DataValue>> {
    let mut ret = BTreeMap::new();
    for (k, v) in ob {
        let k: String = k.extract()?;
        let v = py_to_value(v)?;
        ret.insert(k, v);
    }
    Ok(ret)
}

// ---------------------------------------------------------------------------
// Cozo -> Python conversions
// ---------------------------------------------------------------------------

fn options_to_py(opts: BTreeMap<String, DataValue>, py: Python<'_>) -> PyResult<PyObject> {
    let ret = PyDict::new(py);
    for (k, v) in opts {
        ret.set_item(k, value_to_py(v, py))?;
    }
    Ok(ret.into())
}

fn json_to_py(val: serde_json::Value, py: Python<'_>) -> PyObject {
    match val {
        serde_json::Value::Null => py.None(),
        serde_json::Value::Bool(b) => b.into_py(py),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_py(py)
            } else if let Some(f) = n.as_f64() {
                f.into_py(py)
            } else {
                py.None()
            }
        }
        serde_json::Value::String(s) => s.into_py(py),
        serde_json::Value::Array(a) => {
            let vs: Vec<_> = a.into_iter().map(|v| json_to_py(v, py)).collect();
            vs.into_py(py)
        }
        serde_json::Value::Object(o) => {
            let d = PyDict::new(py);
            for (k, v) in o {
                d.set_item(k, json_to_py(v, py))
                    .expect("setting an item on a fresh dict cannot fail");
            }
            d.into()
        }
    }
}

fn value_to_py(val: DataValue, py: Python<'_>) -> PyObject {
    match val {
        DataValue::Null => py.None(),
        DataValue::Bool(b) => b.into_py(py),
        DataValue::Num(num) => match num {
            Num::Int(i) => i.into_py(py),
            Num::Float(f) => f.into_py(py),
        },
        DataValue::Str(s) => s.as_str().into_py(py),
        DataValue::Bytes(b) => PyBytes::new(py, &b).into(),
        DataValue::Uuid(uuid) => uuid.0.to_string().into_py(py),
        DataValue::Regex(rx) => rx.0.as_str().into_py(py),
        DataValue::List(l) => {
            let vs: Vec<_> = l.into_iter().map(|v| value_to_py(v, py)).collect();
            vs.into_py(py)
        }
        DataValue::Set(l) => {
            let vs: Vec<_> = l.into_iter().map(|v| value_to_py(v, py)).collect();
            vs.into_py(py)
        }
        DataValue::Validity(vld) => {
            [vld.timestamp.0 .0.into_py(py), vld.is_assert.0.into_py(py)].into_py(py)
        }
        DataValue::Bot => py.None(),
        DataValue::Vec(v) => match v {
            Vector::F32(a) => {
                let vs: Vec<_> = a.into_iter().map(|v| v.into_py(py)).collect();
                vs.into_py(py)
            }
            Vector::F64(a) => {
                let vs: Vec<_> = a.into_iter().map(|v| v.into_py(py)).collect();
                vs.into_py(py)
            }
        },
        DataValue::Json(JsonData(j)) => json_to_py(j, py),
    }
}

fn rows_to_py_rows(rows: Vec<Vec<DataValue>>, py: Python<'_>) -> PyObject {
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|val| value_to_py(val, py))
                .collect::<Vec<_>>()
                .into_py(py)
        })
        .collect::<Vec<_>>()
        .into_py(py)
}

fn named_rows_to_py(named_rows: NamedRows, py: Python<'_>) -> PyObject {
    let rows = rows_to_py_rows(named_rows.rows, py);
    let headers = named_rows.headers.into_py(py);
    let next = match named_rows.next {
        None => py.None(),
        Some(nxt) => named_rows_to_py(*nxt, py),
    };
    BTreeMap::from([("rows", rows), ("headers", headers), ("next", next)]).into_py(py)
}

// ---------------------------------------------------------------------------
// Classes
// ---------------------------------------------------------------------------

#[pyclass]
struct CozoDbPy {
    db: Option<DbInstance>,
}

#[pyclass]
struct CozoDbMulTx {
    tx: MultiTransaction,
}

impl CozoDbPy {
    /// Returns the open database or a "database closed" error.
    fn db(&self) -> PyResult<&DbInstance> {
        self.db.as_ref().ok_or_else(closed_err)
    }
}

#[pymethods]
impl CozoDbPy {
    #[new]
    fn new(engine: &str, path: &str, options: &str) -> PyResult<Self> {
        match DbInstance::new(engine, path, options) {
            Ok(db) => Ok(Self { db: Some(db) }),
            Err(err) => Err(PyException::new_err(format!("{err:?}"))),
        }
    }

    pub fn run_script(
        &self,
        py: Python<'_>,
        query: &str,
        params: &PyDict,
        immutable: bool,
    ) -> PyResult<PyObject> {
        let db = self.db()?;
        let params = convert_params(params)?;
        let mutability = if immutable {
            ScriptMutability::Immutable
        } else {
            ScriptMutability::Mutable
        };
        py.allow_threads(|| db.run_script(query, params, mutability))
            .map(|rows| named_rows_to_py(rows, py))
            .map_err(|e| report_to_pyerr(py, e, query))
    }

    /// Registers `callback(op, new_rows, old_rows)` for changes on relation `rel`.
    /// The callback runs on a dedicated thread, not on the rayon pool.
    pub fn register_callback(&self, rel: &str, callback: &PyAny) -> PyResult<u32> {
        let db = self.db()?;
        ensure_callable(callback, "callback")?;
        let cb: Py<PyAny> = callback.into();
        let (id, ch) = db.register_callback(rel, None);
        std::thread::Builder::new()
            .name(format!("cozo-callback-{id}"))
            .spawn(move || {
                for (op, new, old) in ch {
                    Python::with_gil(|py| {
                        let op_py: PyObject = PyString::new(py, op.as_str()).into();
                        let new_py = rows_to_py_rows(new.rows, py);
                        let old_py = rows_to_py_rows(old.rows, py);
                        let args = PyTuple::new(py, [op_py, new_py, old_py]);
                        if let Err(err) = cb.as_ref(py).call1(args) {
                            // Keeps the Python traceback.
                            err.print(py);
                        }
                    })
                }
            })
            .map_err(|e| PyException::new_err(e.to_string()))?;
        Ok(id)
    }

    pub fn register_fixed_rule(
        &self,
        name: String,
        arity: usize,
        callback: &PyAny,
    ) -> PyResult<()> {
        let db = self.db()?;
        ensure_callable(callback, "callback")?;
        let cb: Py<PyAny> = callback.into();
        let rule_impl = SimpleFixedRule::new(
            arity,
            move |inputs: Vec<NamedRows>, options: BTreeMap<String, DataValue>| -> Result<_> {
                Python::with_gil(|py| -> Result<NamedRows> {
                    let py_inputs = PyList::new(
                        py,
                        inputs.into_iter().map(|nr| rows_to_py_rows(nr.rows, py)),
                    );
                    let py_opts = options_to_py(options, py).into_diagnostic()?;
                    let args = PyTuple::new(py, vec![PyObject::from(py_inputs), py_opts]);
                    let res = cb.as_ref(py).call1(args).into_diagnostic()?;
                    Ok(NamedRows::new(vec![], py_to_rows(res).into_diagnostic()?))
                })
            },
        );
        db.register_fixed_rule(name, rule_impl).map_err(report2py)
    }

    /// Returns `False` if the database is closed or the id is unknown.
    pub fn unregister_callback(&self, id: u32) -> bool {
        self.db.as_ref().is_some_and(|db| db.unregister_callback(id))
    }

    pub fn unregister_fixed_rule(&self, name: &str) -> PyResult<bool> {
        self.db()?.unregister_fixed_rule(name).map_err(report2py)
    }

    pub fn export_relations(&self, py: Python<'_>, relations: Vec<String>) -> PyResult<PyObject> {
        let db = self.db()?;
        let res = py
            .allow_threads(|| db.export_relations(relations.iter()))
            .map_err(report2py)?;
        let ret = PyDict::new(py);
        for (k, v) in res {
            ret.set_item(k, named_rows_to_py(v, py))?;
        }
        Ok(ret.into())
    }

    pub fn import_relations(&self, py: Python<'_>, data: &PyDict) -> PyResult<()> {
        let db = self.db()?;
        let mut arg = BTreeMap::new();
        for (k, v) in data.iter() {
            let k = k.extract::<String>()?;
            arg.insert(k, py_to_named_rows(v)?);
        }
        py.allow_threads(|| db.import_relations(arg))
            .map_err(report2py)
    }

    pub fn backup(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let db = self.db()?;
        py.allow_threads(|| db.backup_db(path)).map_err(report2py)
    }

    pub fn restore(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let db = self.db()?;
        py.allow_threads(|| db.restore_backup(path))
            .map_err(report2py)
    }

    pub fn import_from_backup(
        &self,
        py: Python<'_>,
        in_file: &str,
        relations: Vec<String>,
    ) -> PyResult<()> {
        let db = self.db()?;
        py.allow_threads(|| db.import_from_backup(in_file, &relations))
            .map_err(report2py)
    }

    /// Closes the database. Returns `True` if it was open.
    pub fn close(&mut self) -> bool {
        self.db.take().is_some()
    }

    pub fn multi_transact(&self, write: bool) -> PyResult<CozoDbMulTx> {
        let db = self.db()?;
        Ok(CozoDbMulTx {
            tx: db.multi_transaction(write),
        })
    }
}

#[pymethods]
impl CozoDbMulTx {
    pub fn abort(&self) -> PyResult<()> {
        self.tx.abort().map_err(report2py)
    }

    pub fn commit(&self) -> PyResult<()> {
        self.tx.commit().map_err(report2py)
    }

    pub fn run_script(&self, py: Python<'_>, query: &str, params: &PyDict) -> PyResult<PyObject> {
        let params = convert_params(params)?;
        py.allow_threads(|| self.tx.run_script(query, params))
            .map(|rows| named_rows_to_py(rows, py))
            .map_err(|e| report_to_pyerr(py, e, query))
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

#[pyfunction]
fn eval_expressions(
    py: Python<'_>,
    query: &str,
    params: &PyDict,
    bindings: &PyDict,
) -> PyResult<PyObject> {
    let params = convert_params(params)?;
    let bindings = convert_params(bindings)?;
    py.allow_threads(|| evaluate_expressions(query, &params, &bindings))
        .map(|v| value_to_py(v, py))
        .map_err(|e| report_to_pyerr(py, e, query))
}

#[pyfunction]
fn variables(py: Python<'_>, query: &str, params: &PyDict) -> PyResult<BTreeSet<String>> {
    let params = convert_params(params)?;
    get_variables(query, &params).map_err(|e| report_to_pyerr(py, e, query))
}

#[pymodule]
fn cozo_embedded(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<CozoDbPy>()?;
    m.add_class::<CozoDbMulTx>()?;
    m.add_function(wrap_pyfunction!(eval_expressions, m)?)?;
    m.add_function(wrap_pyfunction!(variables, m)?)?;
    Ok(())
}