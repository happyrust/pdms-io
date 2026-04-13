use std::path::{Path, PathBuf};

use crate::core::{DbHandle, RefNo};
use anyhow::anyhow;
use parse_pdms_db::parse::parse_raw_ele_data;
use serde_json::json;
use serde_json::Value;

pub struct CoreDllOracle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrDiff {
    pub key: String,
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareReport {
    pub refno: String,
    pub rust_path: PathBuf,
    pub fixture_path: PathBuf,
    pub report_path: PathBuf,
    pub ignored_keys: Vec<String>,
    pub diff_count: usize,
    pub matches: bool,
    pub diffs: Vec<AttrDiff>,
}

impl CompareReport {
    pub fn to_json_value(&self) -> Value {
        json!({
            "refno": self.refno,
            "rustPath": self.rust_path.to_string_lossy().to_string(),
            "fixturePath": self.fixture_path.to_string_lossy().to_string(),
            "reportPath": self.report_path.to_string_lossy().to_string(),
            "ignoredKeys": self.ignored_keys,
            "diffCount": self.diff_count,
            "matches": self.matches,
            "diffs": self
                .diffs
                .iter()
                .map(|diff| {
                    json!({
                        "key": diff.key,
                        "left": diff.left,
                        "right": diff.right,
                    })
                })
                .collect::<Vec<_>>(),
        })
    }
}

impl CoreDllOracle {
    pub fn refno_to_string(refno: RefNo) -> String {
        format!("{}:{}", refno.hi(), refno.lo())
    }

    pub fn default_db_option_base(repo_root: impl AsRef<Path>) -> PathBuf {
        repo_root.as_ref().join("DbOption")
    }

    pub fn prepare_parse_environment(repo_root: impl AsRef<Path>) {
        let repo_root = repo_root.as_ref();
        let config_base = Self::default_db_option_base(repo_root);
        let source_toml = repo_root.join("DbOption.toml");
        let compat_toml = repo_root.join("db_options").join("DbOption.toml");
        if source_toml.exists() && !compat_toml.exists() {
            if let Some(parent) = compat_toml.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(&source_toml, &compat_toml);
        }
        // SAFETY: 测试/工具链在进程启动后单线程配置解析阶段调用，用于为 aios_core 指定配置文件。
        unsafe {
            let env_value = if source_toml.exists() {
                source_toml.to_string_lossy().to_string()
            } else {
                config_base.to_string_lossy().to_string()
            };
            std::env::set_var("DB_OPTION_FILE", env_value);
            std::env::set_current_dir(repo_root).ok();
        }
    }

    pub fn fixture_dir(repo_root: impl AsRef<Path>) -> PathBuf {
        repo_root.as_ref().join("test_output").join("core_dll")
    }

    pub fn rust_output_dir(repo_root: impl AsRef<Path>) -> PathBuf {
        repo_root.as_ref().join("test_output").join("rust_parse")
    }

    pub fn report_dir(repo_root: impl AsRef<Path>) -> PathBuf {
        repo_root.as_ref().join("test_output").join("compare_reports")
    }

    pub fn fixture_file_name(refno: &str) -> String {
        let sanitized = refno
            .trim()
            .chars()
            .map(|ch| match ch {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
                ':' | '/' | '\\' | '.' | ' ' => '_',
                _ => '_',
            })
            .collect::<String>();
        format!("{}.json", sanitized)
    }

    pub fn fixture_path(repo_root: impl AsRef<Path>, refno: &str) -> PathBuf {
        Self::fixture_dir(repo_root)
            .join(Self::fixture_file_name(refno))
    }

    pub fn rust_output_path(repo_root: impl AsRef<Path>, refno: &str) -> PathBuf {
        Self::rust_output_dir(repo_root)
            .join(Self::fixture_file_name(refno))
    }

    pub fn report_path(repo_root: impl AsRef<Path>, refno: &str) -> PathBuf {
        Self::report_dir(repo_root)
            .join(Self::fixture_file_name(refno))
    }

    pub fn read_json(path: impl AsRef<Path>) -> anyhow::Result<Value> {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn write_json(path: impl AsRef<Path>, value: &Value) -> anyhow::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(value)?)?;
        Ok(())
    }

    pub fn read_fixture(repo_root: impl AsRef<Path>, refno: &str) -> anyhow::Result<Value> {
        Self::read_json(Self::fixture_path(repo_root, refno))
    }

    pub fn write_fixture(
        repo_root: impl AsRef<Path>,
        refno: &str,
        value: &Value,
    ) -> anyhow::Result<PathBuf> {
        let path = Self::fixture_path(repo_root, refno);
        Self::write_json(&path, value)?;
        Ok(path)
    }

    pub fn write_rust_output(
        repo_root: impl AsRef<Path>,
        refno: &str,
        value: &Value,
    ) -> anyhow::Result<PathBuf> {
        let path = Self::rust_output_path(repo_root, refno);
        Self::write_json(&path, value)?;
        Ok(path)
    }

    pub fn normalize_attributes(
        value: &Value,
    ) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
        let attrs = value
            .get("attributes")
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow::anyhow!("JSON 缺少 attributes 对象"))?;

        let mut out = std::collections::BTreeMap::new();
        for (key, value) in attrs {
            out.insert(key.clone(), Self::normalize_value(value));
        }
        Ok(out)
    }

    pub fn diff_attributes(
        left: &Value,
        right: &Value,
        ignore_keys: &[&str],
    ) -> anyhow::Result<Vec<AttrDiff>> {
        let left = Self::normalize_attributes(left)?;
        let right = Self::normalize_attributes(right)?;
        let ignored: std::collections::BTreeSet<&str> = ignore_keys.iter().copied().collect();

        let mut keys = std::collections::BTreeSet::new();
        keys.extend(left.keys().cloned());
        keys.extend(right.keys().cloned());

        let mut diffs = Vec::new();
        for key in keys {
            if ignored.contains(key.as_str()) {
                continue;
            }
            let l = left.get(&key).cloned();
            let r = right.get(&key).cloned();
            if l != r {
                diffs.push(AttrDiff {
                    key,
                    left: l,
                    right: r,
                });
            }
        }
        Ok(diffs)
    }

    pub fn diff_fixture_attributes(
        repo_root: impl AsRef<Path>,
        refno: &str,
        rust_value: &Value,
        ignore_keys: &[&str],
    ) -> anyhow::Result<Vec<AttrDiff>> {
        let fixture = Self::read_fixture(repo_root, refno)?;
        Self::diff_attributes(rust_value, &fixture, ignore_keys)
    }

    pub fn compare_with_fixture(
        repo_root: impl AsRef<Path>,
        refno: &str,
        rust_value: &Value,
        ignore_keys: &[&str],
    ) -> anyhow::Result<CompareReport> {
        let repo_root = repo_root.as_ref();
        let rust_path = Self::write_rust_output(repo_root, refno, rust_value)?;
        let fixture_path = Self::fixture_path(repo_root, refno);
        let diffs = Self::diff_fixture_attributes(repo_root, refno, rust_value, ignore_keys)?;
        let report_path = Self::report_path(repo_root, refno);
        let report = CompareReport {
            refno: refno.trim().to_string(),
            rust_path,
            fixture_path,
            report_path: report_path.clone(),
            ignored_keys: ignore_keys.iter().map(|item| item.to_string()).collect(),
            diff_count: diffs.len(),
            matches: diffs.is_empty(),
            diffs,
        };
        Self::write_json(&report_path, &report.to_json_value())?;
        Ok(report)
    }

    pub fn build_rust_parse_json(handle: &DbHandle, refno: RefNo) -> anyhow::Result<Value> {
        let hit = handle
            .find_refno(refno, None)?
            .ok_or_else(|| anyhow!("找不到 refno {}", Self::refno_to_string(refno)))?;
        let record = handle.read_record(hit.loc)?;
        let ele_data = std::panic::catch_unwind(|| parse_raw_ele_data(&record))
            .map_err(|panic_payload| {
                let message = if let Some(text) = panic_payload.downcast_ref::<&str>() {
                    (*text).to_string()
                } else if let Some(text) = panic_payload.downcast_ref::<String>() {
                    text.clone()
                } else {
                    "parse_raw_ele_data panic".to_string()
                };
                anyhow!("Rust 解析配置未就绪或解析链 panic: {}", message)
            })??;
        let type_name = ele_data
            .whole_attmap
            .attmap
            .get_as_string("TYPE")
            .unwrap_or_else(|| ele_data.noun.to_string());

        let mut attributes = serde_json::Map::new();
        attributes.insert("REFNO".into(), Value::String(ele_data.refno.to_string()));
        attributes.insert("OWNER".into(), Value::String(ele_data.owner.to_string()));
        attributes.insert("NAME".into(), Value::String(ele_data.name.to_string()));
        attributes.insert("TYPE".into(), Value::String(type_name));
        attributes.insert(
            "CHILDREN".into(),
            Value::Array(
                ele_data
                    .children
                    .iter()
                    .map(|child| Value::String(child.to_string()))
                    .collect(),
            ),
        );

        for (key, value) in ele_data.whole_attmap.attmap.iter() {
            attributes.insert(key.to_string(), Value::String(format!("{:?}", value)));
        }
        for (key, value) in ele_data.whole_attmap.explicit_attmap.iter() {
            attributes.insert(key.to_string(), Value::String(format!("{:?}", value)));
        }

        Ok(json!({
            "refno": Self::refno_to_string(refno),
            "source": "rust_parse",
            "attributes": Value::Object(attributes),
            "meta": {
                "db_path": handle.path().to_string_lossy().to_string(),
                "sesno": hit.sesno,
                "page_no": hit.loc.page_no,
                "byte_offset": hit.loc.byte_offset,
                "ext_no": hit.loc.ext_no,
            }
        }))
    }

    pub fn compare_refno_with_fixture(
        repo_root: impl AsRef<Path>,
        handle: &DbHandle,
        refno: RefNo,
        ignore_keys: &[&str],
    ) -> anyhow::Result<CompareReport> {
        let rust_json = Self::build_rust_parse_json(handle, refno)?;
        Self::compare_with_fixture(repo_root, &Self::refno_to_string(refno), &rust_json, ignore_keys)
    }

    fn normalize_value(value: &Value) -> String {
        match value {
            Value::Null => String::new(),
            Value::Bool(v) => v.to_string(),
            Value::Number(v) => v.to_string(),
            Value::String(v) => v.trim().to_string(),
            Value::Array(values) => values
                .iter()
                .map(Self::normalize_value)
                .collect::<Vec<_>>()
                .join("|"),
            Value::Object(_) => value.to_string(),
        }
    }
}
