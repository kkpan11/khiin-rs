#![cfg(test)]

pub(crate) mod mock_protos;

use std::path::PathBuf;

use crate::config::Config;
use crate::config::InputType;
use crate::data::Database;
use crate::data::Dictionary;
use crate::Engine;

pub(crate) use mock_protos::*;

pub fn debug_db_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("debug")
        .join("khiin.db")
}

pub fn debug_db_filename() -> String {
    debug_db_path().into_os_string().into_string().unwrap()
}

pub fn get_engine() -> Option<Engine> {
    let filename = debug_db_filename();
    Engine::new(filename.as_str())
}

pub fn get_db() -> Database {
    let db_path = debug_db_path();
    Database::new(&db_path).unwrap()
}

pub fn get_dict() -> Dictionary {
    let db = get_db();
    Dictionary::new(&db, InputType::Numeric).expect("Could not load dictionary")
}

pub fn get_conf() -> Config {
    Config::new()
}