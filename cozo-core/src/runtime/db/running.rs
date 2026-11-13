/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[allow(unused_imports)]
use std::thread;
#[allow(unused_imports)]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[allow(unused_imports)]
use miette::{bail, Diagnostic, IntoDiagnostic, Result};
use thiserror::Error;

pub(crate) struct RunningQueryHandle {
    pub(crate) started_at: f64,
    pub(crate) poison: Poison,
}

pub(crate) struct RunningQueryCleanup {
    pub(crate) id: u64,
    pub(crate) running_queries: Arc<Mutex<BTreeMap<u64, RunningQueryHandle>>>,
}

impl Drop for RunningQueryCleanup {
    fn drop(&mut self) {
        let mut map = self.running_queries.lock().unwrap();
        if let Some(handle) = map.remove(&self.id) {
            handle.poison.0.store(true, Ordering::Relaxed);
        }
    }
}

/// Used for user-initiated termination of running queries
#[derive(Clone, Default)]
pub struct Poison(pub(crate) Arc<AtomicBool>);

impl Poison {
    /// Will return `Err` if user has initiated termination.
    #[inline(always)]
    pub fn check(&self) -> Result<()> {
        #[derive(Debug, Error, Diagnostic)]
        #[error("Running query is killed before completion")]
        #[diagnostic(code(eval::killed))]
        #[diagnostic(help("A query may be killed by timeout, or explicit command"))]
        struct ProcessKilled;

        if self.0.load(Ordering::Relaxed) {
            bail!(ProcessKilled)
        }
        Ok(())
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn set_timeout(&self, _secs: f64) -> Result<()> {
        bail!("Cannot set timeout when threading is disallowed");
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_timeout(&self, secs: f64) -> Result<()> {
        let pill = self.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_micros((secs * 1000000.) as u64));
            pill.0.store(true, Ordering::Relaxed);
        });
        Ok(())
    }
}

pub(crate) fn seconds_since_the_epoch() -> Result<f64> {
    #[cfg(not(target_arch = "wasm32"))]
    let now = SystemTime::now();
    #[cfg(not(target_arch = "wasm32"))]
    return Ok(now
        .duration_since(UNIX_EPOCH)
        .into_diagnostic()?
        .as_secs_f64());

    #[cfg(target_arch = "wasm32")]
    Ok(js_sys::Date::now())
}