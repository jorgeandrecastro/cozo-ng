/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#![allow(unused_imports)]

use std::path::Path;
use std::sync::atomic::Ordering;

use itertools::Itertools;
use miette::{bail, Diagnostic, Result};
use smartstring::SmartString;
use thiserror::Error;

use crate::data::tuple::{Tuple, TupleT};
use crate::runtime::relation::{AccessLevel, InsufficientAccessLevel};
use crate::storage::Storage;

use super::{Db, ImportIntoIndex};

impl<'s, S: Storage<'s>> Db<S> {
    /// Backup the running database into an Sqlite file
    #[allow(unused_variables)]
    pub fn backup_db(&'s self, out_file: impl AsRef<Path>) -> Result<()> {
        #[cfg(feature = "storage-sqlite")]
        {
            let sqlite_db = crate::new_cozo_sqlite(out_file)?;
            if sqlite_db.relation_store_id.load(Ordering::SeqCst) != 0 {
                bail!("Cannot create backup: data exists in the target database.");
            }
            let mut tx = self.transact()?;
            let iter = tx.store_tx.range_scan(&[], &[0xFF]);
            sqlite_db.db.batch_put(iter)?;
            tx.commit_tx()?;
            Ok(())
        }
        #[cfg(not(feature = "storage-sqlite"))]
        bail!("backup requires the 'storage-sqlite' feature to be enabled")
    }

    /// Restore from an Sqlite backup
    #[allow(unused_variables)]
    pub fn restore_backup(&'s self, in_file: impl AsRef<Path>) -> Result<()> {
        #[cfg(feature = "storage-sqlite")]
        {
            let sqlite_db = crate::new_cozo_sqlite(in_file)?;
            let mut s_tx = sqlite_db.transact()?;
            {
                let mut tx = self.transact()?;
                let store_id = tx.relation_store_id.load(Ordering::SeqCst);
                if store_id != 0 {
                    bail!(
                        "Cannot restore backup: data exists in the current database. \
                You can only restore into a new database (store id: {}).",
                        store_id
                    );
                }
                tx.commit_tx()?;
            }
            let iter = s_tx.store_tx.total_scan();
            self.db.batch_put(iter)?;
            s_tx.commit_tx()?;
            Ok(())
        }
        #[cfg(not(feature = "storage-sqlite"))]
        bail!("backup requires the 'storage-sqlite' feature to be enabled")
    }

    /// Import data from relations in a backup file.
    /// The target stored relations must already exist in the database, and it must not
    /// have any associated indices. If you want to import into relations with indices,
    /// use [Db::import_relations].
    ///
    /// Note that triggers and callbacks are _not_ run for the relations, if any exists.
    /// If you need to activate triggers or callbacks, use queries with parameters.
    #[allow(unused_variables)]
    pub fn import_from_backup(
        &'s self,
        in_file: impl AsRef<Path>,
        relations: &[String],
    ) -> Result<()> {
        #[cfg(not(feature = "storage-sqlite"))]
        bail!("backup requires the 'storage-sqlite' feature to be enabled");

        #[cfg(feature = "storage-sqlite")]
        {
            let rel_names = relations.iter().map(SmartString::from).collect_vec();
            let locks = self.obtain_relation_locks(rel_names.iter());
            let _guards = locks.iter().map(|l| l.read().unwrap()).collect_vec();

            let source_db = crate::new_cozo_sqlite(in_file)?;
            let mut src_tx = source_db.transact()?;
            let mut dst_tx = self.transact_write()?;

            for relation in relations {
                if relation.contains(':') {
                    bail!(ImportIntoIndex(relation.to_string()))
                }
                let src_handle = src_tx.get_relation(relation, false)?;
                let dst_handle = dst_tx.get_relation(relation, false)?;

                if !dst_handle.indices.is_empty() {
                    #[derive(Debug, Error, Diagnostic)]
                    #[error("Cannot import data into relation {0} from backup as the relation has indices")]
                    #[diagnostic(code(tx::bare_import_with_indices))]
                    #[diagnostic(help("Use `import_relations()` instead"))]
                    pub(crate) struct RestoreIntoRelWithIndices(pub(crate) String);

                    bail!(RestoreIntoRelWithIndices(dst_handle.name.to_string()))
                }

                if dst_handle.access_level < AccessLevel::Protected {
                    bail!(InsufficientAccessLevel(
                        dst_handle.name.to_string(),
                        "data import".to_string(),
                        dst_handle.access_level
                    ));
                }

                let src_lower = Tuple::default().encode_as_key(src_handle.id);
                let src_upper = Tuple::default().encode_as_key(src_handle.id.next());

                let data_it = src_tx.store_tx.range_scan(&src_lower, &src_upper).map(
                    |src_pair| -> Result<(Vec<u8>, Vec<u8>)> {
                        let (mut src_k, mut src_v) = src_pair?;
                        dst_handle.amend_key_prefix(&mut src_k);
                        dst_handle.amend_key_prefix(&mut src_v);
                        Ok((src_k, src_v))
                    },
                );
                for result in data_it {
                    let (key, val) = result?;
                    dst_tx.store_tx.put(&key, &val)?;
                }
            }

            src_tx.commit_tx()?;
            dst_tx.commit_tx()
        }
    }
}