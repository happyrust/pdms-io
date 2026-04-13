use anyhow::Result;
use pdmsdb_engine_v2::compare::core_dll_runtime::{CoreDllMode, CoreDllRuntime, CoreDllStatus};

#[test]
fn pe_machine_reader_handles_minimal_x86_pe() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("core_dll_runtime_fake_x86.dll");
    let _ = std::fs::remove_file(&file_path);

    let mut bytes = vec![0u8; 0x200];
    bytes[0x3C..0x40].copy_from_slice(&(0x80u32).to_le_bytes());
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    bytes[0x84..0x86].copy_from_slice(&(0x014Cu16).to_le_bytes());
    std::fs::write(&file_path, bytes)?;

    let machine = CoreDllRuntime::read_pe_machine(&file_path)?;
    assert_eq!(machine, 0x014C);

    let _ = std::fs::remove_file(&file_path);
    Ok(())
}

#[test]
fn preflight_reports_current_machine_constraints() -> Result<()> {
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let preflight = CoreDllRuntime::preflight(&repo_root, CoreDllRuntime::default_dll_path())?;

    assert!(preflight.dll_exists, "当前机器应存在 2.10 core.dll");
    assert_eq!(preflight.pe_machine, Some(0x014C));
    assert!(
        preflight.powershell32_exists,
        "当前机器应存在 32 位 PowerShell"
    );
    assert!(preflight.metadata_required_api_names.len() <= 4);
    assert!(preflight.export_count >= preflight.exported_db_function_names.len());
    match preflight.status {
        CoreDllStatus::Ready => {
            assert_eq!(preflight.recommended_mode, CoreDllMode::PowerShell32Helper);
            assert!(preflight.loadlibrary_ok);
            assert!(
                (preflight.db_functions_json_exists
                    && preflight.metadata_required_api_names.len()
                        == CoreDllRuntime::required_db_api_names().len())
                    || CoreDllRuntime::required_db_api_names()
                        .iter()
                        .all(|required| {
                            preflight
                                .exported_db_function_names
                                .iter()
                                .any(|name| name == required)
                        })
            );
        }
        CoreDllStatus::Blocked => {
            assert_eq!(preflight.recommended_mode, CoreDllMode::FixtureOnly);
            assert!(
                !preflight.loadlibrary_ok
                    || (!(preflight.db_functions_json_exists
                        && preflight.metadata_required_api_names.len()
                            == CoreDllRuntime::required_db_api_names().len())
                        && !CoreDllRuntime::required_db_api_names()
                            .iter()
                            .all(|required| {
                                preflight
                                    .exported_db_function_names
                                    .iter()
                                    .any(|name| name == required)
                            }))
            );
        }
        CoreDllStatus::MissingDll => panic!("当前机器应存在 2.10 core.dll"),
    }
    Ok(())
}

#[test]
fn read_db_functions_json_parses_required_symbols() -> Result<()> {
    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("core_dll_db_functions_test.json");
    let _ = std::fs::remove_file(&file_path);

    std::fs::write(
        &file_path,
        r#"{
  "functions": [
    {"name": "db5_open_read_db", "address": "0x105ee520"},
    {"name": "db5_close_db", "address": "0x105ee940"},
    {"name": "db4_get_ce_att", "address": "0x10621000"},
    {"name": "db4_get_att_dets", "address": "0x1061f2d0"}
  ]
}"#,
    )?;

    let parsed = CoreDllRuntime::read_db_functions_json(&file_path)?;
    assert_eq!(parsed.len(), 4);
    assert_eq!(parsed[0].name, "db5_open_read_db");

    let _ = std::fs::remove_file(&file_path);
    Ok(())
}
