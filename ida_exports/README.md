# core.dll IDA 导出与 2.10 对齐说明

本目录用于存放从 **IDA Pro** 对 **Everything3D 2.10** 的 `core.dll` 导出的 JSON，供写回实现与 core.dll 回归测试使用。当 ida-pro-mcp 的 `ida://idb/entrypoints`、`ida://idb/types` 等资源因 API 兼容性报错时，可改用本目录下的 IDAPython 脚本导出等价信息。

## 1. 将 IDA 对齐到 2.10

1. 在 IDA Pro 中打开 **2.10** 的 core.dll：
   - 路径：`D:\AVEVA\Everything3D2.10\core.dll`
2. 等待自动分析完成（或运行一次 “Reanalyze program”）。
3. 若使用 **ida-pro-mcp** 验证：
   - 确保当前 IDB 即上述 2.10 文件；
   - 请求 `ida://idb/metadata`：应看到 path/sha256 对应 2.10；
   - 请求 `ida://idb/segments`：应看到 .text/.rdata/.data 等段；
   - `ida://idb/entrypoints`、`ida://idb/types` 若报错，则采用下方脚本导出。

## 2. 使用 IDAPython 脚本导出 JSON（兜底方案）

在 IDA 中：**File → Script file…** 选择仓库中的：

- **`scripts/export_core_dll_for_pdms.py`**

或于 IDAPython 控制台执行：

```python
exec(open(r"<repo_path>\ida_exports\scripts\export_core_dll_for_pdms.py").read())
```

脚本会在 **本目录**（即 `ida_exports/`）下生成：

| 文件 | 内容 |
|------|------|
| `exports.json` | 导出符号：名称、序号（若有）、地址 |
| `db_functions.json` | 名称含 `db1_`/`db2_`/`db3_`/`db4_`/`db5_` 的函数列表（地址、名称） |
| `structs.json` | 与索引/会话/页/claim/free 相关的结构体名及成员（可选） |

生成后可由 Rust/测试或文档引用，无需依赖 MCP 的 entrypoints/types 资源。

## 3. 版本与校验

- 导出前请确认 IDA 当前 IDB 为 **Everything3D 2.10** 的 core.dll。
- 若 2.10 与 3.x 的 core.dll 共存，注意不要误用 3.x 的 IDB 导出到本目录。
