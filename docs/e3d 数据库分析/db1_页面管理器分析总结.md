# E3D/PDMS 数据库底层页面管理器 (db1) 分析报告

## 1. 概述

通过对 `core.dll` 的逆向分析，我们识别并确认了负责 PDMS/E3D 数据库文件操作的核心底层驱动层——**db1 页面管理器**。该模块负责直接与物理磁盘文件（.db, .prp 等）交互，并维护一个高效的内存页面缓存池。

> 本文档基于 **E3D 3.1** 版本 core.dll 更新（2026-04-09）。

## 2. 核心函数分析表

以下是已映射的函数列表，包含 E3D 2.x 和 3.1 两个版本的地址。

| 函数名称 (IDA Renamed) | E3D 2.x 地址 | E3D 3.1 地址 | 功能描述 | 逆向分析要点 |
| :--- | :--- | :--- | :--- | :--- |
| **`db1_init`** | `0x106339F0` | — | 页面管理器初始化 | 设置内存缓存池大小、哈希表及并发控制结构。 |
| **`db1_finish`** | `0x10631C70` | — | 页面管理器清理 | 强制刷入所有 Dirty 页面并释放资源。 |
| **`db1_read_page`** | `0x10630C00` | **`0x5AF0640`** | 磁盘读取（带重试） | 调用 `FHDBRN` → `DirectAccessToken` vtable，含文件锁重试（错误码 11 → SYWAIT → 模式切换 FHSWIT） |
| **`db1_write_page`** | `0x10633FB0` | **`0x5AF1C30`** | 磁盘写入 | 临时文件 vs 正式 DB 双路径；正式写入支持 32 页批量缓冲（`dword_6A54158`）后调 `sub_5AF2260` 落盘 |
| **`db1_get_page`** | `0x10634DC0` | **`0x5AEE4E0`** | 逻辑取页（核心调度） | 查缓存(`sub_5AF04A0`) → 未命中 → 分配槽位(`sub_5AEF150`) → 读磁盘 → 支持多页预读(`dword_6A5415C`) → 本地文件缓存(`sub_5BCC210`/`sub_5BCC240`) |
| **`db1_get_new_page`** | `0x10635B00` | — | 物理页分配 | 在文件尾部或空闲链表中分配新页，用于存储新元素。 |
| **`db1_update_page`** | `0x10635E30` | **`0x5AF1600`** | 事务状态更新 (COW) | 多用户模式验证 dbno → 临时页面分配+数据拷贝+交换 → 标记 dirty (0x4000) |
| **`db1_lock_page`** | `0x10631030` | **`0x5AEFC30`** | 内存锁定 | `flags[0:13]` lock_count++ (XOR 掩码 0x3FFF) |
| **`db1_unlock_page`** | `0x10630EF0` | **`0x5AF11D0`** | 内存解锁 | `flags[0:13]` lock_count-- ；若已为 0 则报错 656 |
| **`db1_is_page_incore`** | `0x10631270` | **`0x5AF04A0`** | 缓存命中检查 | 快速判断指定的 (dbno, pgno, extno) 是否在内存中。 |
| **`db1_plu_locate_entry`** | `0x10630580` | **`0x5AEF150`** | 空闲页槽位分配 | LRU 置换算法选择可用槽位。 |
| **`db1_lose_pages`** | `0x10631450` | **`0x5AEDA00`** | 缓存失效 | 清除内存中的缓存页，常用于切换库或处理同步冲突。 |

## 3. 页面描述符结构 (E3D 3.1)

缓存池 `dword_6A540EC` 中每个页面条目 **28 字节 (7 DWORD)**：

| 偏移 | DWORD 索引 | 内容 | 标志位说明 |
| :--- | :--- | :--- | :--- |
| +0 | [0] | 页面数据指针 | |
| +12 | [3] | 页面序列号 (page_id) | |
| +16 | [4] | extent/session 号 | |
| +20 | [5] | dbno (所属数据库号) | |
| +24 | [6] | 标志位 | bit 0-13: lock_count, bit 14 (0x4000): dirty, bit 15 (0x8000): prefetched, bit 16 (0x10000): referenced |

## 4. 关键全局变量 (E3D 3.1)

| 地址 | 含义 |
| :--- | :--- |
| `dword_6A540EC` | 页面描述符池基址 |
| `dword_6453DC4` | 页面大小（DWORD 数, ×4 = 字节数） |
| `dword_6453DC0` | 默认空页面缓冲 |
| `dword_6A54110` | 当前 dbno |
| `dword_6A54118` | 当前 extent 号 |
| `dword_6A54114` | session 页面序列计数器 |
| `dword_6A54140` | 临时页面序列计数器 |
| `dword_6A54120` | 数据库文件大小（页数） |
| `dword_6A54144` | 临时文件大小（页数） |
| `dword_6A54154` | 已写入页面计数 |
| `dword_6A5418C` | 读缓存表大小 |
| `dword_6A541A0` | 读缓存表 (token, page_no) |
| `dword_6A54158` | 批量写缓冲（32 页） |
| `dword_6A54160` | 批量写启用标志 |
| `dword_6A5415C` | 多页预读粒度 |
| `dword_6A540E8` | 文件 token |
| `dword_6A540F8` | 本地文件缓存页缓冲 |

## 5. 架构设计分析

### 5.1 页面调度流程
```
上层业务 → DB_Access → db1_get_page(0x5AEE4E0)
  → db1_is_page_incore(0x5AF04A0) 缓存命中?
    → 命中: 增加 lock_count, 返回数据指针
    → 未命中: db1_plu_locate_entry(0x5AEF150) 分配槽位
      → 尝试本地文件缓存 sub_5BCC210
      → db1_read_page(0x5AF0640) → FHDBRN → DirectAccessToken::read
      → 写入本地文件缓存 sub_5BCC240
      → 支持多页预读 (batch read dword_6A5415C 页)
```

### 5.2 写入流程
```
db1_update_page(0x5AF1600) 标记 dirty
  → db1_write_page(0x5AF1C30)
    → 临时页: 写入临时文件
    → 正式页: 32 页批量缓冲 → sub_5AF2260 → FHDBWN → DirectAccessToken::write
```

### 5.3 逆向建议
- **数据结构**: `db1_get_page` 参数: `(token[2], lock_flag, prefetch_count, out_data_ptr)`
- **页面大小**: `dword_6453DC4` × 4 字节（通常 512 或 2048）
- **同步机制**: `db1_update_page` 之后的 `db1_write_page` 调用时机分析事务提交点

## 6. 与 pdms-io-fork Rust 实现的对应

| core.dll (db1) | pdms-io-fork (Rust) |
| :--- | :--- |
| 页面描述符池 `dword_6A540EC` | `PageManager.cache: HashMap<(ext_no, page_no), CachedPage>` |
| `db1_lock_page` lock_count | `CachedPage.lock_count` |
| `db1_update_page` dirty 标记 | `CachedPage.is_dirty` |
| `db1_read_page` → FHDBRN | `PageManager::read_page_from_file` (seek + read) |
| `db1_write_page` → FHDBWN | `PageManager::write_page_to_file` (seek + write) |
| LRU 置换 `db1_plu_locate_entry` | `PageManager` 按 `last_access` 淘汰 |
| 32 页批量写 | — (Rust 侧未实现批量写) |
| 多页预读 `dword_6A5415C` | — (Rust 侧单页读取) |

## 7. 结论

`db1` 模块是 PDMS 数据库的"心脏"。所有的物理 I/O 和缓存一致性都由此模块保证。E3D 3.1 版本相比 2.x 架构完全一致，仅地址偏移不同。
