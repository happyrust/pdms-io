use anyhow::Result;
use pdmsdb_engine_v2::compare::core_dll_oracle::CoreDllOracle;
use serde_json::json;

#[test]
fn core_dll_diff_ignores_configured_keys() -> Result<()> {
    let left = json!({
        "attributes": {
            "NAME": "PIPE-1",
            "PGNO": "100",
            "SESNO": "101"
        }
    });
    let right = json!({
        "attributes": {
            "NAME": "PIPE-1",
            "PGNO": "200",
            "SESNO": "102"
        }
    });

    let diff = CoreDllOracle::diff_attributes(&left, &right, &["PGNO", "SESNO"])?;
    assert!(diff.is_empty());
    Ok(())
}

#[test]
fn core_dll_diff_reports_real_attribute_changes() -> Result<()> {
    let left = json!({
        "attributes": {
            "NAME": "PIPE-1",
            "TYPE": "PIPE"
        }
    });
    let right = json!({
        "attributes": {
            "NAME": "PIPE-2",
            "TYPE": "PIPE"
        }
    });

    let diff = CoreDllOracle::diff_attributes(&left, &right, &[])?;
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].key, "NAME");
    assert_eq!(diff[0].left.as_deref(), Some("PIPE-1"));
    assert_eq!(diff[0].right.as_deref(), Some("PIPE-2"));
    Ok(())
}
