# 阻塞项：E3D 3.1 Multi-Extent & Extract

## 开放问题

- 现有 fixture `ams1112_0001` 是不是多 extent？需要先验证。
- Extract 与 multi-extent 是否拆为两个 goal（如果工作量过大）？
- Extract 链的"父 extract"是否需要在文件外的索引中查询？
- 跨 extent 的 RefNo 是否会冲突？dbno 编码是否承担消歧角色？

## 停下并询问

- 在拿到真正多 extent 的 fixture 之前，**不允许**写代码绕 multi-extent 路径。
- 如发现 Extract 涉及外部 dictionary file，停下并询问是否将其纳入本 goal。
- 跨 extent 引用解析时如遇 IDA 中尚未恢复的字段，停下并补结构恢复。
- 把 multi-extent 接入 `e3d31-writeback` 之前必须分开 goal 处理。

## 危险或高风险操作

- 在没有 oracle 验证的情况下"按 2.10 文档假设" multi-extent 行为。
- 修改单 extent 路径的 invariant 以适配 multi-extent（应当新增分支而不是替换）。
- 把 Extract 可见性结果缓存到全局状态。

## 已知阻塞

- 多 extent fixture 缺失（最优先解决）。
- Extract 字段的 IDA 证据尚未恢复。
- 现有 oracle goal 未就绪时，可见性合成无法做端到端验证。
