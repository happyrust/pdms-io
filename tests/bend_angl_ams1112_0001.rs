use aios_core::RefU64;
use parse_pdms_db::parse::parse_ele_data;
use pdms_io::io::PdmsIO;
use pdms_io::test::resolve_test_db_path;
use std::path::{Path, PathBuf};

#[tokio::test]
async fn test_ams1112_0001_bend_17496_171138_angl() -> anyhow::Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let db_option_file = manifest_dir.join("DbOption.toml");
    if db_option_file.exists() {
        unsafe {
            std::env::set_var("DB_OPTION_FILE", &db_option_file);
        }
    }

    let db_path = match resolve_test_db_path("ams1112_0001") {
        Some(path) => path,
        None => {
            eprintln!("数据库文件不存在，跳过: ams1112_0001");
            return Ok(());
        }
    };
    if !Path::new(&db_path).exists() {
        eprintln!("数据库文件不存在，跳过: {}", db_path.display());
        return Ok(());
    }

    let refno: RefU64 = "17496/171138".into();
    let mut io = PdmsIO::new("ams", &db_path, true);
    io.open()?;

    let (sesno, offset) = io
        .search_latest_refno(refno, None)
        .ok_or_else(|| anyhow::anyhow!("找不到 refno: {}", refno))?;
    let record = io.read_element_record_cached(offset)?;

    let mut input = record.as_slice();
    while input.len() >= 4 && (input[..4] == [0, 0, 0, 0] || input[..4] == [0, 0, 0, 7]) {
        input = &input[4..];
    }

    let ele_data = parse_ele_data(input).await?;
    let merged = ele_data.whole_attmap.merge();
    let element_type = merged.get_type();
    let angl = merged.get_f64("ANGL").unwrap_or_default();
    let angl_str = merged.get_as_string("ANGL").unwrap_or_default();

    println!("RefNo: {}", refno);
    println!("Sesno: {}", sesno);
    println!("Offset: {:#X}", offset);
    println!("TYPE: {}", element_type);
    println!("ANGL(str): {}", angl_str);
    println!("ANGL(f64): {}", angl);

    assert_eq!(element_type, "BEND", "目标元素类型应为 BEND");
    assert!(
        (angl - 14.976).abs() < 1e-3,
        "17496/171138 的 BEND ANGL 应约为 14.976°，实际为 {}",
        angl
    );

    Ok(())
}
