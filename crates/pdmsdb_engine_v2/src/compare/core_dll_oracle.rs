use std::path::{Path, PathBuf};

use crate::core::{DbHandle, RefNo};
use crate::db4::{ElementRecordView, parse_explicit_blocks, parse_member_refs};
use crate::db4::explicit_attrs::read_explicit_string;
use anyhow::anyhow;
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
        let source_toml = repo_root.join("DbOption.toml");
        let compat_toml = repo_root.join("db_options").join("DbOption.toml");
        if source_toml.exists() && !compat_toml.exists() {
            if let Some(parent) = compat_toml.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(&source_toml, &compat_toml);
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
        let view = ElementRecordView::from_raw(&record)?;

        let mut attributes = serde_json::Map::new();
        attributes.insert(
            "REFNO".into(),
            Value::String(format!("{}:{}", view.refno.hi(), view.refno.lo())),
        );
        attributes.insert(
            "OWNER".into(),
            Value::String(format!("{}:{}", view.owner.hi(), view.owner.lo())),
        );
        attributes.insert(
            "NOUN_HASH".into(),
            Value::String(format!("0x{:08X}", view.noun_hash)),
        );
        attributes.insert(
            "IMPL_LEN".into(),
            Value::Number(view.impl_len_words.into()),
        );

        let children = parse_member_refs(&view.members_data);
        attributes.insert(
            "CHILDREN".into(),
            Value::Array(
                children
                    .iter()
                    .map(|c| Value::String(format!("{}:{}", c.hi(), c.lo())))
                    .collect(),
            ),
        );

        if !view.explicit_data.is_empty() {
            if let Ok(blocks) = parse_explicit_blocks(&view.explicit_data) {
                for (i, block) in blocks.iter().enumerate() {
                    let key = format!("EXPLICIT_{}_0x{:08X}", i, block.hash);
                    let value = if !block.payload.is_empty()
                        && block.payload.len() >= 4
                        && u32::from_be_bytes(
                            block.payload[0..4].try_into().unwrap_or([0; 4]),
                        ) > 0
                    {
                        read_explicit_string(&block.payload)
                    } else {
                        format!("{}B payload", block.payload.len())
                    };
                    attributes.insert(key, Value::String(value));
                }
            }
        }

        Ok(json!({
            "refno": Self::refno_to_string(refno),
            "source": "engine_v2",
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
