# 阻塞项：E3D 3.1 Core IO 重写

## 开放问题

- 哪些 E3D 3.1 样本数据库文件应作为 header/session/index/record 验证的 fixture 证据？
- 恢复出的函数名是否应写回 active IDA 数据库，还是在审阅前只保留为外部 JSON？
- 第一个只读 Rust 设计必须覆盖哪些字段：header/session/index/data page/record segment/element record 全部覆盖，还是先覆盖更小子集？

## 停下并询问

- 如果 active IDA 实例不再是 `D:\AVEVA\Everything3D3.1\core.dll.i64`，停下并询问。
- 如果证据指向的 E3D/core.dll 版本不是 3.1，停下并询问。
- 在执行批量 IDA 重命名、类型声明或 stack-frame 修改前，停下并询问。
- 在仓库或已批准 goal 包之外创建/编辑文件前，停下并询问。
- 在把范围从只读结构恢复改成写回实现前，停下并询问。
- 如果结构布局依赖无法由 IDA 证据或 fixture 字节支撑的猜测，停下并询问。

## 危险或高风险操作

- 批量 IDA 重命名或类型套用。
- 原地修改样本数据库文件。
- 添加实时 `core.dll` FFI 调用。
- 创建新的外部 workspace 或复制大型二进制 fixture。
- 在结构恢复批准前实现写回或 session commit。

## 已知阻塞

- 当前 IDA 类型表似乎没有现成 E3D 领域结构体。下一步需要从函数、栈/成员访问、常量和 fixture 字节中恢复结构。
- 当前仓库的 `ida_exports/db_functions.json` 来自早期假设；在重新映射前不能当作 E3D 3.1 真值。
