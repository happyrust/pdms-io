#![feature(array_methods)]
#![feature(type_ascription)]

#[allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_imports,
    unused,
    missing_docs,
    unused_results,
    unused_must_use
)]
#[macro_use]
extern crate nom;
#[macro_use]
extern crate serde;
extern crate clap;
use aios_core::options::DbOption;
use std::time::Instant;
const ATT_MDB: i32 = 0x8221C;
const ATT_DB: i32 = 0x81C2B;
type AiosDbError = Result<(), Box<dyn std::error::Error>>;

fn main() -> anyhow::Result<()> {
    use config::{Config, File};
    let s = Config::builder()
        .add_source(File::with_name("DbOption"))
        .build()?;
    let db_option: DbOption = s.try_deserialize().unwrap();
    dbg!(&db_option);
    let time = Instant::now();
    println!("初始化数据库时间: {} ms", time.elapsed().as_millis());
    return Ok(());
}
