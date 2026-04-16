param(
  [Parameter(Mandatory = $true)]
  [string]$DllPath,

  [Parameter(Mandatory = $true)]
  [string]$DbFunctionsPath
)

$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class K32 {
  [DllImport("kernel32.dll", CharSet=CharSet.Ansi, SetLastError=true)]
  public static extern IntPtr GetProcAddress(IntPtr hModule, string procName);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr LoadLibrary(string lpFileName);
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool FreeLibrary(IntPtr hModule);
  [DllImport("kernel32.dll")]
  public static extern uint GetLastError();
}
"@

$dbFunctions = Get-Content -Raw -Path $DbFunctionsPath | ConvertFrom-Json
$requiredNames = @("db5_open_read_db", "db5_close_db", "db4_get_ce_att", "db4_get_att_dets")

$module = [K32]::LoadLibrary($DllPath)
$loadOk = $module -ne [IntPtr]::Zero
$loadError = if ($loadOk) { $null } else { [K32]::GetLastError() }

$functions = @{}
foreach ($item in $dbFunctions.functions) {
  $name = [string]$item.name
  $metadataAddress = [string]$item.address
  $proc = if ($loadOk) { [K32]::GetProcAddress($module, $name) } else { [IntPtr]::Zero }
  $functions[$name] = @{
    metadataAddress = $metadataAddress
    exportedByName = ($proc -ne [IntPtr]::Zero)
  }
}

$status = if (-not $loadOk) { "blocked" } else { "ready" }
$result = @{
  dllPath = $DllPath
  dbFunctionsPath = $DbFunctionsPath
  loadlibraryOk = $loadOk
  loadlibraryError = $loadError
  status = $status
  requiredNames = $requiredNames
  functions = $functions
}

if ($loadOk) {
  [K32]::FreeLibrary($module) | Out-Null
}

$result | ConvertTo-Json -Depth 5 -Compress
