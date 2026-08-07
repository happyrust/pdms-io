# 计划：E3D 3.1 Rust 只读 IO Crate

## 方案概览

在本仓库中创建一个新的 Rust crate `e3d_reader`，按照 `docs/ida-3.1-structures.md` 第 13 章的架构设计实现只读 IO 核心。工作按模块分层推进：先搭建 crate 骨架和基础类型，然后逐层实现页 I/O、元数据解析、B+tree 搜索、记录读取，最后整合为 `ReadOnlyEngine` 公共 API 并用 fixture 验证。

## 工作切片

| Slice | Purpose | Done when | Risks |
| --- | --- | --- | --- |
| 1 | Crate 骨架 + 基础类型 | `e3d_reader` crate 存在，RefNo/PageHeader/PageType/error 类型可编译 | Cargo workspace 配置问题 |
| 2 | Page I/O + Cache | 能按页号读取文件页面，LRU 缓存工作 | 页大小探测逻辑 |
| 3 | Meta 解析 | 能解析 page 0 (descriptor) 和 page 1 (file_info)，输出 DbDescriptor/FileInfo | 偏移可能与 fixture 不匹配 |
| 4 | Session 解析 | 能解析 session page (type 3)，回溯 session 链 | 3.1 session 结构可能有未知字段 |
| 5 | B+tree 索引搜索 | 能按 RefNo 搜索索引，返回 PageAddress | B+tree 节点内部布局尚不完整 |
| 6 | Record 读取 | 能读取元素记录槽位，构建 ElementRecordView | 跨页分段机制不确定 |
| 7 | Engine 集成 + Fixture 验证 | ReadOnlyEngine::open + find_element 端到端工作，fixture 验证通过 | 集成时发现前置模块问题 |

## 执行顺序

- Slice 1 必须最先执行（crate 基础设施）
- Slice 2-3 可并行（页 I/O 和元数据解析独立）
- Slice 4 依赖 Slice 2（需要读取页面）
- Slice 5 依赖 Slice 2（需要读取索引页面）
- Slice 6 依赖 Slice 2 和 5（需要页面和索引搜索结果）
- Slice 7 依赖所有前置 Slice

## 验收标准

- [ ] `e3d_reader` crate 存在且 `cargo check` 通过
- [ ] 能打开至少一个 E3D 3.1 fixture 数据库文件
- [ ] 能读取并打印 DbDescriptor 和 FileInfo
- [ ] 能回溯 session 链
- [ ] 能按已知 RefNo 定位元素（B+tree 搜索）
- [ ] 能读取定位到的元素的原始字节
- [ ] 所有结构偏移可追溯到 struct_layouts.json

## 方向控制

- 偏移和常量优先使用 struct_layouts.json 中的 3.1 值
- 遇到 fixture 不匹配时暂停并记录，不要猜测修复
- 旧代码只作为命名参考，不复制实现
