# 阻塞项：E3D 3.1 属性解析 / UDA

## 已决策（2026-05-11 plannotator gate）

- 现有 `D:/work/plant-code/e3d-attlib/` 的处置：**重写**（不沿用旧实现的字段假设；新实现由 IDA 证据驱动）。
- UDA 字典位置（**2026-05-11 Slice 1.5 IDA 修订**）：**嵌入在外部属性数据文件内**，与系统属性字典共享同一文件；文件名由 `ATTOPE`（`sub_55F4290`）的调用方传入，3.1 binary 中字符串 `attlib.dat` 0 xref。
- `attribute_id` 暴露形式：**要可读符号名**（基于 IDA 中 `PDMS_Hash::String` `0x588cb87` 的 base-27 dehash，已与现有 `db1_dehash` 实现核对一致），提供 `id <-> name` 双向映射。
- 本 goal 的代码产物落地位置：**写入重写后的 `e3d-attlib` crate**（位于 `D:/work/plant-code/e3d-attlib/`，与 `e3d-io` 平级的兄弟仓库）。

## 已解决（2026-05-11 Slice 1.5 IDA 验证）

- 现有 `e3d-attlib` 中 6 个魔术常量的 IDA 证据：
  - `0x81BF1` / `0x81BF2` / `0x171FAD39`：`PDMS_Hash::String` (`0x588cb87`) 与多处范围检查
  - `531442` / `387951929`：`sub_55F53B8` 中直接出现作为 hash 范围下 / 上界
  - `PAGE_SIZE = 2048`：fixture 巧合，应从 descriptor word[0x34] × 4 派生，重写时去硬编码
- 现有 `e3d-attlib` 的算法骨架方向正确，重写聚焦于补证据引用、清除硬编码、补测试。

## 开放问题

（已无未决项；执行期如发现新阻塞需追加到此处。）

## 停下并询问

- 在清点现有 `e3d-attlib` + `attrs.rs` 之前，**不允许**开始任何重写工作。
- UDA 字典如果指向外部文件，停下并询问该文件的版本控制策略。
- 把属性解析层耦合进 `e3d-reader` 的公共 API 之前必须先 review crate 边界决策。
- 在 oracle goal 未就绪前不得标记"系统属性 100% 解析"已完成。

## 危险或高风险操作

- 直接删除 / 重命名现有 `e3d-attlib` 内容而不留迁移路径。
- 用未经 IDA 验证的 type_tag 解码字节。
- 把属性 schema 演进硬编码到 `e3d-reader::record::ElementRecordView`。
- 给 UDA 与系统属性使用同一 ID 空间。

## 已知阻塞

- `DBE_Value` 子类层级未完整恢复（vtable / RTTI 解读 pending）。
- UDA 字典位置 / 格式未确定。
- 现有 `attrs.rs` 与 `e3d-attlib` 的实际能力与覆盖范围未清点。
- oracle goal 完成之前，覆盖率数据缺乏对照参考。
