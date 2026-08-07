# 验证：E3D 3.1 Multi-Extent & Extract

## 命令

| Command | Purpose | Expected pass condition | Evidence location |
| --- | --- | --- | --- |
| `python -m json.tool ida_exports/3.1/struct_layouts.json` | 验证新增 extent_link / file_chain 字段后的 JSON 合法性 | exit 0；含 `extent_link`、`file_chain` | `progress.jsonl` |
| `cargo test -p e3d-reader --test single_extent_regression` | 单 extent 路径无回归 | 现有 11+ 用例全部 pass | `progress.jsonl` |
| `cargo test -p e3d-reader --test multi_extent_addressing` | (extent_no, page_no) 寻址 | 所有用例 pass；跨 extent 引用解析正确 | `progress.jsonl` |
| `cargo test -p e3d-reader --test extract_visibility` | Extract 可见性合成 | 至少 1 个 fixture 上 oracle vs Rust 一致 | `progress.jsonl` |

## 人工检查

- 已取得真正的 multi-extent fixture 并在 progress 中记录来源。
- `descriptor.ext_size` / `descriptor.max_ext` 的语义有 IDA 证据（不再标"中"置信度）。
- Extract 链头 / 父 extract / 可见性优先级在 `docs/ida-3.1-structures.md` 新章节中有解释。
- 现有只读 / 写回路径在 multi-extent fixture 上无回归。
- 对 5 个 goal 文档跑 Plannotator gate。

## 证据规则

- multi-extent fixture 的物理 layout（每个 extent 的 page 数 / 文件名 / 链接关系）必须先记录到 `progress.jsonl`，再开始实现。
- Extract 可见性的每条"规则"都需要至少一个 oracle 对照用例做证据。
- 如某条规则无法通过 oracle 验证（例如 oracle 不支持），必须显式记录"无法验证 / 待后续 goal"。
