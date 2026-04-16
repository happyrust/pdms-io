param(
  [string]$DbPath = "",
  [string]$Refno = "17496:171138",
  [string]$OutputRoot = "",
  [string]$Ignore = "PGNO",
  [switch]$SeedFixture
)

$ErrorActionPreference = "Stop"

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$ManifestPath = Join-Path $RepoRoot "crates\pdmsdb_engine_v2\Cargo.toml"

if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
  $OutputRoot = Join-Path $RepoRoot "test_output\engine_v2_compare_cli"
}

$env:CARGO_TARGET_DIR = Join-Path $RepoRoot ".target_v2cli"

$args = @(
  "--config", "build.rustc-wrapper=''",
  "run",
  "--manifest-path", $ManifestPath,
  "--bin", "engine_v2_compare_fixture",
  "--",
  "--refno", $Refno,
  "--output-root", $OutputRoot,
  "--ignore", $Ignore
)

if (-not [string]::IsNullOrWhiteSpace($DbPath)) {
  $args += @("--db", $DbPath)
}

if ($SeedFixture) {
  $args += "--seed-fixture"
}

Write-Host "RepoRoot: $RepoRoot"
Write-Host "OutputRoot: $OutputRoot"
Write-Host "Refno: $Refno"
if (-not [string]::IsNullOrWhiteSpace($DbPath)) {
  Write-Host "DbPath: $DbPath"
}

Push-Location $RepoRoot
try {
  cargo @args
} finally {
  Pop-Location
}
