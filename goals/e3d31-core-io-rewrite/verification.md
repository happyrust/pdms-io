# 验证：E3D 3.1 Core IO 重写

## 命令

| Command | Purpose | Expected pass condition | Evidence location |
| --- | --- | --- | --- |
| `python -m json.tool ida_exports/3.1/db_functions.json` | 验证恢复出的 3.1 数据库函数映射 JSON | 命令退出码为 0，并能打印合法 JSON | `progress.jsonl` |
| `python -m json.tool ida_exports/3.1/struct_layouts.json` | 验证恢复出的 3.1 结构布局 JSON | 命令退出码为 0，并能打印合法 JSON | `progress.jsonl` |
| `rg "置信度|证据|偏移" docs/ida-3.1-structures.md` | 检查结构文档是否包含置信度、证据和偏移 | 输出能在真实结构章节中命中这些词 | `progress.jsonl` |
| `rg "E3D 3.1|只读|非目标" goals/e3d31-core-io-rewrite` | 检查已批准 goal 文档是否保留基线和范围 | 输出确认 E3D 3.1、只读范围和非目标已写明 | `progress.jsonl` |

## 人工检查

- 审阅 `docs/ida-3.1-structures.md`，确认每个读取路径结构都有字段偏移、字段含义、证据和置信度。
- 审阅 `ida_exports/3.1/db_functions.json`，确认候选函数在证据较弱时没有被描述成确定结论。
- 审阅 `ida_exports/3.1/struct_layouts.json`，确认未知字段被显式表示，而不是被猜测填掉。
- 确认架构仍然止步于只读 IO 核心，没有夹带写回、claim/release、refresh、compact 或实时 `core.dll` FFI。
- 对 `brief.md`、`plan.md`、`verification.md`、`blockers.md` 和 `goal-prompt.md` 运行 Plannotator gate；每个文档批准前不得继续执行。

## 证据规则

- 验证结果记录到 `progress.jsonl`。
- 可用时记录命令、状态、时间戳和产物路径。
- 除非测试覆盖了被声明的需求，否则不能只凭测试通过就认为需求完成。
- 如果 IDA 推导出的结构字段仍不确定，记录不确定性和下一步所需证据，不要把该需求标记完成。
