use anyhow::Result;
use pdmsdb_engine_v2::compare::core_dll_oracle::CoreDllOracle;
use pdmsdb_engine_v2::compare::legacy_oracle::LegacyOracle;
use pdmsdb_engine_v2::{EngineOptions, EngineV2, RefNo};
use serde_json::Value;

fn resolve_db() -> Option<std::path::PathBuf> {
    let repo_root = workspace_root();
    LegacyOracle::resolve_repo_test_db_path(repo_root, "ams1112_0001")
}

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn sample_refno() -> RefNo {
    RefNo::from_parts(17496, 171138)
}

#[test]
fn compare_refno_with_fixture_writes_rust_json_and_report() -> Result<()> {
    let db_path = match resolve_db() {
        Some(path) => path,
        None => {
            println!("数据库文件不存在，跳过测试: ams1112_0001");
            return Ok(());
        }
    };

    let workspace_root = workspace_root();
    CoreDllOracle::prepare_parse_environment(&workspace_root);

    let handle = EngineV2::open_read(&db_path, EngineOptions::default())?;
    let rust_json = match CoreDllOracle::build_rust_parse_json(&handle, sample_refno()) {
        Ok(value) => value,
        Err(err) if err.to_string().contains("db_options/DbOption") => {
            println!("Rust 解析配置缺失，跳过 compare fixture workflow: {}", err);
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    assert_eq!(rust_json["source"].as_str(), Some("rust_parse"));
    assert_eq!(
        rust_json["refno"].as_str(),
        Some(CoreDllOracle::refno_to_string(sample_refno()).as_str())
    );
    assert!(rust_json["attributes"].is_object());

    let repo_root = std::env::temp_dir().join("engine_v2_compare_fixture_workflow");
    std::fs::create_dir_all(&repo_root)?;

    let mut fixture_json = rust_json.clone();
    if let Some(attributes) = fixture_json
        .get_mut("attributes")
        .and_then(Value::as_object_mut)
    {
        attributes.insert("PGNO".into(), Value::String("9999".into()));
    }
    CoreDllOracle::write_fixture(
        &repo_root,
        &CoreDllOracle::refno_to_string(sample_refno()),
        &fixture_json,
    )?;

    let report =
        CoreDllOracle::compare_refno_with_fixture(&repo_root, &handle, sample_refno(), &["PGNO"])?;
    assert!(report.matches);
    assert_eq!(report.diff_count, 0);
    assert!(report.rust_path.exists());
    assert!(report.report_path.exists());
    Ok(())
}
