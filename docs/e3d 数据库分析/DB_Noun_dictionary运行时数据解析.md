# DB_Noun::dictionary_ 运行时数据解析

> 分析工具：IDA Pro MCP + x64dbg MCP
> 分析目标：E3D 3.1 core.dll（运行时进程内存）
> 分析日期：2026-04-15

## 1. 概述

`DB_Noun::dictionary_` 是 E3D core.dll 中的全局静态 `std::map<int, DB_Noun const*>`，在运行时保存所有已知的元素类型（NOUN）注册信息。本文档记录了该数据结构在进程内存中的具体布局、解析方法和提取结果。

---

## 2. 定位 dictionary_

### 2.1 符号信息

```
符号（mangled）:
?dictionary_@DB_Noun@@1V?$map@HPBVDB_Noun@@U?$less@H@std@@V?$allocator@U?$pair@$$CBHPBVDB_Noun@@@std@@@3@@std@@A

符号（demangled）:
protected: static class std::map<int, class DB_Noun const *, struct std::less<int>,
    class std::allocator<struct std::pair<int const, class DB_Noun const *>>>
    DB_Noun::dictionary_
```

### 2.2 运行时地址

通过 x64dbg 解析符号获取：

```
x64dbg> DbgValFromString("core.?dictionary_@DB_Noun@@...")
→ 0x5ADD359C
```

core.dll 模块加载范围：`0x59A90000` ~ `0x5DBA0000`（约 65MB）。

---

## 3. MSVC std::map 内存布局

### 3.1 std::map 头部结构（8 字节）

MSVC 的 `std::map` 内部使用红黑树（`_Tree`），其头部结构为：

```
偏移  字段          大小   含义
+0    _Myhead       4B     红黑树 sentinel（哨兵）节点指针
+4    _Mysize       4B     map 中的元素数量（size_t）
```

实际读取结果：

```
地址 0x5ADD359C:
  [+0] _Myhead = 0x05D06410  (sentinel 节点地址)
  [+4] _Mysize = 0x0000078B  (1931 个条目)
```

### 3.2 红黑树节点布局（24 字节）

每个 `_Tree_node` 包含：

```
偏移  字段          大小   含义
+0    _Left         4B     左子节点指针（叶子指向 sentinel）
+4    _Parent       4B     父节点指针（根节点的 parent 指向 sentinel）
+8    _Right        4B     右子节点指针（叶子指向 sentinel）
+12   _Color        1B     颜色（0=红色, 1=黑色）
+13   _Isnil        1B     是否为 nil/sentinel 节点
+14   （padding）    2B     对齐填充
+16   key           4B     NOUN hash 值（int，由 db1_hash 计算）
+20   value         4B     DB_Noun const* 指针
```

### 3.3 sentinel（哨兵）节点

sentinel 节点位于 `0x05D06410`：

```
地址 0x05D06410:
  [+0] _Left   = 0x05D67C70  (最小节点 / begin iterator)
  [+4] _Parent = 0x05D51B70  (根节点)
  [+8] _Right  = 0x05DF5E70  (最大节点 / rbegin iterator)
```

### 3.4 根节点示例

根节点位于 `0x05D51B70`：

```
地址 0x05D51B70:
  [+0]  _Left   = 0x05D066D0
  [+4]  _Parent = 0x05D06410  (→ sentinel, 确认为根)
  [+8]  _Right  = 0x05D51B10
  [+12] _Color  = 0x01 (黑色，根节点必须为黑色)
  [+13] _Isnil  = 0x00 (非 nil)
  [+16] key     = 0x07AE30DE (noun_hash)
  [+20] value   = 0x05BE5110 (DB_Noun* 指针)
```

### 3.5 最小节点（begin）

最小节点位于 `0x05D67C70`（hash 最小的 NOUN）：

```
地址 0x05D67C70:
  [+0]  _Left   = 0x05D06410 (→ sentinel, 无更小的)
  [+4]  _Parent = 0x05DFEEF0
  [+8]  _Right  = 0x05D06410 (→ sentinel, 叶子节点)
  [+12] _Color  = 0x00 (红色)
  [+16] key     = 0x00081C2B (= db1_hash("ADFFG"))
  [+20] value   = 0x05D78510 (DB_Noun*)
```

---

## 4. 树遍历算法

### 4.1 中序遍历（获取所有条目，按 hash 升序）

```
function inorder_traverse(sentinel_addr):
    sentinel = read_node(sentinel_addr)
    root = sentinel._Parent
    
    result = []
    stack = []
    current = root
    
    while current != sentinel_addr or stack not empty:
        while current != sentinel_addr:
            stack.push(current)
            current = read_dword(current + 0)  // _Left
        
        current = stack.pop()
        key = read_dword(current + 16)    // noun_hash
        val = read_dword(current + 20)    // DB_Noun*
        result.append((key, val))
        
        current = read_dword(current + 8) // _Right
    
    return result
```

### 4.2 从 begin 线性遍历（通过 successor）

```
function next_node(node, sentinel):
    right = read_dword(node + 8)
    if right != sentinel:
        // 右子树的最左节点
        node = right
        while read_dword(node + 0) != sentinel:
            node = read_dword(node + 0)
        return node
    else:
        // 向上回溯直到从左子树返回
        parent = read_dword(node + 4)
        while node == read_dword(parent + 8):  // node == parent._Right
            node = parent
            parent = read_dword(node + 4)
        return parent
```

---

## 5. db1_hash 算法

所有 NOUN 名称和属性名称都通过 `db1_hash` 映射为 32 位无符号整数：

```python
def db1_hash(name: str) -> int:
    h = 0
    for ch in name.upper():
        h = (h * 26 + ord(ch) - 0x40) & 0xFFFFFFFF
    return h

def db1_dehash(h: int) -> str:
    chars = []
    while h > 0:
        r = h % 26
        h = h // 26
        if r == 0:
            r = 26
            h -= 1
        chars.append(chr(r + 0x40))
    return ''.join(reversed(chars))
```

验证示例：

| NOUN 名称 | Hash (hex) | Hash (dec) |
|-----------|-----------|------------|
| DB | 0x0000006A | 106 |
| BOX | 0x000006E6 | 1766 |
| TEE | 0x00003557 | 13655 |
| BRAN | 0x0000B900 | 47360 |
| ELBO | 0x0001773B | 96059 |
| EQUI | 0x00018657 | 99927 |
| PIPE | 0x000463E9 | 287721 |
| STRT | 0x00054F30 | 347952 |
| VALV | 0x0005EA62 | 387682 |
| ZONE | 0x0007221D | 467485 |

---

## 6. 提取结果

### 6.1 统计

| 指标 | 值 |
|------|-----|
| dictionary_ 地址 | 0x5ADD359C |
| sentinel 地址 | 0x05D06410 |
| 总条目数 | 1931 |
| NOUN 常量总数（IDA） | 1932 |
| 最小 hash | 0x0000006A (DB) |
| 最大 hash | 0x8CE60CE0 (UNKNOWN) |

### 6.2 输出文件

| 文件 | 格式 | 内容 |
|------|------|------|
| `all_noun_types.json` | JSON | 全部 1932 个 NOUN 名称列表 |
| `noun_hash_table.json` | JSON | 每个 NOUN 的名称、hash 值和全局变量地址 |
| `noun_dictionary_dump.bin` | 二进制 | 按 hash 升序排列的 NOUN 记录 |

### 6.3 二进制 dump 格式（noun_dictionary_dump.bin）

```
文件头:
  [0..4]  count (u32 little-endian) — 条目总数

每条记录:
  [+0..4]   noun_hash  (u32 LE)   — db1_hash(NOUN名称)
  [+4..8]   name_len   (u32 LE)   — 名称字节长度
  [+8..8+N] name_bytes (ASCII)    — NOUN 名称
  [padding]                       — 0 填充到 4 字节对齐

记录紧密排列，无额外间隔。
```

### 6.4 常见管道类 NOUN 速查

| NOUN | Hash | 说明 |
|------|------|------|
| PIPE | 0x000463E9 | 管道 |
| BRAN | 0x0000B900 | 管道分支 |
| STRT | 0x00054F30 | 直段 |
| ELBO | 0x0001773B | 弯头 |
| TEE | 0x00003557 | 三通 |
| VALV | 0x0005EA62 | 阀门 |
| REDU | 0x0004E181 | 异径管 |
| FLAN | 0x0001BBC8 | 法兰 |
| GASK | 0x0001E535 | 垫片 |
| NOZZ | 0x0003EB8A | 接管嘴 |
| EQUI | 0x00018657 | 设备 |
| SITE | 0x00053249 | 站点 |
| ZONE | 0x0007221D | 区域 |

---

## 7. NOUN_* 全局常量的布局

在 core.dll 的 .data 段中，`dictionary_` 之后紧跟大量 `NOUN_*` 全局常量指针：

```
0x5ADD359C: dictionary_ (8 bytes: _Myhead + _Mysize)
0x5ADD35A4: NOUN_DDAT   → DB_Noun* (运行时已填充)
0x5ADD35AC: NOUN_PFDEFI → DB_Noun*
0x5ADD35B4: NOUN_REGI   → DB_Noun*
0x5ADD35BC: NOUN_CONNCT → DB_Noun*
0x5ADD35C4: NOUN_ECOPC  → DB_Noun*
...
(共 1932 个 NOUN_* 全局指针)
```

这些全局指针在 IDA 静态分析中初始值为 `0xFFFFFFFF`，在运行时被填充为对应的 `DB_Noun*` 指针地址。

---

## 8. 解析工具链

| 步骤 | 工具 | 操作 |
|------|------|------|
| 1. 定位地址 | x64dbg MCP | `DbgValFromString` 解析 mangled 符号 |
| 2. 读取头部 | x64dbg MCP | `DbgValFromString` 读取 [addr] 解引用 |
| 3. 遍历树 | x64dbg MCP | `ReadDismAtAddress` 读取节点原始字节 |
| 4. 提取 NOUN 名称 | IDA Pro MCP | `py_eval` 遍历 `idautils.Names()` 过滤 `NOUN_*` |
| 5. 计算 hash | IDA Pro MCP | `py_eval` 执行 `db1_hash()` Python 实现 |
| 6. 导出数据 | IDA Pro MCP | `py_eval` 写入 JSON/BIN 文件 |
| 7. 交叉验证 | 两者结合 | IDA 名称 ↔ x64dbg 运行时值 对照 |
