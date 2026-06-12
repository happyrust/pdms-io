#![feature(type_ascription)]
#![feature(slice_pattern)]
#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde;

extern crate core;

pub use parse::parse_pdms_dir;
pub use parse::{parse_db, parse_file};

pub mod consts;
pub mod error_types;
pub mod parse;
pub mod parse_explict_tools;
pub mod parser;
pub mod refno_index;
#[cfg(test)]
pub mod test_cases;

pub type BHashMap<K, V> = std::collections::HashMap<K, V>;
