/*
 * Copyright 2022, The Cozo Project Authors.
 *
 * This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
 * If a copy of the MPL was not distributed with this file,
 * You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint, jstring};
use jni::JNIEnv;
use lazy_static::lazy_static;

use cozo::*;

#[derive(Default)]
struct Handles {
    current: AtomicI32,
    dbs: Mutex<BTreeMap<i32, DbInstance>>,
}

lazy_static! {
    static ref HANDLES: Handles = Handles {
        current: Default::default(),
        dbs: Mutex::new(Default::default())
    };
}

fn get_db(id: i32) -> Option<DbInstance> {
    let dbs = HANDLES.dbs.lock().unwrap();
    dbs.get(&id).cloned()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_cozodb_CozoJavaBridge_openDb(
    mut env: JNIEnv,
    _class: JClass,
    engine: JString,
    path: JString,
    options: JString,
) -> jint {
    env.with_env(|env| -> jni::errors::Result<jint> {
        let engine: String = env.get_string(&engine)?.into();
        let path: String = env.get_string(&path)?.into();
        let options: String = env.get_string(&options)?.into();
        let id = match DbInstance::new(&engine, path, &options) {
            Ok(db) => {
                let id = HANDLES.current.fetch_add(1, Ordering::AcqRel);
                let mut dbs = HANDLES.dbs.lock().unwrap();
                dbs.insert(id, db);
                id
            }
            Err(err) => {
                eprintln!("{err:?}");
                -1
            }
        };
        Ok(id)
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_cozodb_CozoJavaBridge_closeDb(
    mut env: JNIEnv,
    _class: JClass,
    id: jint,
) -> jboolean {
    env.with_env(|_| -> jni::errors::Result<jboolean> {
        let db = {
            let mut dbs = HANDLES.dbs.lock().unwrap();
            dbs.remove(&id)
        };
        Ok(db.is_some().into())
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

const DB_NOT_FOUND: &str = r#"{"ok":false,"message":"database not found"}"#;

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_cozodb_CozoJavaBridge_runQuery(
    mut env: JNIEnv,
    _class: JClass,
    id: jint,
    script: JString,
    params_str: JString,
) -> jstring {
    env.with_env(|env| -> jni::errors::Result<jstring> {
        let script: String = env.get_string(&script)?.into();
        let params_str: String = env.get_string(&params_str)?.into();
        match get_db(id) {
            None => Ok(env.new_string(DB_NOT_FOUND)?.into_raw()),
            Some(db) => {
                let res = db.run_script_str(&script, &params_str, false);
                Ok(env.new_string(res)?.into_raw())
            }
        }
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_cozodb_CozoJavaBridge_exportRelations(
    mut env: JNIEnv,
    _class: JClass,
    id: jint,
    rel: JString,
) -> jstring {
    env.with_env(|env| -> jni::errors::Result<jstring> {
        let rel: String = env.get_string(&rel)?.into();
        match get_db(id) {
            None => Ok(env.new_string(DB_NOT_FOUND)?.into_raw()),
            Some(db) => {
                let res = db.export_relations_str(&rel);
                Ok(env.new_string(res)?.into_raw())
            }
        }
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_cozodb_CozoJavaBridge_importRelations(
    mut env: JNIEnv,
    _class: JClass,
    id: jint,
    data: JString,
) -> jstring {
    env.with_env(|env| -> jni::errors::Result<jstring> {
        let data: String = env.get_string(&data)?.into();
        match get_db(id) {
            None => Ok(env.new_string(DB_NOT_FOUND)?.into_raw()),
            Some(db) => {
                let res = db.import_relations_str(&data);
                Ok(env.new_string(res)?.into_raw())
            }
        }
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_cozodb_CozoJavaBridge_backup(
    mut env: JNIEnv,
    _class: JClass,
    id: jint,
    file: JString,
) -> jstring {
    env.with_env(|env| -> jni::errors::Result<jstring> {
        let file: String = env.get_string(&file)?.into();
        match get_db(id) {
            None => Ok(env.new_string(DB_NOT_FOUND)?.into_raw()),
            Some(db) => {
                let res = db.backup_db_str(file);
                Ok(env.new_string(res)?.into_raw())
            }
        }
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_cozodb_CozoJavaBridge_restore(
    mut env: JNIEnv,
    _class: JClass,
    id: jint,
    file: JString,
) -> jstring {
    env.with_env(|env| -> jni::errors::Result<jstring> {
        let file: String = env.get_string(&file)?.into();
        match get_db(id) {
            None => Ok(env.new_string(DB_NOT_FOUND)?.into_raw()),
            Some(db) => {
                let res = db.restore_backup_str(file);
                Ok(env.new_string(res)?.into_raw())
            }
        }
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_cozodb_CozoJavaBridge_importFromBackup(
    mut env: JNIEnv,
    _class: JClass,
    id: jint,
    data: JString,
) -> jstring {
    env.with_env(|env| -> jni::errors::Result<jstring> {
        let data: String = env.get_string(&data)?.into();
        match get_db(id) {
            None => Ok(env.new_string(DB_NOT_FOUND)?.into_raw()),
            Some(db) => {
                let res = db.import_from_backup_str(&data);
                Ok(env.new_string(res)?.into_raw())
            }
        }
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}
