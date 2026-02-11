# core.dll 回归 Harness 规划

本文档定义用于“写回后由 core.dll 读回并对比”的**最小可调用 API**、**JSON 输出格式**及 **Rust 侧对比策略**。目标：Rust 写回新 Element → 用 2.10 core.dll 按 RefNo 读出属性 → 输出 JSON → 与 Rust 解析（PdmsIO → EleData → JSON）做字段级 diff。

---

## 1. 最小可调用 API（2.10 core.dll）

基于现有逆向文档（[core_dll_数据库读写函数.md](e3d%20数据库分析/core_dll_数据库读写函数.md)、[db2_db5_驱动层分析总结.md](e3d%20数据库分析/db2_db5_驱动层分析总结.md)），最小调用链建议如下。**地址以 2.10 为准**（若使用 ida_exports 脚本，请对 2.10 IDB 导出后取址）。

| 阶段 | 函数名 | 用途 |
|------|--------|------|
| 初始化 | `db5_init` | 第 5 层初始化，设置用户/上下文 |
| 打开 | `db5_open_read_db` | 只读打开数据库（路径/句柄由调用方传入） |
| 定位 | db3 表搜索 + db4 设 CE | 按 RefNo 在索引中查找，将“当前元素”设为该 RefNo（具体为 `db3_start_table_search` / `db3_get_next_table_entry` 或等价接口，再 `db4_set_ce` 或由 db5 封装的一次“按 RefNo 定位”） |
| 读属性 | `db4_get_ce_att` | 获取当前元素(CE)的指定属性值 |
| 读属性元信息 | `db4_get_att_dets` | 可选，用于得知类型/长度便于序列化为 JSON |
| 关闭 | `db5_close_db` | 关闭数据库，释放资源 |

**说明**：

- 若 2.10 提供“按 RefNo 打开元素”的高层封装（如 db5 层一次调用即设 CE），可优先用该封装，减少对 db3/db4 的依赖。
- 调用约定：stdcall/cdecl 需与 2.10 导出一致（通常 32 位 DLL 为 stdcall）；参数类型/顺序需通过 IDA 或 ida_exports 导出确认。
- **降级方案**：若直接调用 core.dll 因初始化/授权/依赖失败，可采用 E3D 自带可执行或 PML 脚本间接调用（例如“按 RefNo 打印属性”），再将输出解析为同一 JSON 格式参与对比。

---

## 2. JSON 输出格式（“按 RefNo 的元素属性”）

Harness 输出建议为**单元素属性表**，便于与 Rust 解析的 `EleData` 转 JSON 做字段级对比。

### 2.1 建议结构

```json
{
  "refno": "12345:67890",
  "source": "core_dll",
  "attributes": {
    "NAME": "SOME-EQUIP",
    "TYPE": "EQUI",
    "OWNER": "12345:67889",
    "REFNO": "12345:67890",
    "DBREF": "...",
    "CHILDREN": ["12345:67891", "12345:67892"]
  },
  "meta": {
    "dll_version": "2.10",
    "db_path": "..."
  }
}
```

- **refno**：当前元素 RefNo（字符串，便于与 Rust 一致）。
- **source**：固定 `"core_dll"`，与 Rust 侧 `"rust_parse"` 区分。
- **attributes**：属性名 → 值的平面表；值为字符串或字符串数组（如 CHILDREN）；数值/布尔可转为字符串以简化 diff。
- **meta**：可选，版本与 DB 路径，不参与语义对比。

### 2.2 Rust 解析侧对应格式

Rust 将 `EleData` 序列化为同结构，仅 `source: "rust_parse"`，例如：

```json
{
  "refno": "12345:67890",
  "source": "rust_parse",
  "attributes": {
    "NAME": "SOME-EQUIP",
    "TYPE": "EQUI",
    "OWNER": "12345:67889",
    "REFNO": "12345:67890",
    "CHILDREN": ["12345:67891", "12345:67892"]
  },
  "meta": { "db_path": "..." }
}
```

对比时只比较 `attributes` 和 `refno`；`meta` 可忽略或仅做记录。

---

## 3. Rust 侧对比策略

### 3.1 输入

- **rust_json**：Rust 解析（PdmsIO → 按 RefNo 读记录 → parse_pdms_db → EleData）后生成的 JSON 文件。
- **core_dll_json**：core.dll harness（或 PML/可执行间接）输出的 JSON 文件。

### 3.2 规则

1. **字段级 diff**：对 `attributes` 内每个键做值比较；键缺失或值不同则报 diff。
2. **忽略键**：以下键可选忽略，不参与对比（物理/会话相关）：
   - `PGNO`、`SESNO`、`DBREF`（若 Rust 未写）、`CREATED_SESSION`、`CHANGED_SESSION` 等与存储位置/会话号相关的字段。
3. **归一化**：RefNo/NAME 等字符串做 trim、大小写按需归一化（若 E3D 与 Rust 对大小写一致则不做）；数值统一为字符串再比较。
4. **结果**：输出“一致”或列表：`{ "key": "attr_name", "rust": "...", "core_dll": "..." }`。

### 3.3 实现方式

- 可在 Rust 测试中：读取两份 JSON，反序列化为 `serde_json::Value`，比较 `attributes` 子对象（排除忽略键），断言无差异或输出 diff 列表。
- 或单独脚本（Python/Rust）读两文件并打印 diff，供人工确认。

---

## 4. 与 ida_exports 的配合

- 使用 **ida_exports/scripts/export_core_dll_for_pdms.py** 对 **2.10** core.dll 导出 `exports.json`、`db_functions.json` 后，可从 `db_functions.json` 取上述函数的**名称与地址**，用于生成 ffi 绑定或动态 GetProcAddress。
- 若 MCP `ida://idb/entrypoints` / `ida://idb/types` 可用，可直接从 MCP 取导出与类型信息；否则以 ida_exports 为唯一来源。
