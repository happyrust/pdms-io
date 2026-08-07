# 阻塞项：E3D 3.1 写回支持

## 已决策

- **crate 命名 / 布局**：把现有 `e3d-reader` 重命名为 **`e3d-io`**，读 / 写路径共存于同一 crate。重命名工作纳入本 goal Slice 1 之前的预备步骤；外部引用统一指向 `e3d-io`，旧 `e3d-reader` 名称在 2026-05 之后停止接收新代码（plannotator gate 决定，2026-05-11）。

## 开放问题

- 是否允许里程碑 0 暂时只支持 in-place update（不触发 B+tree split）？
- session commit 失败的恢复策略：rollback 自动重写 / fail-fast 留待人工？
- 写入是否需要全程持锁，还是允许多线程 claim 不同元素？

## 停下并询问

- 在 `e3d31-coredll-ffi-oracle` goal 完成之前不得开始 round-trip 验证。
- 任何"绕过 oracle 自我校验"的做法都必须先 review。
- 在 B+tree split / merge 实现前，每次都必须先在 oracle 上确认一个最小 split 用例的字节级行为。
- 把写入路径扩展到 multi-extent 之前，停下并跨向 `e3d31-multi-extent` goal。
- 改 fixture 原文件之前 100% 停下；只能在副本上做实验。

## 危险或高风险操作

- 直接覆盖 fixture 原文件。
- 在 IDA 中批量重命名写入侧函数。
- 把 oracle 调用注入到 e3d-writer 的生产路径（必须仅限测试）。
- 在 session commit 失败时继续做后续写入。
- 不经审阅就跨 extent 写入。

## 已知阻塞

- 写入侧的 `FHSPLT`、`FHDBWN`、`db_save_work` 内部调用图尚未完整恢复。
- 全局变量 `dword_6A54024`（导航栈）在 claim/release 时的修改语义未恢复。
- session journal 的字段语义未恢复。
- B+tree leaf split 时索引中间节点更新规则未恢复。
