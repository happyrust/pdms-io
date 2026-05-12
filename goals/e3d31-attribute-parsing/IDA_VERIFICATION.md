# Slice 1.5 — IDA 证据验证（attlib.dat vs DB-embedded UDA）

> 本文件是 Slice 1 清点产生的两个新阻塞的 IDA 验证产物，写于 2026-05-11。
> IDA 实例：`D:/AVEVA/Everything3D3.1/core.dll.i64`（PID 2356，port 13337）。

---

## 1. 阻塞 #1：UDA 字典在 DB 文件内 还是 在外部 attlib 文件内？

### IDA 证据

| 证据点 | 地址 | 结论 |
| --- | --- | --- |
| 字符串 `"attlib.dat"` | `0x5db03b4` | **0 个 xref** — 死代码，3.1 不再硬编码该文件名 |
| 字符串 `"ATTLIB Record Cache"` | `0x5d4a50c` | 由 `sub_53924AC` 引用，用于 MTR trace 打印属性记录缓存的状态 |
| 字符串 `"ATGTDF"` | `0x5db4660` | 由 `sub_55F53B8` 引用，作为 MTR trace 入口名 |
| 字符串 `"Unable to open the Attribute Data File - "` | `0x5db45cc` | 由 `sub_55F4290` 引用 — 这是**属性数据文件**的错误消息 |
| 字符串 `"attlib_all/GALPRF"` | `0x5d20f33` | 表名 / token 字典条目（属性表内的标签） |

### 关键函数：`sub_55F4290`（标记为 `ATTOPE`，属性打开）

```c
int sub_55F4290(int a1, int filename_ptr, int filename_len, _DWORD *err_flag) {
    // ...
    MTRENT("ATTOPE", 6u, "\n");
    // 全局句柄初始化
    dword_6E336C0 = 0;
    // ...
    // 用 FHFIND 打开文件，filename 从参数传入
    v19 = FHFIND(filename_ptr + 1, v13, "OLD, READ", 9, ..., &dword_6E336C0);
    if (success) {
        // 读取多个表：sub_55F4FFC (ATTR), sub_55F53B8 (ATGTDF), sub_55F594C (ATNAIN?)
        // ...同一个文件包含多个表
    } else {
        // 错误：Unable to open the Attribute Data File - <filename>
    }
}
```

### 关键函数：`sub_55F53B8`（解析 ATGTDF 表）

```c
int sub_55F53B8(...) {
    MTRENT("ATGTDF", 6u, "\n");
    v13 = 531442;      // 0x81BF2 = base-27 hash 下界 + 1
    v14 = 387951929;   // 0x171FAD39 = base-27 hash 上界
    // FHDBRN 从 dword_6E336C0 (ATTOPE 打开的句柄) 读页面
    v16 = FHDBRN(&dword_6E336C0, &v15, var80C, &dword_5DB4670);
    // 解析 3 字一组的 (hash, w1=type, kind)；kind==2 时按 w1==4 与否处理 array
    // ...
}
```

### 结论

- 3.1 的属性 schema（**包含 UDA 定义**）通过 **外部属性数据文件** 读取，文件名由调用方传入（不是硬编码 `attlib.dat`）。
- 该文件包含多张表：ATTR（属性记录）、ATNAIN（noun → attr 映射）、ATGTDF（属性定义）等。
- UDA 在 `core.dll` 的 C++ 类层级中被建模为 `DB_Attribute` 的一种（`DB_Attribute::isUDA()` / `DB_Attribute::findUda()`），但它的字典定义来源也是同一个属性数据文件。
- **plannotator gate 的决策"UDA 字典嵌入在数据库文件内"与 IDA 证据不符**——应修订为：**UDA 字典与系统属性字典共享同一个外部属性数据文件**。

→ 这一发现需要回到 plannotator gate 复审一次（修订 brief.md / blockers.md / plan.md 中的 UDA 字典位置假设）。

---

## 2. 阻塞 #2：6 个魔术常量的 IDA 证据

### `0x81BF1` / `0x81BF2` / `0x171FAD39`：base-27 PDMS 名字哈希

**IDA 函数**：`PDMS_Hash::String`（`0x588cb87` 区域）

```c
// PDMS_Hash::String — 把 hash 还原为字符串
if (v3 == 0) return "NULL";
if (v3 < 0x81BF2) return; // 早退
if (v3 > 0x171FAD39) {
    // 长名分支：(v3 - 2075961) & 0xFFFFFF, 取 6-bit
} else {
    // 标准分支：v14 = v3 - 531441; (= v3 - 0x81BF1)
    for (i = 0; i < a3; ++i) {
        v15 = v14 / 0x1B;
        v16 = (v14 ? v14 % 0x1B + 64 : 32);
        a2[i] = v16;
        v14 /= 0x1B;
    }
}
```

| 常量 | IDA 位置 | 含义 |
| --- | --- | --- |
| `0x81BF1` (=531441) | `0x588cb87`、`0x588c943`、`0x525e6b2` 等 3+ 处 | base-27 解码减去的偏移；hash = 0 之后的最小哈希值前一格 |
| `0x81BF2` (=531442) | 10+ 处（包括 `sub_55F53B8` 的 `v13`，地址 `0x55f5048` 区域） | hash 最小有效值；ATGTDF 解析时作为 hash 范围下界 |
| `0x171FAD39` (=387951929) | 10+ 处 | hash 最大有效值；ATGTDF 解析时作为 hash 范围上界 |

**结论**：现有 `e3d-attlib/src/hash.rs::db1_hash` / `db1_dehash` 算法与 IDA `PDMS_Hash::String` 完全一致。**实现正确，只是缺 IDA 引用注释。**

### `531442` 与 `387951929`：ATGTDF 解析中的 hash 范围

**IDA 函数**：`sub_55F53B8`（已在 §1 节展示）

直接出现在反编译代码中：
```c
v13 = 531442;
v14 = 387951929;
// ... if (v18 < v13 || v18 > v14) break; ...
```

`e3d-attlib/src/parser.rs::guess_atgtdf_start` 中的 `531_442 <= c[0] <= 387_951_929` 启发式与此完全对应。

### `PAGE_SIZE = 2048`

`e3d-attlib` 硬编码 `PAGE_SIZE = 2048`。

**IDA 证据**：page_size 由 descriptor 中的 word[0x34] 决定（已恢复，见 `docs/ida-3.1-structures.md` §1）：fixture `ams1112_0001` 中 word[0x34] = 512 words → 512 × 4 = 2048 bytes。**fixture 上巧合等于 2048**，但属性数据文件可能有自己的 page_size，需要在重写时改为从 descriptor / file_info 读取，而不是硬编码。

---

## 3. 解锁后的下一步

- `attlib.dat` vs DB UDA 阻塞 → **需要 plannotator gate 重新复审**，把"UDA 字典在 DB 文件内"修订为"UDA 字典在外部属性数据文件内，与系统属性字典共享同一文件"。
- 6 个魔术常量 → **解锁**：所有常量都有 IDA 证据，可在重写时附引用。
- 因此 Slice 2（系统属性类型表恢复）可在用户确认 gate 修订后启动；Slice 5（UDA 字典恢复）目标也要相应调整为"在属性数据文件内定位 UDA 区段"。

---

## 4. 给用户的请求

请确认是否同意以下事实修订：

1. **UDA 字典位置**：把"嵌入在数据库文件内"改为"嵌入在外部属性数据文件内（文件名由调用方传入，不是硬编码 `attlib.dat`）"。
2. **`e3d-attlib` 重写仍然有效**：现有实现的算法骨架（base-27 hash、3 字 tuple、ATTR/ATNAIN/ATGTDF 表）方向正确，重写主要是补 IDA 证据引用、消除硬编码 `PAGE_SIZE`、补完类型表覆盖、加测试。
3. 是否同意现在把上面 §1 §2 §3 同步回 plannotator gate 文档（brief.md / blockers.md / plan.md / verification.md），并跑一次 gate 复审？
