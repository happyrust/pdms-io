# E3D/PDMS 数据库驱动层分析报告 (db2-db5)

## 1. 概述

本报告是对 `core.dll` 中 db2、db3、db4、db5 模块的逆向分析总结。这些模块与 db1（页面管理器）共同构成了 PDMS/E3D 数据库引擎的核心驱动层。

> 本文档基于 **E3D 3.1** 版本 core.dll 更新（2026-04-09）。

| 模块 | 职责 | 关键功能 |
| :--- | :--- | :--- |
| **db1** | 页面管理器 | 物理页 I/O、内存缓存、页锁定 |
| **db2** | 会话/头部管理 | 数据库头部、会话信息、Extract 管理 |
| **db3** | B-树索引管理 | 索引页检索、节点分裂、表遍历 |
| **db4** | 元素/属性管理 | 元素创建、属性读写、引用关系 |
| **db5** | 数据库访问层 | 打开/关闭库、保存、压缩、刷新 |

## 2. E3D 3.1 调度架构

所有 db2~db5 函数在 3.1 版本中通过统一的 **opcode 调度器** 访问：

```
db_xxx(params)                        // C API wrapper (如 db_create_element)
  → dispatcher(opcode, params)        // 带日志/错误处理的统一分发
    → db[2-5]_xxx_impl(params)        // 实际实现函数
```

opcode 名称表位于 `off_6003900`，包含 60+ 个标准 `db_*` / `dbx_*` 函数名。

---

## 3. db2 模块 - 会话与头部管理

db2 负责管理数据库文件的头部信息、会话（Session）状态以及 Extract 操作记录。

| 函数名称 (IDA 标记) | E3D 2.x 地址 | E3D 3.1 (opcode → dispatcher) | 功能描述 |
| :--- | :--- | :--- | :--- |
| **`db2_init`** | `0x10621EE0` | — | 初始化会话管理器，设置数据库块查找表 |
| **`db2_insert_extract`** | `0x10620C20` | op=342 → `sub_5ACFBB0` | 插入 Extract 操作记录到数据库头部 |
| **`db2_remove_extract`** | `0x10620DD0` | `db_remove_extract` → dispatcher | 移除 Extract 记录 |
| **`db2_modify_header_page`** | `0x10620F60` | — | 修改数据库文件的元数据头部页 |
| **`db2_get_session_pgid`** | `0x10621480` | — | 获取当前会话对应的页 ID |
| **`db2_there_are_aux_db_blocks`** | `0x10621DC0` | — | 检查是否存在辅助数据库块 |
| **`db2_find_db_data`** | `0x10622800` | — | 根据库号定位对应的内存数据块 |
| **`db2_create_db_lookup_entry`** | `0x10622940` | — | 创建数据库查找表条目 |
| **`db2_find_empty_db_block`** | `0x10622CD0` | — | 查找空闲的数据库块槽位 |
| **`db2_find_current_db_block`** | `0x10622DC0` | — | 定位当前活动的数据库块 |
| **`db2_get_db_int_att`** | `0x10622F20` | `db_get_file_header_integer` op → `sub_5AC7100`系 | 获取数据库级整型属性 |
| **`db2_get_db_arr_att`** | `0x10623180` | `db_get_file_header_int_array` | 获取数据库级数组属性 |
| **`db2_set_db_int_att`** | `0x10623360` | — | 设置数据库级整型属性 |
| **`db2_set_db_arr_att`** | `0x106235A0` | — | 设置数据库级数组属性 |

---

## 4. db3 模块 - B-树索引管理

db3 实现了 PDMS 数据库的 B-树索引结构，用于高效检索元素和属性。

| 函数名称 (IDA 标记) | E3D 2.x 地址 | E3D 3.1 地址/入口 | 功能描述 |
| :--- | :--- | :--- | :--- |
| **`db3_init`** | `0x1061A950` | — | 初始化 B-树索引管理器 |
| **`db3_finish`** | `0x1061AC90` | op=192 → `sub_5AC0320` (via `db_finish_table_search`) | 关闭索引管理器，释放搜索令牌 |
| **`db3_get_page_entry`** | `0x1061AF30` | **`FHSRCH`** `0x5469180` | 从 B-树索引页中检索指定键的条目 |
| **`db3_update_page_entry`** | `0x1061B4B0` | — | 更新索引页中的条目 |
| **`db3_insert_page_entry`** | `0x1061B5C0` | **`FHXPND`** `0x5469690` | 向 B-树索引页插入新的键值对（含扩展） |
| **`db3_split_node`** | `0x1061BA50` | **`FHSPLT`** `0x5469CF0` | 当 B-树节点满时执行节点分裂 |
| **`db3_split_root`** | `0x1061C340` | — | 分裂根节点，增加树高度 |
| **`db3_create_new_table`** | `0x1061C8D0` | — | 在数据库中创建新的索引表结构 |
| **`db3_scan_index_page`** | `0x1061CF20` | **`FHITER`** `0x546DD8E` | 扫描索引页内容（迭代遍历） |
| **`db3_get_table_entry`** | `0x1061E9F0` | `DB_IndexTableIterator` `0x5A19070` | 根据键从索引表中检索对应值 |
| **`db3_start_table_search`** | `0x1061EC50` | `DB_IndexTableIterator::ctor` `0x5A19150` | 启动索引表的迭代搜索 |
| **`db3_get_next_table_entry`** | `0x1061F110` | `DB_IndexTableIterator::increment` `0x5A1AD00` | 迭代遍历，获取下一个索引条目 |
| — | — | **`FHDELT`** `0x5469EF0` | B-树节点删除 |

### E3D 3.1 中 db3 的 Fortran 桥接函数

| Fortran 函数 | 地址 | 对应 db3 操作 |
| :--- | :--- | :--- |
| `FHSRCH` | `0x5469180` | B-树搜索（文件路径查找） |
| `FHXPND` | `0x5469690` | B-树节点扩展/插入 |
| `FHSPLT` | `0x5469CF0` | B-树节点分裂 |
| `FHDELT` | `0x5469EF0` | B-树节点删除 |
| `FHFIND` | `0x546A0C0` | 文件查找/打开 |
| `FHITER` | `0x546DD8E` | 索引页迭代扫描 |
| `FHNPTH` | `0x546D970` | 路径解析/导航 |

---

## 5. db4 模块 - 元素与属性管理

db4 是 PDMS 元素（Element）的核心管理器，负责元素的创建、属性读写及引用关系维护。

| 函数名称 (IDA 标记) | E3D 2.x 地址 | E3D 3.1 (opcode → dispatcher) | 功能描述 |
| :--- | :--- | :--- | :--- |
| **`db4_init`** | `0x1060C5F0` | — | 初始化元素管理器，设置当前元素(CE)指针 |
| **`db4_init_element_page`** | `0x1060C950` | — | 初始化新的元素存储页结构 |
| **`db4_create_element`** | `0x1060CCA0` | op=32 → `sub_5ABD270` (via `db_create_element`) | 在数据库中分配并初始化新的 PDMS 元素 |
| **`db4_get_list`** | `0x1060CE20` | `db_get_int_array`/`db_get_ref_array` | 读取元素的列表类型属性数据 |
| **`db4_store_list`** | `0x1060D3A0` | `db_put_int_array`/`db_put_ref_array` | 将列表数据写入元素的属性区 |
| **`db4_copy_user_element`** | `0x1060D820` | — | 深拷贝一个元素及其所有属性 |
| **`db4_get_next_ext_ref`** | `0x1060E540` | — | 获取下一个外部引用 |
| **`db4_insert_ref`** | `0x1060E800` | `db_insert_element` op → dispatcher | 在元素间建立父子或关联引用关系 |
| **`db4_remove_ref`** | `0x1060EB10` | `db_detach_element` | 删除元素间的引用关系 |
| **`db4_clear_stack`** | `0x1060EBF0` | op=38 → `sub_5ABE020` (via `db_destroy_stack`) | 清除元素导航栈 |
| **`db4_get_ce_att`** | `0x10612A50` | 按类型分派: | 获取当前元素(CE)的指定属性值 |
| | | op=80 → `sub_5AC8160` (`db_get_integer`) | 整型属性 |
| | | op=106 → `sub_5ACDA10` (`db_get_string`) | 字符串属性 |
| | | `db_get_real` / `db_get_reference` / `db_get_logical` | 其他类型 |
| **`db4_get_att_dets`** | `0x10611FF0` | `db_get_attribute_info` / `db_get_attribute_list` | 查询属性的类型、大小等元信息 |

### E3D 3.1 属性读写完整 opcode 映射

| C API | opcode | dispatcher |
| :--- | :--- | :--- |
| `db_get_integer` | 80 | `sub_5AC8160` |
| `db_get_real` | — | — |
| `db_get_string` | 106 | `sub_5ACDA10` |
| `db_get_reference` | — | — |
| `db_get_logical` | — | — |
| `db_get_int_array` | — | — |
| `db_get_real_array` | — | — |
| `db_get_ref_array` | — | — |
| `db_put_integer` | 144 | `sub_5AD28D0` |
| `db_put_string` | — | — |
| `db_put_reference` | — | — |
| `db_put_int_array` | — | — |
| `db_put_ref_array` | — | — |
| `db_get_element_info` | 70 | `sub_5AC7100` |
| `db_go_to_element` | 108 | `sub_5ACF400` |
| `db_get_bucket` | 360 | `sub_5AC3810` |

---

## 6. db5 模块 - 数据库访问层

db5 是面向应用程序的高层接口，提供数据库的打开、关闭、保存、压缩等操作。

| 函数名称 (IDA 标记) | E3D 2.x 地址 | E3D 3.1 (opcode → dispatcher → impl) | 功能描述 |
| :--- | :--- | :--- | :--- |
| **`db5_init`** | `0x105E30E0` | — | 初始化数据库访问层，设置用户上下文 |
| **`db5_finish`** | `0x105E3740` | `db_finish` → dispatcher | 关闭访问层，保存未提交更改并释放资源 |
| **`db5_open_read_db`** | `0x105E4940` | op=134 → `sub_5AD0350` → **`sub_5AE5670`** | 以只读方式打开数据库文件 |
| **`db5_open_write_db`** | `0x105E4A40` | op=138 → `sub_5AD0910` | 以读写方式打开数据库（独占锁） |
| **`db5_open_shared_db`** | `0x105E4B60` | `db_open_shared_db` | 以共享方式打开数据库（多用户） |
| **`db5_close_db`** | `0x105E4D60` | `db_close_db` | 关闭数据库，同步缓存并释放句柄 |
| **`db5_set_mark`** | `0x105E4F30` | `db_mark_ses` | 设置事务回滚标记点(Mark) |
| **`db5_save_work`** | `0x105E9C80` | `db_save_work` (opcode table: `0x5d8967c`) | 将所有修改持久化到磁盘文件 |
| **`db5_compact`** | `0x105EA8B0` | op=28 → `sub_5ABCE90` (via `db_compact`) | 压缩数据库，整理碎片空间 |
| **`db5_refresh_work`** | `0x105E8000` | op=300 → `sub_5AD67A0` (via `db_refresh_db`) | 刷新工作区，重新加载其他用户的最新修改 |
| **`db5_suspend_db`** | `0x105E85A0` | — | 挂起数据库，暂停写入操作 |

### E3D 3.1 额外的 db5 级函数

| opcode 名称表中的函数 | 说明 |
| :--- | :--- |
| `db_write_page0` | 写入页面（变体） |
| `db_partial_save_work` | 部分保存 |
| `db_partial_save_work_all_but` | 排除式部分保存 |
| `dbx_save_work_extra` | 扩展保存 |
| `dbx_save_work_incl_table_changes` | 含表变更的保存 |
| `db_undo_failed_flush` | 撤销失败的刷新 |
| `db_undo_flush_using_parent` | 使用父级撤销刷新 |
| `db_register_failed_flush` | 注册失败的刷新 |
| `db_get_pending_and_failed_flushes` | 获取待处理和失败的刷新列表 |
| `dbx_read_page` | 扩展页面读取 |

---

## 7. 架构总结 (E3D 3.1)

```
┌─────────────────────────────────────────────────────────────────┐
│                    应用层 (PML/命令行/DB_DB C++)                   │
├─────────────────────────────────────────────────────────────────┤
│  db_* C API (opcode 调度 + off_6003900 名称表 + 错误码/日志)       │
├─────────────────────────────────────────────────────────────────┤
│  db5 - 数据库访问层 (sub_5AE5670 等)                              │
│  (open/close/save/compact/refresh)                              │
├─────────────────────────────────────────────────────────────────┤
│  db4 - 元素管理层                │  db2 - 会话管理层              │
│  (CE/属性/引用)                  │  (头部/Session/Extract)        │
│  sub_5ABD270 (create)            │  sub_5ACFBB0 (extract)        │
│  sub_5AC8160 (get_int)           │                                │
│  sub_5ACDA10 (get_str)           │                                │
├────────────────────────────┴────────────────────────────────────┤
│  db3 - B-树索引层 (Fortran 桥接)                                  │
│  FHSRCH(0x5469180) / FHXPND(0x5469690) / FHSPLT(0x5469CF0)     │
│  FHITER(0x546DD8E) / DB_IndexTableIterator(0x5A19070)           │
├─────────────────────────────────────────────────────────────────┤
│  db1 - 页面管理层                                                 │
│  get_page(0x5AEE4E0) / lock_page(0x5AEFC30)                     │
│  update_page(0x5AF1600) / write_page(0x5AF1C30)                  │
│  read_page(0x5AF0640) → FHDBRN(0x5B8D4F0)                       │
├─────────────────────────────────────────────────────────────────┤
│  Fortran I/O 层                                                   │
│  FHDBRN → DirectAccessToken vtable[16] → OS ReadFile             │
│  FHDBWN → DirectAccessToken vtable[?]  → OS WriteFile            │
│  FIOXST / FIONEW / FUDEL / FHSWIT / FHLOSE                      │
├─────────────────────────────────────────────────────────────────┤
│                         操作系统 I/O                              │
└─────────────────────────────────────────────────────────────────┘
```

## 8. 版本对照说明

E3D 2.x 文档中的函数名 (`db1_xxx` ~ `db5_xxx`) 是分析员在 IDA Pro 中手动命名的，不是原始符号。E3D 3.1 core.dll 中不包含这些符号字符串（仅有 `db1_update_page` 和 `DB1_n_pages_written` 两个调试字符串残留）。

两版本**架构完全一致**，差异仅为编译地址偏移。
