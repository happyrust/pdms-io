use anyhow::Result;
use pdmsdb_engine_v2::compare::core_dll_oracle::CoreDllOracle;
use serde_json::json;

#[test]
fn core_dll_oracle_fixture_diff() -> Result<()> {
    let repo_root = std::env::temp_dir().join("core_dll_fixture_diff_repo");
    std::fs::create_dir_all(&repo_root)?;

    let left = json!({
        "refno": "1:2",
        "attributes": {
            "NAME": "PIPE-1",
            "TYPE": "PIPE",
            "PGNO": "100"
        }
    });
    let right = json!({
        "refno": "1:2",
        "attributes": {
            "NAME": "PIPE-1",
            "TYPE": "PIPE",
            "PGNO": "200"
        }
    });

    let fixture_path = CoreDllOracle::write_fixture(&repo_root, "1:2", &right)?;
    assert_eq!(
        fixture_path,
        repo_root.join("test_output").join("core_dll").join("1_2.json")
    );

    let left_json = left;
    let right_json = CoreDllOracle::read_fixture(&repo_root, "1:2")?;
    let diff = CoreDllOracle::diff_attributes(&left_json, &right_json, &["PGNO"])?;
    assert!(diff.is_empty());

    let diff_via_fixture = CoreDllOracle::diff_fixture_attributes(&repo_root, "1:2", &left_json, &["PGNO"])?;
    assert!(diff_via_fixture.is_empty());
    Ok(())
}

#[test]
fn core_dll_oracle_compare_workflow_writes_report() -> Result<()> {
    let repo_root = std::env::temp_dir().join("core_dll_compare_workflow_repo");
    std::fs::create_dir_all(&repo_root)?;

    let rust_json = json!({
        "refno": "13246/514326",
        "source": "rust_parse",
        "attributes": {
            "NAME": "PIPE-1",
            "TYPE": "PIPE",
            "PGNO": "100"
        }
    });
    let fixture_json = json!({
        "refno": "13246/514326",
        "source": "core_dll",
        "attributes": {
            "NAME": "PIPE-1",
            "TYPE": "PIPE",
            "PGNO": "200"
        }
    });

    CoreDllOracle::write_fixture(&repo_root, "13246/514326", &fixture_json)?;
    let report = CoreDllOracle::compare_with_fixture(
        &repo_root,
        "13246/514326",
        &rust_json,
        &["PGNO"],
    )?;

    assert!(report.matches);
    assert_eq!(report.diff_count, 0);
    assert_eq!(
        report.rust_path,
        repo_root.join("test_output").join("rust_parse").join("13246_514326.json")
    );
    assert_eq!(
        report.report_path,
        repo_root
            .join("test_output")
            .join("compare_reports")
            .join("13246_514326.json")
    );

    let report_json = CoreDllOracle::read_json(&report.report_path)?;
    assert_eq!(report_json["refno"].as_str(), Some("13246/514326"));
    assert_eq!(report_json["matches"].as_bool(), Some(true));
    assert_eq!(report_json["diffCount"].as_u64(), Some(0));
    Ok(())
}

#[test]
fn core_dll_oracle_fixture_file_name_sanitizes_refno() -> Result<()> {
    assert_eq!(CoreDllOracle::fixture_file_name("13246/514326"), "13246_514326.json");
    assert_eq!(CoreDllOracle::fixture_file_name(" 1:2 "), "1_2.json");
    Ok(())
}
