# E3D 3.1 属性解析 / UDA（草案）

> **状态**：DRAFT — 尚未经过 Plannotator gate。
> 本 goal 合并了"把当前 `e3d-reader/src/record/attrs.rs` 与 `e3d-attlib` 依赖拆出独立 goal"以及"完整 UDA 解析"两件事。

## 目标结果

在 `e3d-reader` 的原始 record 字节读取之上，建立 **完整 E3D 3.1 属性 schema 解析层**：能根据 `core.dll` 内的 attribute 表把 ElementRecordView 的 raw_bytes 解码为 `(attribute_id, attribute_value_typed)` 映射，并涵盖 UDA（User Defined Attribute）的命名空间、类型、可见性、默认值与继承规则。

## 背景

- 只读 goal 已显式把"完整属性类型解析"列为非目标。
- 但实际实现中已经引入 `src/record/attrs.rs` 与 `e3d-attlib` 依赖（来自兄弟 crate `D:/work/plant-code/e3d-attlib/`），属于事实越界，由本 goal 收编。
- IDA 已识别属性体系入口：`DBE_Value` 子类（`DBE_StringValue` / `DBE_PositionValue` / `DBE_OrientationValue` / `DBE_DirectionValue`）、`PDMS_Hash`、`noun_hash` 等。

## 约束

- 仅做解析；不修改 / 不计算派生属性。
- 类型系统的细分（标量 / 字符串 / position / orientation / direction / reference / UDA）必须有 IDA 证据。
- UDA 字典与系统属性字典需要分别建立，命名冲突要有明确策略。
- 不引入运行时 `core.dll` 调用（如需对照，走 `e3d31-coredll-ffi-oracle`）。

## 非目标

- 写入 attribute（→ `e3d31-writeback`）。
- 表达式 / 规则（rule）求值（独立后续 goal）。
- UI / 浏览工具。
- 属性级别的二进制兼容（只解析，不重新打包）。

## 已决策（2026-05-11 plannotator gate，含 Slice 1.5 IDA 修订）

- 现有 `D:/work/plant-code/e3d-attlib/` 的处置：**重写**；本 goal 的产物落到重写后的 `e3d-attlib` crate，不新建独立 `e3d-attrs`。
- UDA 字典位置（**2026-05-11 IDA 修订**）：**与系统属性字典共享同一个外部属性数据文件**；该文件名由调用方在 `ATTOPE`（`sub_55F4290`）时传入，不再硬编码为 `attlib.dat`（字符串 `attlib.dat` 在 3.1 binary 中 0 xref，属遗留死代码）。详见 `IDA_VERIFICATION.md` §1。
- `attribute_id` 暴露形式：**双向**：保留数值 ID，同时还原 `core.dll` 内的属性符号名（`PDMS_Hash::String` `0x588cb87` 即 base-27 dehash 算法，与现有 `db1_dehash` 一致），对外提供 `id <-> name` 查询 API。
- 现有 `e3d-attlib` 的算法骨架（base-27 hash、`(hash, type, kind)` 三字 tuple、ATTR / ATNAIN / ATGTDF 三张表）**方向正确**，重写聚焦在：补 IDA 证据引用、去掉硬编码 `PAGE_SIZE`（应从 descriptor 读）、补类型覆盖、加测试。

## 完成定义

- 全部系统标量 / 字符串 / position / orientation / direction / reference 类型可解析，至少 1 个 fixture 上所有元素的所有非 UDA 属性可被读取。
- 在 fixture 的外部属性数据文件中成功识别并解析 UDA 字典区段（与系统属性字典共享同一文件），至少 1 个 fixture 上的 UDA 属性可正确解码。
- `e3d-attlib` 重写完成；旧实现已彻底替换，每个偏移 / 常量 / 类型码都带 IDA 引用；`PAGE_SIZE` 不再硬编码。
- `id <-> name` 双向 API 在 fixture 上可还原 100% 的系统属性名与 ≥90% 的 UDA 属性名。
- `e3d-io` （原 `e3d-reader`）与重写后的 `e3d-attlib` 之间的依赖方向、re-export 策略有明确决策与文档。
