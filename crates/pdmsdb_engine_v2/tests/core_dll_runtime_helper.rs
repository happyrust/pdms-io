use anyhow::Result;
use pdmsdb_engine_v2::compare::core_dll_runtime::CoreDllRuntime;

#[test]
fn smoke_helper_returns_structured_json() -> Result<()> {
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let dll_path = CoreDllRuntime::default_dll_path();
    if !dll_path.exists() {
        println!("core.dll 不存在，跳过 helper smoke");
        return Ok(());
    }

    let value = CoreDllRuntime::invoke_smoke_helper(&repo_root, &dll_path)?;
    assert_eq!(
        value["dllPath"].as_str(),
        Some(r"D:\AVEVA\Everything3D2.10\core.dll")
    );
    assert!(value.get("loadlibraryOk").is_some());
    assert!(value.get("functions").is_some());
    assert!(value["functions"].get("db5_open_read_db").is_some());
    Ok(())
}
