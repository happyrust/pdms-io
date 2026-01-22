param(
  [string]$AmsFile = $env:PDMS_AMS_FILE
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($AmsFile)) {
  throw "请传入 -AmsFile 或设置环境变量 PDMS_AMS_FILE"
}

$env:PDMS_AMS_FILE = $AmsFile

cargo test --lib test::expression_test_utils::tests::dump_phei_fixture -- --ignored --nocapture

$fixtureRel = "test_output/phei_13246_514326.element.bin"
if (-not (Test-Path $fixtureRel)) {
  throw "未找到夹具文件: $fixtureRel"
}

$env:PDMS_ELE_FIXTURE = (Resolve-Path $fixtureRel).Path

cargo test --lib test::expression_test_utils::tests::test_phei_expression -- --ignored --nocapture
