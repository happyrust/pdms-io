# Engine V2 Compare Fixture CLI

此工具用于在 **Engine V2** 上对单个 RefNo 执行如下流程：

1. 只读打开数据库
2. 按 RefNo 读取 record
3. 解析为 Rust 标准 JSON
4. 与 fixture JSON 做字段级 diff
5. 输出 compare report

## 默认样本

- 默认数据库：`ams1112_0001`
- 默认 RefNo：`17496:171138`
- 默认忽略键：`PGNO`

## 直接运行

```powershell
cargo --config "build.rustc-wrapper=''" run `
  --manifest-path crates/pdmsdb_engine_v2/Cargo.toml `
  --bin engine_v2_compare_fixture -- `
  --output-root .tmp_compare_cli `
  --seed-fixture
```

再次执行可做真实比较：

```powershell
cargo --config "build.rustc-wrapper=''" run `
  --manifest-path crates/pdmsdb_engine_v2/Cargo.toml `
  --bin engine_v2_compare_fixture -- `
  --output-root .tmp_compare_cli
```

## PowerShell 包装脚本

可直接使用：

```powershell
.\debug_scripts\core_dll\run_engine_v2_compare_fixture.ps1 -SeedFixture
.\debug_scripts\core_dll\run_engine_v2_compare_fixture.ps1
```

自定义参数示例：

```powershell
.\debug_scripts\core_dll\run_engine_v2_compare_fixture.ps1 `
  -DbPath "D:\AVEVA\Projects\E3D2.1\AvevaMarineSample\ams000\ams1112_0001" `
  -Refno "17496:171138" `
  -OutputRoot "D:\tmp\engine_v2_compare" `
  -Ignore "PGNO,SESNO"
```

## 输出目录

`--output-root` 下会生成：

- `test_output/core_dll/<refno>.json`
- `test_output/rust_parse/<refno>.json`
- `test_output/compare_reports/<refno>.json`

## 说明

- 当前机器若 `sccache` 端口被占用，必须保留 `--config "build.rustc-wrapper=''"`。
- 此 CLI 只使用 Rust 重写链与 fixture compare，不接 live `core.dll` 调用。
