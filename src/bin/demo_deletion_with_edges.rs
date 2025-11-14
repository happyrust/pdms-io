use pdms_io::io::PdmsIO;
use std::path::PathBuf;

/// 占位示例：后续可以在这里实现带边关系的删除演示逻辑。
fn main() {
    // 目前仅作为占位，保证 `cargo check` / `cargo build` 可以通过。
    // 实际删除 + 边关系演示逻辑可以根据需求再补充。
    let _dummy_path = PathBuf::from("./dummy.ms");
    let _ = std::mem::size_of::<PdmsIO>();
}
