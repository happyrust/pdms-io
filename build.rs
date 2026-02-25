use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    // 设置 PROTOC 环境变量
    let protoc_path = format!("{}/protoc/bin/protoc.exe", env!("CARGO_MANIFEST_DIR"));
    let protoc_include = format!("{}/protoc/include", env!("CARGO_MANIFEST_DIR"));

    if Path::new(&protoc_path).exists() {
        println!("cargo:rustc-env=PROTOC={}", protoc_path);
        println!("cargo:rustc-env=PROTOC_INCLUDE={}", protoc_include);
    }

    // 重新构建时重新运行
    println!("cargo:rerun-if-changed=protoc");
}
