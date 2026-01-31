use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DblistDocument {
    pub source: String,
    pub elements: Vec<DblistElement>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DblistElement {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub attrs: Vec<DblistAttr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DblistAttr {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    Outside,
    InInput,
}

pub fn parse_dblist_file<P: AsRef<Path>>(path: P) -> Result<DblistDocument, String> {
    let path = path.as_ref();
    let content =
        fs::read_to_string(path).map_err(|e| format!("读取 DBLIST 失败: {}", e))?;
    let source = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    Ok(parse_dblist_text(&content, &source))
}

pub fn parse_dblist_text(text: &str, source: &str) -> DblistDocument {
    let content = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut state = ParseState::Outside;
    let mut elements: Vec<DblistElement> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut current: Option<DblistElement> = None;
    let mut continuation = false;

    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("--") {
            continue;
        }

        match state {
            ParseState::Outside => {
                if trimmed.starts_with("INPUT BEGIN") {
                    state = ParseState::InInput;
                }
                continue;
            }
            ParseState::InInput => {
                if trimmed.starts_with("INPUT END") || trimmed.starts_with("INPUT FINISH") {
                    if let Some(elem) = current.take() {
                        elements.push(elem);
                    }
                    break;
                }
                if trimmed == "END" {
                    if let Some(elem) = current.take() {
                        elements.push(elem);
                    } else {
                        // 真实 DBLIST 文件中可能出现连续的 END（如段落收尾 / 容错输出），
                        // 这类 END 不影响元素解析，直接忽略以避免误报。
                    }
                    continuation = false;
                    continue;
                }
                if trimmed.starts_with("NEW ") {
                    if let Some(elem) = current.take() {
                        elements.push(elem);
                    }
                    current = Some(parse_new_line(trimmed));
                    continuation = false;
                    continue;
                }

                if let Some(elem) = current.as_mut() {
                    if continuation {
                        let (value, cont) = strip_trailing_dollar(trimmed);
                        let normalized = normalize_value(&value);
                        if let Some(last) = elem.attrs.last_mut() {
                            if !normalized.is_empty() {
                                if !last.value.is_empty() {
                                    last.value.push(' ');
                                }
                                last.value.push_str(&normalized);
                            }
                        } else {
                            warnings.push(format!("第 {} 行: 续行无前置属性", idx + 1));
                        }
                        continuation = cont;
                        continue;
                    }

                    if let Some((key, rest)) = split_key_value(trimmed) {
                        let (value, cont) = strip_trailing_dollar(rest);
                        let normalized = normalize_value(&value);
                        elem.attrs.push(DblistAttr {
                            key: key.to_string(),
                            value: normalized,
                        });
                        continuation = cont;
                    } else {
                        elem.attrs.push(DblistAttr {
                            key: trimmed.to_string(),
                            value: String::new(),
                        });
                        continuation = false;
                    }
                } else {
                    warnings.push(format!("第 {} 行: INPUT 段落中出现孤立行", idx + 1));
                }
            }
        }
    }

    DblistDocument {
        source: source.to_string(),
        elements,
        warnings,
    }
}

fn parse_new_line(line: &str) -> DblistElement {
    let mut parts = line.split_whitespace();
    let _ = parts.next();
    let kind = parts.next().unwrap_or("").to_string();
    let name = parts.collect::<Vec<_>>().join(" ");
    let name = if name.is_empty() { None } else { Some(name) };
    DblistElement {
        kind,
        name,
        attrs: Vec::new(),
    }
}

fn split_key_value(line: &str) -> Option<(&str, &str)> {
    for (idx, ch) in line.char_indices() {
        if ch.is_whitespace() {
            let (key, rest) = line.split_at(idx);
            return Some((key, rest.trim()));
        }
    }
    None
}

fn strip_trailing_dollar(input: &str) -> (String, bool) {
    let trimmed = input.trim_end();
    if trimmed.ends_with('$') {
        let without = trimmed.trim_end_matches('$').trim_end();
        (without.to_string(), true)
    } else {
        (trimmed.to_string(), false)
    }
}

fn normalize_value(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dblist_ignores_stray_end() {
        let txt = r#"
-- comment
INPUT BEGIN
NEW BRANCH /TEST
FOO BAR
END
END
INPUT END
"#;

        let doc = parse_dblist_text(txt, "x.txt");
        assert!(doc.warnings.is_empty(), "warnings={:?}", doc.warnings);
        assert_eq!(doc.elements.len(), 1);
        assert_eq!(doc.elements[0].kind, "BRANCH");
        assert_eq!(doc.elements[0].name.as_deref(), Some("/TEST"));
        assert_eq!(doc.elements[0].attrs.len(), 1);
        assert_eq!(doc.elements[0].attrs[0].key, "FOO");
        assert_eq!(doc.elements[0].attrs[0].value, "BAR");
    }

    #[test]
    fn normalize_keeps_quoted_spaces() {
        let value = "'Rectangular Control Damper'";
        assert_eq!(normalize_value(value), "'Rectangular Control Damper'");
    }

    #[test]
    fn continuation_merges_lines() {
        let text = "INPUT BEGIN\nNEW TEST /X\nDESP A B $\n1 2 3 $\n4 5 6\nEND\nINPUT FINISH";
        let doc = parse_dblist_text(text, "sample");
        assert_eq!(doc.elements.len(), 1);
        let attrs = &doc.elements[0].attrs;
        assert_eq!(attrs.len(), 1);
        assert_eq!(attrs[0].key, "DESP");
        assert_eq!(attrs[0].value, "A B 1 2 3 4 5 6");
    }
}
