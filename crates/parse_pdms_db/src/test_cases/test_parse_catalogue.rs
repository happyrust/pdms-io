use std::path::PathBuf;

use crate::parse::parse_file;

/// 元件库（AvevaCatalogue）数据库文件解析冒烟测试。
///
/// 说明：
/// - 默认使用本机常见路径（见 DbOption.toml 的 project_path）。
/// - 为避免在无数据库文件的环境里失败，默认 `#[ignore]`，需要时手动执行：
///   `cargo test -p parse_pdms_db test_parse_aveva_catalogue_acp7000_0001_smoke -- --ignored`
#[tokio::test]
#[ignore]
async fn test_parse_aveva_catalogue_acp7000_0001_smoke() {
    // aios_core 在部分初始化流程中会用 `File::with_name("DbOption")` 从当前工作目录找配置；
    // 测试进程的 cwd 可能是 target 目录，故这里显式切到 workspace root（含 DbOption.toml）。
    struct DirGuard(std::path::PathBuf);
    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }
    let old_dir = std::env::current_dir().expect("read cwd");
    let _guard = DirGuard(old_dir);
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf();
    std::env::set_current_dir(&workspace_root).expect("chdir workspace root");

    let path = PathBuf::from("D:/AVEVA/Projects/E3D2.1/AvevaCatalogue/acp000/acp7000_0001");
    assert!(path.exists(), "数据库文件不存在：{}", path.display());

    // 项目名前缀用于 file_name 前缀解析 field_no
    let project = "acp7000";
    let file_name = "acp7000_0001";

    let result = parse_file(&path, &None, file_name, project).await;
    match result {
        Ok(db) => {
            assert!(!db.total_attr_map.is_empty(), "解析成功但未得到任何属性");
        }
        Err(e) => panic!("解析 {} 失败: {e:?}", path.display()),
    }
}
