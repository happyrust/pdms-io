use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, anyhow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreDllMode {
    FixtureOnly,
    PowerShell32Helper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreDllStatus {
    Ready,
    Blocked,
    MissingDll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreDllPreflight {
    pub dll_path: PathBuf,
    pub dll_exists: bool,
    pub pe_machine: Option<u16>,
    pub exports_json_exists: bool,
    pub db_functions_json_exists: bool,
    pub metadata_required_api_names: Vec<String>,
    pub export_count: usize,
    pub exported_db_function_names: Vec<String>,
    pub powershell32_exists: bool,
    pub loadlibrary_ok: bool,
    pub loadlibrary_error: Option<u32>,
    pub status: CoreDllStatus,
    pub recommended_mode: CoreDllMode,
    pub notes: Vec<String>,
}

pub struct CoreDllRuntime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSymbol {
    pub name: String,
    pub ordinal: u32,
    pub rva: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbFunctionSymbol {
    pub name: String,
    pub address: String,
}

impl CoreDllRuntime {
    pub fn required_db_api_names() -> &'static [&'static str] {
        &[
            "db5_open_read_db",
            "db5_close_db",
            "db4_get_ce_att",
            "db4_get_att_dets",
        ]
    }

    pub fn default_dll_path() -> PathBuf {
        PathBuf::from(r"D:\AVEVA\Everything3D2.10\core.dll")
    }

    pub fn default_powershell32() -> PathBuf {
        PathBuf::from(r"C:\Windows\SysWOW64\WindowsPowerShell\v1.0\powershell.exe")
    }

    pub fn default_helper_script(repo_root: impl AsRef<Path>) -> PathBuf {
        repo_root
            .as_ref()
            .join("debug_scripts")
            .join("core_dll")
            .join("core_dll_smoke_helper.ps1")
    }

    pub fn preflight(
        repo_root: impl AsRef<Path>,
        dll_path: impl AsRef<Path>,
    ) -> Result<CoreDllPreflight> {
        let repo_root = repo_root.as_ref();
        let dll_path = dll_path.as_ref().to_path_buf();
        let exports_json_exists = repo_root.join("ida_exports").join("exports.json").exists();
        let db_functions_path = repo_root.join("ida_exports").join("db_functions.json");
        let db_functions_json_exists = db_functions_path.exists();
        let powershell32_exists = Self::default_powershell32().exists();
        let dll_exists = dll_path.exists();

        let mut notes = Vec::new();
        let metadata_db_functions = if db_functions_json_exists {
            Self::read_db_functions_json(&db_functions_path).unwrap_or_default()
        } else {
            Vec::new()
        };
        let metadata_required_api_names = metadata_db_functions
            .iter()
            .filter(|symbol| Self::required_db_api_names().contains(&symbol.name.as_str()))
            .map(|symbol| symbol.name.clone())
            .collect::<Vec<_>>();
        let exports = if dll_exists {
            Self::read_exports(&dll_path).unwrap_or_default()
        } else {
            Vec::new()
        };
        let exported_db_function_names = exports
            .iter()
            .filter(|symbol| symbol.name.starts_with("db"))
            .map(|symbol| symbol.name.clone())
            .collect::<Vec<_>>();
        let pe_machine = if dll_exists {
            Some(Self::read_pe_machine(&dll_path)?)
        } else {
            None
        };

        let (loadlibrary_ok, loadlibrary_error) = if dll_exists && powershell32_exists {
            Self::probe_loadlibrary(&dll_path)?
        } else {
            (false, None)
        };

        let direct_api_ready = Self::required_db_api_names()
            .iter()
            .all(|required| exports.iter().any(|symbol| symbol.name == *required));
        let metadata_ready = Self::required_db_api_names().iter().all(|required| {
            metadata_db_functions
                .iter()
                .any(|symbol| symbol.name == *required)
        });

        let status = if !dll_exists {
            notes.push("core.dll 不存在".into());
            CoreDllStatus::MissingDll
        } else if !powershell32_exists {
            notes.push("缺少 32 位 PowerShell helper".into());
            CoreDllStatus::Blocked
        } else if !loadlibrary_ok {
            notes.push(format!(
                "32 位 LoadLibrary 失败，错误码 {:?}",
                loadlibrary_error
            ));
            CoreDllStatus::Blocked
        } else if !(metadata_ready || direct_api_ready) {
            notes.push("缺少可调用的 db API 导出或 db_functions.json 地址元数据".into());
            CoreDllStatus::Blocked
        } else {
            CoreDllStatus::Ready
        };

        if pe_machine == Some(0x014c) {
            notes.push("core.dll 为 32 位 x86".into());
        }
        if !exports_json_exists || !db_functions_json_exists {
            notes.push("ida_exports 仍未导出 exports.json/db_functions.json".into());
        }
        if exported_db_function_names.is_empty() {
            notes.push("PE 导出表中未发现 db* 符号".into());
        }

        let recommended_mode = if status == CoreDllStatus::Ready {
            CoreDllMode::PowerShell32Helper
        } else {
            CoreDllMode::FixtureOnly
        };

        Ok(CoreDllPreflight {
            dll_path,
            dll_exists,
            pe_machine,
            exports_json_exists,
            db_functions_json_exists,
            metadata_required_api_names,
            export_count: exports.len(),
            exported_db_function_names,
            powershell32_exists,
            loadlibrary_ok,
            loadlibrary_error,
            status,
            recommended_mode,
            notes,
        })
    }

    pub fn read_pe_machine(path: impl AsRef<Path>) -> Result<u16> {
        let bytes = std::fs::read(path)?;
        if bytes.len() < 0x40 {
            return Err(anyhow!("PE 文件头长度不足"));
        }
        let pe_offset = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
        if pe_offset + 6 > bytes.len() {
            return Err(anyhow!("PE 偏移超出文件范围"));
        }
        if &bytes[pe_offset..pe_offset + 4] != b"PE\0\0" {
            return Err(anyhow!("无效的 PE 签名"));
        }
        Ok(u16::from_le_bytes(
            bytes[pe_offset + 4..pe_offset + 6].try_into().unwrap(),
        ))
    }

    pub fn invoke_smoke_helper(
        repo_root: impl AsRef<Path>,
        dll_path: impl AsRef<Path>,
    ) -> Result<serde_json::Value> {
        let repo_root = repo_root.as_ref();
        let script = Self::default_helper_script(repo_root);
        let db_functions = repo_root.join("ida_exports").join("db_functions.json");
        if !script.exists() {
            return Err(anyhow!("缺少 helper 脚本: {}", script.display()));
        }
        if !db_functions.exists() {
            return Err(anyhow!(
                "缺少 db_functions.json: {}",
                db_functions.display()
            ));
        }

        let output = Command::new(Self::default_powershell32())
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                script.to_string_lossy().as_ref(),
                "-DllPath",
                dll_path.as_ref().to_string_lossy().as_ref(),
                "-DbFunctionsPath",
                db_functions.to_string_lossy().as_ref(),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("helper 执行失败: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(serde_json::from_str(stdout.trim())?)
    }

    pub fn read_exports(path: impl AsRef<Path>) -> Result<Vec<ExportSymbol>> {
        let bytes = std::fs::read(path)?;
        if bytes.len() < 0x40 {
            return Err(anyhow!("PE 文件头长度不足"));
        }

        let pe_offset = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
        if pe_offset + 24 > bytes.len() {
            return Err(anyhow!("PE 头偏移超出范围"));
        }
        if &bytes[pe_offset..pe_offset + 4] != b"PE\0\0" {
            return Err(anyhow!("无效的 PE 签名"));
        }

        let number_of_sections =
            u16::from_le_bytes(bytes[pe_offset + 6..pe_offset + 8].try_into().unwrap()) as usize;
        let size_of_optional_header =
            u16::from_le_bytes(bytes[pe_offset + 20..pe_offset + 22].try_into().unwrap()) as usize;
        let optional_header = pe_offset + 24;
        if optional_header + size_of_optional_header > bytes.len() {
            return Err(anyhow!("可选头超出文件范围"));
        }

        let magic = u16::from_le_bytes(
            bytes[optional_header..optional_header + 2]
                .try_into()
                .unwrap(),
        );
        let data_dir_offset = match magic {
            0x10B => optional_header + 96,
            0x20B => optional_header + 112,
            other => return Err(anyhow!("未知 PE magic: 0x{:X}", other)),
        };
        if data_dir_offset + 8 > bytes.len() {
            return Err(anyhow!("导出目录表超出文件范围"));
        }

        let export_rva = u32::from_le_bytes(
            bytes[data_dir_offset..data_dir_offset + 4]
                .try_into()
                .unwrap(),
        );
        if export_rva == 0 {
            return Ok(Vec::new());
        }

        let section_table = optional_header + size_of_optional_header;
        let export_offset =
            Self::rva_to_file_offset(&bytes, section_table, number_of_sections, export_rva)
                .ok_or_else(|| anyhow!("无法将 export RVA 映射到文件偏移"))?;
        if export_offset + 40 > bytes.len() {
            return Err(anyhow!("导出目录结构超出文件范围"));
        }

        let base = u32::from_le_bytes(
            bytes[export_offset + 16..export_offset + 20]
                .try_into()
                .unwrap(),
        );
        let number_of_functions = u32::from_le_bytes(
            bytes[export_offset + 20..export_offset + 24]
                .try_into()
                .unwrap(),
        ) as usize;
        let number_of_names = u32::from_le_bytes(
            bytes[export_offset + 24..export_offset + 28]
                .try_into()
                .unwrap(),
        ) as usize;
        let functions_rva = u32::from_le_bytes(
            bytes[export_offset + 28..export_offset + 32]
                .try_into()
                .unwrap(),
        );
        let names_rva = u32::from_le_bytes(
            bytes[export_offset + 32..export_offset + 36]
                .try_into()
                .unwrap(),
        );
        let ordinals_rva = u32::from_le_bytes(
            bytes[export_offset + 36..export_offset + 40]
                .try_into()
                .unwrap(),
        );

        let functions_off =
            Self::rva_to_file_offset(&bytes, section_table, number_of_sections, functions_rva)
                .ok_or_else(|| anyhow!("无法映射 functions RVA"))?;
        let names_off =
            Self::rva_to_file_offset(&bytes, section_table, number_of_sections, names_rva)
                .ok_or_else(|| anyhow!("无法映射 names RVA"))?;
        let ordinals_off =
            Self::rva_to_file_offset(&bytes, section_table, number_of_sections, ordinals_rva)
                .ok_or_else(|| anyhow!("无法映射 ordinals RVA"))?;

        let mut exports = Vec::new();
        for idx in 0..number_of_names {
            let name_rva_off = names_off + idx * 4;
            let ordinal_off = ordinals_off + idx * 2;
            if name_rva_off + 4 > bytes.len() || ordinal_off + 2 > bytes.len() {
                break;
            }

            let name_rva =
                u32::from_le_bytes(bytes[name_rva_off..name_rva_off + 4].try_into().unwrap());
            let ordinal_index =
                u16::from_le_bytes(bytes[ordinal_off..ordinal_off + 2].try_into().unwrap())
                    as usize;
            if ordinal_index >= number_of_functions {
                continue;
            }

            let function_rva_off = functions_off + ordinal_index * 4;
            if function_rva_off + 4 > bytes.len() {
                continue;
            }
            let function_rva = u32::from_le_bytes(
                bytes[function_rva_off..function_rva_off + 4]
                    .try_into()
                    .unwrap(),
            );
            let name = Self::read_c_string(
                &bytes,
                Self::rva_to_file_offset(&bytes, section_table, number_of_sections, name_rva)
                    .ok_or_else(|| anyhow!("无法映射导出名称 RVA"))?,
            )?;
            exports.push(ExportSymbol {
                name,
                ordinal: base + ordinal_index as u32,
                rva: function_rva,
            });
        }

        Ok(exports)
    }

    pub fn read_db_functions_json(path: impl AsRef<Path>) -> Result<Vec<DbFunctionSymbol>> {
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        let functions = value
            .get("functions")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| anyhow!("db_functions.json 缺少 functions 数组"))?;

        let mut out = Vec::new();
        for item in functions {
            let name = item
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow!("db_functions.json 条目缺少 name"))?;
            let address = item
                .get("address")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| anyhow!("db_functions.json 条目缺少 address"))?;
            out.push(DbFunctionSymbol {
                name: name.to_string(),
                address: address.to_string(),
            });
        }
        Ok(out)
    }

    fn read_c_string(bytes: &[u8], offset: usize) -> Result<String> {
        if offset >= bytes.len() {
            return Err(anyhow!("字符串偏移超出范围"));
        }
        let end = bytes[offset..]
            .iter()
            .position(|&b| b == 0)
            .map(|pos| offset + pos)
            .ok_or_else(|| anyhow!("未找到字符串结尾"))?;
        Ok(String::from_utf8(bytes[offset..end].to_vec())?)
    }

    fn rva_to_file_offset(
        bytes: &[u8],
        section_table: usize,
        section_count: usize,
        rva: u32,
    ) -> Option<usize> {
        for index in 0..section_count {
            let section_off = section_table + index * 40;
            if section_off + 40 > bytes.len() {
                return None;
            }

            let virtual_size =
                u32::from_le_bytes(bytes[section_off + 8..section_off + 12].try_into().ok()?);
            let virtual_address =
                u32::from_le_bytes(bytes[section_off + 12..section_off + 16].try_into().ok()?);
            let raw_size =
                u32::from_le_bytes(bytes[section_off + 16..section_off + 20].try_into().ok()?);
            let raw_ptr =
                u32::from_le_bytes(bytes[section_off + 20..section_off + 24].try_into().ok()?);

            let section_size = virtual_size.max(raw_size);
            if rva >= virtual_address && rva < virtual_address + section_size {
                let delta = rva - virtual_address;
                return Some(raw_ptr as usize + delta as usize);
            }
        }
        None
    }

    fn probe_loadlibrary(path: &Path) -> Result<(bool, Option<u32>)> {
        let script = format!(
            "@'\nAdd-Type @\"\nusing System;\nusing System.Runtime.InteropServices;\npublic static class K32 {{\n  [DllImport(\"kernel32.dll\", CharSet=CharSet.Unicode, SetLastError=true)]\n  public static extern IntPtr LoadLibrary(string lpFileName);\n  [DllImport(\"kernel32.dll\", SetLastError=true)]\n  public static extern bool FreeLibrary(IntPtr hModule);\n  [DllImport(\"kernel32.dll\")]\n  public static extern uint GetLastError();\n}}\n\"@\n$dll = '{}'\n$h = [K32]::LoadLibrary($dll)\nif ($h -eq [IntPtr]::Zero) {{ Write-Output ('load_failed:' + [K32]::GetLastError()) }} else {{ Write-Output 'load_ok'; [K32]::FreeLibrary($h) | Out-Null }}\n'@",
            path.display().to_string().replace('\'', "''")
        );

        let output = Command::new(Self::default_powershell32())
            .args(["-NoProfile", "-Command", &script])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let text = format!("{}{}", stdout, stderr);

        if text.contains("load_ok") {
            return Ok((true, None));
        }

        let error = text
            .split("load_failed:")
            .nth(1)
            .and_then(|s| s.lines().next())
            .and_then(|s| s.trim().parse::<u32>().ok());
        Ok((false, error))
    }
}
