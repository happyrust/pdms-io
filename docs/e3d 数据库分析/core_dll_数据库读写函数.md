# core.dll 数据库读写函数总结

> 通过 IDA Pro 逆向分析 core.dll 中的 db1-db5 调试字符串，定位并命名了 **401 个** 数据库读写相关函数。
> 这些函数构成了 E3D/PDMS 数据库引擎的五层架构。

## 架构概览

```
┌─────────────────────────────────────────────────┐
│  db5 - 会话/Extract管理层 (最高层)              │
│  负责: 数据库打开/关闭、会话管理、flush/refresh  │
│  负责: 多写合并、声明/释放、压缩操作             │
│  函数数量: ~104                                  │
├─────────────────────────────────────────────────┤
│  db4 - 元素管理层                                │
│  负责: 元素CRUD、属性读写、当前元素(CE)栈管理    │
│  负责: 层次导航、引用管理、成员列表操作          │
│  函数数量: ~123                                  │
├─────────────────────────────────────────────────┤
│  db3 - B树索引层                                 │
│  负责: B树索引操作、表搜索、页条目管理           │
│  负责: 节点分裂、索引比较                        │
│  函数数量: ~34                                   │
├─────────────────────────────────────────────────┤
│  db2 - 数据库块管理层                            │
│  负责: 数据库块创建/删除、extract管理             │
│  负责: 会话属性读写、Page0/Page1属性、桶管理     │
│  函数数量: ~90                                   │
├─────────────────────────────────────────────────┤
│  db1 - 物理页管理层 (最底层)                     │
│  负责: 物理页读写、页缓存、页框管理              │
│  负责: 页锁定/解锁、令牌管理、文件I/O            │
│  函数数量: ~50                                   │
└─────────────────────────────────────────────────┘
```

---

## db5 - 会话/Extract管理层

最高层接口，管理数据库的打开/关闭、会话（session）、extract 的 flush/refresh 操作，以及多写模式下的合并与声明/释放机制。

### 初始化与生命周期

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x105ECCC0 | db5_init | 数据库第5层初始化 |
| 0x105ED260 | db5_disconnect_cache | 断开数据库缓存连接 |
| 0x105ED320 | db5_finish | 数据库第5层结束/清理 |
| 0x105EDC00 | db5_set_user | 设置数据库用户 |
| 0x105EDCA0 | db5_get_statistics | 获取数据库统计信息 |
| 0x105EDEE0 | db5_set_conversion_language | 设置转换语言 |

### 数据库打开/关闭

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x105EE450 | db5_get_current_db_mode | 获取当前数据库模式 |
| 0x105EE520 | db5_open_read_db | 以只读模式打开数据库 |
| 0x105EE620 | db5_open_write_db | 以写入模式打开数据库 |
| 0x105EE740 | db5_open_shared_db | 以共享模式打开数据库 |
| 0x105EE840 | db5_open_nosave_db | 以不保存模式打开数据库 |
| 0x105EE940 | db5_close_db | 关闭数据库 |

### 会话与标记

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x105EDF70 | db5_mark_ses | 标记会话 |
| 0x105EE0D0 | db5_is_db_changed | 检查数据库是否已更改 |
| 0x105EE230 | db5_is_ext_changed | 检查extract是否已更改 |
| 0x105EEB10 | db5_set_mark | 设置数据库标记点 |
| 0x105EEC00 | db5_move_to_adjacent_mark | 移动到相邻标记点 |
| 0x105EEFA0 | db5_start_tab_comp_between_marks | 在标记点之间开始表比较 |

### 错误处理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x105EF0D0 | db5_get_no_of_errors | 获取错误数量 |
| 0x105EF1A0 | db5_get_next_error | 获取下一个错误 |
| 0x105EF2E0 | db5_put_error | 记录错误信息 |

### 元素创建/移除权限检查

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x105EF540 | db5_el_removal_from_ce_list_ok | 检查是否可从CE列表中移除元素 |
| 0x105EF6A0 | db5_ce_removal_ok | 检查CE是否可以移除 |
| 0x105EF7F0 | db5_el_creation_in_ce_list_ok | 检查是否可在CE列表中创建元素 |
| 0x105EFA40 | db5_el_creation_ok | 检查元素创建是否允许 |

### 页写入与会话管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x105EFCC0 | db5_write_changed_table_pages | 写入已更改的表页 |
| 0x105F0200 | db5_write_changed_element_pages | 写入已更改的元素页 |
| 0x105F0600 | db5_create_session_page | 创建会话页 |
| 0x105F0DA0 | db5_validate_claim_lists | 验证声明列表 |

### 退出与保存

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x105F1070 | db5_quit | 退出数据库(含保存) |
| 0x105F2FE0 | db5_partial_save_quit | 部分保存并退出 |
| 0x105F3870 | db5_save_work | 保存工作区 |

### Extract管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x105F14D0 | db5_is_ext_in_list | 检查extract是否在列表中 |
| 0x105F15B0 | db5_is_ext_list_a_tree | 检查extract列表是否为树结构 |
| 0x105F1700 | db5_update_compaction_cnt_in_ext_tree | 更新extract树中的压缩计数 |
| 0x105F1950 | db5_copy_els_in_hierarchy_for_compact | 为压缩复制层次结构中的元素 |
| 0x105F1BE0 | db5_refresh_work | 刷新工作区 |
| 0x105F2180 | db5_suspend_db | 挂起数据库 |
| 0x105F2280 | db5_clone_old_session | 克隆旧会话 |

### 压缩操作

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x105F44A0 | db5_compact | 压缩数据库 |
| 0x105F5300 | db5_compact_leaf | 压缩叶节点 |
| 0x105F54B0 | db5_compact_ext_tree | 压缩extract树 |
| 0x105F5670 | db5_compact_extracts | 压缩extracts |

### Flush/Refresh 操作

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10641860 | db5_update_view_of_db | 更新数据库视图 |
| 0x10641B10 | db5_undo_given_flush_from_local_ext | 撤销指定的本地extract刷新 |
| 0x106429D0 | db5_undo_flush_from_local_ext | 撤销本地extract刷新 |
| 0x10642ED0 | db5_find_incomplete_flushes | 查找未完成的刷新 |
| 0x106437F0 | db5_decide_direction_of_refresh | 确定刷新方向 |
| 0x106439E0 | db5_check_els_for_flush | 检查元素是否可刷新 |
| 0x10643C10 | db5_complete_flush_with_nothing_to_refresh | 完成无需刷新的flush |
| 0x10644160 | db5_do_disk_space_saving_for_refresh | 为刷新执行磁盘空间节省 |
| 0x10644600 | db5_register_failed_flush | 注册失败的刷新 |
| 0x106449D0 | db5_copy_roots_for_rigorous_merge | 为严格合并复制根节点 |
| 0x10644B90 | db5_check_claim_lists | 检查声明列表 |
| 0x10645220 | db5_prepare_for_flush_refresh | 准备刷新/flush操作 |
| 0x106455D0 | db5_flush_from_local_ext | 从本地extract执行flush |
| 0x10646530 | db5_undo_all_flushes_from_local_ext | 撤销所有本地extract的flush |
| 0x106466F0 | db5_do_flush | 执行flush操作 |
| 0x10647A50 | db5_refresh_abandon | 放弃刷新操作 |
| 0x106490E0 | db5_flush | 执行数据库flush |

### 声明/释放 (Claim/Release)

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10649680 | db5_is_prim_el_claim_ok | 检查主元素声明是否允许 |
| 0x10649860 | db5_is_el_unchanged | 检查元素是否未更改 |
| 0x10649C70 | db5_do_singlewrite_claim_release | 执行单写声明/释放 |
| 0x1064A260 | db5_change_claim_list_entry | 更改声明列表条目 |
| 0x1064A420 | db5_expunge_user | 清除用户 |
| 0x1064A9D0 | db5_refresh_claim_list | 刷新声明列表 |
| 0x1064AC80 | db5_clm_rls_el_in_owning_ext | 在拥有的extract中声明/释放元素 |
| 0x1064B560 | db5_rls_all_but_in_owning_ext | 在拥有的extract中释放除指定外的所有 |
| 0x1064FCE0 | db5_is_prim_el_release_ok | 检查主元素释放是否允许 |
| 0x1064FE60 | db5_test_and_do_claim_or_release | 测试并执行声明或释放 |
| 0x10650190 | db5_do_claim_release | 执行声明/释放操作 |
| 0x106504A0 | db5_claim_release_refs | 声明/释放引用 |
| 0x10650950 | db5_release_all_but | 释放除指定外的所有 |
| 0x10650F30 | db5_clm_rls_el_in_local_ext | 在本地extract中声明/释放元素 |
| 0x10651550 | db5_clm_rls_refs_in_owning_ext | 在拥有的extract中声明/释放引用 |
| 0x10651870 | db5_rls_all_in_local_ext | 释放本地extract中的所有 |
| 0x10654B60 | db5_clm_rls_refs_in_local_ext | 在本地extract中声明/释放引用 |

### 多写合并 (Multiwrite Merge)

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1064B950 | db5_check_for_name_clash | 检查名称冲突 |
| 0x1064C0E0 | db5_check_reflist_for_del_el | 检查引用列表中的已删除元素 |
| 0x1064C470 | db5_check_reflist_for_ins_el | 检查引用列表中的已插入元素 |
| 0x1064C840 | db5_check_reflist_for_moved_el | 检查引用列表中的已移动元素 |
| 0x1064CC20 | db5_is_merge_needed | 检查是否需要合并 |
| 0x1064CE40 | db5_handle_modified_lock_flag | 处理已修改的锁定标志 |
| 0x1064D070 | db5_handle_given_modified_att | 处理指定的已修改属性 |
| 0x1064E1E0 | db5_get_version_of_combined_table_att | 获取组合表属性的版本 |
| 0x1064E3C0 | db5_add_items_to_changed_list | 添加项目到已更改列表 |
| 0x1064E700 | db5_deal_with_items_in_changed_list | 处理已更改列表中的项目 |
| 0x1064EEC0 | db5_turn_user_key_into_table_key | 将用户键转换为表键 |
| 0x1064F0F0 | db5_handle_data_changes_in_table_atts | 处理表属性中的数据变更 |
| 0x1064F870 | db5_multiwrite_quit | 多写模式退出 |
| 0x10651D10 | db5_is_merge_allowed | 检查是否允许合并 |
| 0x106528A0 | db5_create_table_entries_for_table_atts | 为表属性创建表条目 |
| 0x10653310 | db5_handle_modified_combined_table_att | 处理已修改的组合表属性 |
| 0x10654090 | db5_are_atts_same | 检查属性是否相同 |
| 0x10654E90 | db5_handle_inserted_el | 处理已插入的元素 |
| 0x106552B0 | db5_handle_modified_atts | 处理已修改的属性 |
| 0x10655A80 | db5_are_els_same | 检查元素是否相同 |
| 0x10655F80 | db5_handle_modified_el | 处理已修改的元素 |
| 0x106561F0 | db5_avoid_copies_of_els | 避免元素的副本 |
| 0x106566D0 | db5_handle_claim_list_changes | 处理声明列表变更 |
| 0x10656DA0 | db5_handle_changes_between_base_and_working | 处理基础版本和工作版本之间的变更 |
| 0x106575B0 | db5_handle_element_changes | 处理元素变更 |
| 0x106577D0 | db5_multiwrite_merge_work | 多写模式合并工作 |

---

## db4 - 元素管理层

管理数据库元素的创建、删除、属性读写、当前元素（CE）栈操作、层次导航和引用管理。

### 初始化与生命周期

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10616960 | db4_init | 数据库第4层初始化(元素管理层) |
| 0x1061B260 | db4_finish | 数据库第4层结束/清理 |
| 0x1065C840 | db4_initd | 数据库第4层初始化(差异比较) |
| 0x1065CA20 | db4_finish (副本) | 数据库第4层结束/清理 |

### 元素页管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10616CC0 | db4_init_element_page | 初始化元素页 |
| 0x10616DA0 | db4_get_nep_info | 获取下一元素页信息 |
| 0x10616E80 | db4_set_part_element_page | 设置部分元素页 |

### 元素创建/删除/导航

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10617010 | db4_create_element | 创建元素 |
| 0x1061D7D0 | db4_move_down | 向下移动(层次导航) |
| 0x1061DA40 | db4_insert_element | 插入元素 |
| 0x1061DF80 | db4_detach_element | 分离元素 |
| 0x1061E240 | db4_get_current_pos | 获取当前位置 |
| 0x1061E3D0 | db4_set_cur_pos_to_end | 设置当前位置到末尾 |
| 0x1061E500 | db4_set_cur_pos_to_start | 设置当前位置到起始 |
| 0x1061E620 | db4_go_to_owner | 跳转到所有者 |
| 0x1061E770 | db4_create_element_in_list | 在列表中创建元素 |
| 0x10624470 | db4_delete_ce | 删除当前元素 |

### 列表操作

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10617190 | db4_get_list | 获取列表 |
| 0x10617710 | db4_store_list | 存储列表 |
| 0x10617B90 | db4_get_list (副本) | 获取列表（复制用户元素相关） |
| 0x1061F020 | db4_update_bucket_list | 更新桶列表 |

### 引用管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x106188B0 | db4_get_next_ext_ref | 获取下一个外部引用 |
| 0x10618A60 | db4_locate_ref | 定位引用 |
| 0x10618B70 | db4_insert_ref | 插入引用 |
| 0x10618C70 | db4_remove_refs_for_dbno | 移除指定数据库编号的引用 |
| 0x10618D80 | db4_remove_refs_on_temp_pages | 移除临时页上的引用 |
| 0x10618E80 | db4_remove_ref | 移除引用 |

### 当前元素(CE)栈管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10618F60 | db4_clear_stack | 清除栈 |
| 0x10619320 | db4_store_incore_lists | 存储内存列表 |
| 0x106194E0 | db4_set_ce_from_extref | 从外部引用设置当前元素 |
| 0x1061A670 | db4_pop_ce_stack | 弹出当前元素栈 |
| 0x1061A810 | db4_save_ce_stack | 保存当前元素栈 |
| 0x1061AAA0 | db4_get_nstacks_in_use | 获取正在使用的栈数量 |
| 0x1061ABA0 | db4_destroy_stack | 销毁栈 |
| 0x1061B9F0 | db4_restore_saved_stack | 恢复已保存的栈 |
| 0x1061BC00 | db4_destroy_given_stack | 销毁指定栈 |
| 0x1061BD80 | db4_destroy_stacks_for_db | 销毁数据库的所有栈 |

### 数据库切换

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1061AD20 | db4_switch_db | 切换数据库 |
| 0x1061AE10 | db4_there_is_no_ce | 检查是否没有当前元素 |
| 0x1061AEF0 | db4_switch_to_old_db_block | 切换到旧数据库块 |
| 0x1061B0D0 | db4_switch_to_original_db_block | 切换到原始数据库块 |

### 当前元素更新与验证

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1061B660 | db4_update_ce | 更新当前元素 |
| 0x1061BE90 | db4_ce_update_ok | 检查当前元素更新是否允许 |
| 0x1061CA90 | db4_check_el_type_against_ce | 检查元素类型与当前元素的匹配 |

### 属性类型管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1061C450 | db4_disable_attribute_type | 禁用属性类型 |
| 0x1061C650 | db4_shorten_attribute_type | 缩短属性类型 |
| 0x1061C890 | db4_restrict_ref_attribute | 限制引用属性 |
| 0x1061CC00 | db4_isUserDefinedAttribute | 判断是否为用户自定义属性 |
| 0x1061CEF0 | db4_isSystemAttribute | 判断是否为系统属性 |
| 0x10620600 | db4_isDynamicAttribute | 判断是否为动态属性 |

### 当前元素信息获取

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1061CC80 | db4_getCurrentElement | 获取当前元素 |
| 0x1061CD10 | db4_getBitField | 获取位域 |
| 0x1061CD90 | db4_getDoublePrecisionFlag | 获取双精度标志 |
| 0x1061CE60 | db4_isThereACurrentElement | 判断是否存在当前元素 |
| 0x1061CF70 | db4_get_ce_settings | 获取当前元素设置 |
| 0x1061D220 | db4_get_ce_extref | 获取当前元素的外部引用 |
| 0x1061D310 | db4_set_ce_lock | 设置当前元素锁定 |

### 属性读写

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1061F2D0 | db4_get_att_dets | 获取属性详细信息 |
| 0x1061F670 | db4_getStaticAttributeOffset | 获取静态属性偏移量 |
| 0x1061F740 | db4_remove_ce_dyn_att | 移除当前元素的动态属性 |
| 0x1061F9C0 | db4_check_el_type_against_ref_att | 检查元素类型与引用属性的匹配 |
| 0x1061FBC0 | db4_set_ce_att | 设置当前元素属性 |
| 0x106206E0 | db4_getDynamicAttribute | 获取动态属性 |
| 0x10621000 | db4_get_ce_att | 获取当前元素属性值 |
| 0x10623D10 | db4_find_att | 查找属性 |
| 0x10624E00 | db4_remove_ce_dyn_att_incl_tab_atts | 移除当前元素的动态属性(含表属性) |

### 表属性操作

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1061D430 | db4_check_shared_da_will_fit | 检查共享数据属性是否适配 |
| 0x1061D560 | db4_find_combined_table_att_bead | 查找组合表属性的bead |
| 0x1061D6C0 | db4_find_discrete_int_table_att_bead | 查找离散整数表属性的bead |
| 0x106211E0 | db4_get_ce_table_att | 获取当前元素的表属性 |
| 0x106219B0 | db4_get_ce_int_table_att_entries | 获取当前元素的整数表属性条目 |
| 0x10621D30 | db4_does_table_entry_exist | 检查表条目是否存在 |
| 0x106220C0 | db4_remove_ce_table_att | 移除当前元素的表属性 |
| 0x10622AE0 | db4_set_ce_table_att | 设置当前元素的表属性 |
| 0x106236B0 | db4_set_ce_ref_tab_arr_att | 设置当前元素的引用表数组属性 |
| 0x10623BB0 | db4_set_ce_ref_att | 设置当前元素的引用属性 |
| 0x10624F40 | db4_set_ce_ref_arr_att | 设置当前元素的引用数组属性 |

### 当前元素数据属性

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10619DF0 | db4_get_ce_da_list | 获取当前元素的数据属性列表 |
| 0x10619F50 | db4_get_ce_members_list | 获取当前元素的成员列表 |
| 0x1061A0B0 | db4_convert_el_body_to_double | 将元素体转换为双精度 |
| 0x1061A260 | db4_convert_da_list_to_double | 将数据属性列表转换为双精度 |

### 声明检查与会话缓存

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10649DF0 | db4_set_claim_checking_flag | 设置声明检查标志 |
| 0x106584C0 | db4_has_element_changed | 检查元素是否已更改 |
| 0x10658BE0 | db4_build_session_cache | 构建会话缓存 |
| 0x10659350 | db4_remove_session_caching | 移除会话缓存 |

### 元素信息查询

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10659440 | db4_go_to_primary_owner_for_ce | 跳转到当前元素的主所有者 |
| 0x10659570 | db4_get_ce_att_default | 获取当前元素的默认属性值 |
| 0x10659B70 | db4_get_list_from_el_def | 从元素定义获取列表 |
| 0x10659DE0 | db4_get_current_dbno | 获取当前数据库编号 |
| 0x10659EA0 | db4_get_dbno_from_extref | 从外部引用获取数据库编号 |
| 0x10659F70 | db4_get_att_list_for_element | 获取元素的属性列表 |
| 0x1065A190 | db4_get_element_info | 获取元素信息 |
| 0x1065A3E0 | db4_get_element_list_info | 获取元素列表信息 |
| 0x1065A690 | db4_get_table_info | 获取表信息 |
| 0x1065A870 | db4_get_att_info | 获取属性信息 |
| 0x1065AD50 | db4_get_atts_from_el | 从元素获取属性 |
| 0x1065AF40 | db4_get_changed_sessions_for_ce | 获取当前元素的已更改会话 |
| 0x1065C440 | db4_get_created_session_for_ce | 获取当前元素的创建会话 |

### 差异比较与成员列表操作

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10662880 | DB4_el_list | 第4层元素列表操作 |
| 0x10664400 | DB4_el_list (副本) | 第4层元素列表操作 |
| 0x1065CC70 | db4_prepare_mem_list_for_insertion | 为插入准备成员列表 |
| 0x1065CFE0 | db4_remove_el_from_mem_list | 从成员列表移除元素 |
| 0x1065D400 | db4_move_el_in_mem_list | 在成员列表中移动元素 |
| 0x1065D690 | db4_compare_el_versions | 比较元素版本 |
| 0x1065F7B0 | db4_el_created_under_deleted_el | 检查元素是否创建在已删除元素下 |
| 0x1065FB00 | db4_build_list_of_atts | 构建属性列表 |
| 0x10660570 | db4_get_next_mem_list_diff | 获取下一个成员列表差异 |
| 0x10660C40 | db4_start_mem_list_compare | 开始成员列表比较 |
| 0x10660F30 | db4_copy_secondary_list | 复制辅助列表 |
| 0x106610B0 | db4_check_els_in_secondary_list | 检查辅助列表中的元素 |
| 0x10661330 | db4_is_changed_member_primary | 检查已更改成员是否为主成员 |
| 0x10661500 | db4_end_mem_list_compare | 结束成员列表比较 |
| 0x106615F0 | db4_build_mem_list_for_ins_el | 为插入元素构建成员列表 |
| 0x10661F90 | db4_are_att_lists_same | 检查属性列表是否相同 |
| 0x10662330 | db4_create_mem_list_for_ins_el | 为插入元素创建成员列表 |
| 0x10662640 | db4_get_next_index_diff_from_el_list | 从元素列表获取下一个索引差异 |
| 0x10662D30 | db4_handle_data_changes_using_reflist | 使用引用列表处理数据变更 |
| 0x10663700 | db4_insert_el_in_mem_list | 在成员列表中插入元素 |
| 0x10663900 | db4_reorder_primary_members | 重新排序主成员 |
| 0x10663B80 | db4_handle_mem_list_changes | 处理成员列表变更 |
| 0x106641A0 | db4_attach_el_to_owner | 将元素附加到所有者 |
| 0x10664820 | db4_handle_modified_owner | 处理已修改的所有者 |
| 0x10664BE0 | db4_handle_replaced_el | 处理已替换的元素 |
| 0x10662880 | db4_get_non_primary_members | 获取非主成员 |
| 0x10664400 | db4_get_full_list_from_reflist | 从引用列表获取完整列表 |

---

## db3 - B树索引层

管理数据库的 B 树索引结构，包括索引页的读写、节点分裂、表搜索和索引比较。

### 初始化与生命周期

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10625680 | db3_init | 数据库第3层初始化(B树索引层) |
| 0x106259C0 | db3_finish | 数据库第3层结束/清理 |

### 搜索令牌管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10625090 | db3_create_search_token | 创建搜索令牌 |
| 0x106253F0 | db3_get_nsearch_toks_in_use | 获取正在使用的搜索令牌数量 |
| 0x106254D0 | db3_switch_needed | 检查是否需要切换 |
| 0x106286A0 | db3_delete_search_token | 删除搜索令牌 |
| 0x106288C0 | db3_delete_search_token (副本) | 删除搜索令牌/移除数据库令牌 |

### 页条目操作

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10625C60 | db3_get_page_entry | 获取页条目 |
| 0x10625EE0 | db3_get_page_entry_inc0 | 获取页条目(含第0级) |
| 0x106261E0 | db3_update_page_entry | 更新页条目 |
| 0x106262F0 | db3_insert_page_entry | 插入页条目 |
| 0x106264C0 | db3_remove_level_0_page_entry | 移除第0级页条目 |
| 0x10626620 | db3_remove_level_n_current_page_entry | 移除第N级当前页条目 |

### B树节点操作

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10626790 | db3_split_node | B树节点分裂 |
| 0x10627080 | db3_split_root | B树根节点分裂 |
| 0x106272F0 | db3_set_hint_table_root | 设置提示表根节点 |
| 0x10627540 | db3_unset_hint_table_root | 取消设置提示表根节点 |
| 0x10627610 | db3_create_new_table | 创建新表(B树) |
| 0x10627840 | db3_update_level_n_first_key_entry | 更新第N级首键条目 |

### 表搜索与查询

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x106258C0 | db3_check_table_modified | 检查表是否已修改 |
| 0x106279A0 | db3_get_table_entry_given_root | 根据根节点获取表条目 |
| 0x10627C60 | db3_scan_index_page | 扫描索引页 |
| 0x10628C10 | db3_change_table_entry | 更改表条目 |
| 0x10629760 | db3_get_table_entry | 获取表条目 |
| 0x10629870 | db3_get_name_table_entry | 获取名称表条目 |
| 0x106299C0 | db3_start_table_search | 开始表搜索 |
| 0x10629E80 | db3_get_next_table_entry | 获取下一个表条目 |

### 索引比较

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10627DE0 | db3_start_table_compare | 开始表比较 |
| 0x10628120 | db3_invalidate_compare | 使比较无效 |
| 0x10628220 | db3_start_high_level_compare | 开始高级别比较 |
| 0x10628410 | db3_end_table_compare | 结束表比较 |
| 0x1062A360 | db3_check_for_same_pages | 检查是否为相同页 |
| 0x1062ACB0 | db3_get_no_of_index_pages_changed | 获取已更改的索引页数量 |
| 0x1062AE90 | db3_get_next_difference | 获取下一个差异 |

---

## db2 - 数据库块管理层

管理数据库块（DB Block）的创建、删除、切换，以及 extract 的管理、会话属性的读写和桶（bucket）操作。

### 初始化与生命周期

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1062CC50 | db2_init | 数据库第2层初始化 |
| 0x10638A90 | db2_finish | 数据库第2层结束/清理 |

### Extract管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1062B990 | db2_insert_extract | 插入extract |
| 0x1062BB40 | db2_remove_extract | 移除extract |
| 0x1062C5C0 | db2_open_template_db | 打开模板数据库 |
| 0x1062CB30 | db2_there_are_aux_db_blocks | 检查是否存在辅助数据库块 |
| 0x1062D190 | db2_get_template_header_info | 获取模板头信息 |
| 0x10638510 | db2_close_template_db | 关闭模板数据库 |

### 数据库块操作

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1062BCD0 | db2_modify_header_page | 修改头页 |
| 0x1062D370 | db2_number_of_suspended_write_dbs | 获取挂起的写入数据库数量 |
| 0x1062D450 | db2_find_given_db_block | 查找指定的数据库块 |
| 0x1062D570 | db2_find_db_data | 查找数据库数据 |
| 0x1062D6B0 | db2_create_db_lookup_entry | 创建数据库查找条目 |
| 0x1062DA40 | db2_find_empty_db_block | 查找空数据库块 |
| 0x1062DB30 | db2_find_current_db_block | 查找当前数据库块 |
| 0x10636250 | db2_create_master | 创建master数据库 |
| 0x10636B00 | db2_create_extract | 创建extract数据库 |
| 0x10637250 | db2_create_db_block | 创建数据库块 |
| 0x10637990 | db2_create_compact_dbb | 创建压缩数据库块 |
| 0x10637BD0 | db2_delete_db_block | 删除数据库块 |
| 0x10637E80 | db2_delete_db_lookup_entry | 删除数据库查找条目 |
| 0x10638170 | db2_remove_db_incore_data | 移除数据库内存数据 |
| 0x106383D0 | db2_delete_compact_dbb | 删除压缩数据库块 |
| 0x10638930 | db2_change_cur_db_block | 更改当前数据库块 |
| 0x106394E0 | db2_clone_db_block | 克隆数据库块 |

### 数据库属性读写

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1062DC90 | db2_get_db_int_att | 获取数据库整数属性 |
| 0x1062DEF0 | db2_get_db_arr_att | 获取数据库数组属性 |
| 0x1062E0D0 | db2_set_db_int_att | 设置数据库整数属性 |
| 0x1062E310 | db2_set_db_arr_att | 设置数据库数组属性 |

### 表根节点管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1062E500 | db2_get_db_table_root | 获取数据库表根节点 |
| 0x1062E6C0 | db2_remove_db_table_root | 移除数据库表根节点 |
| 0x1062E9B0 | db2_set_db_table_root | 设置数据库表根节点 |
| 0x1062ECE0 | db2_set_new_db_table_root | 设置新的数据库表根节点 |
| 0x1062EFB0 | db2_copy_roots_using_given_extno | 使用指定extract编号复制根节点 |

### 元素定义

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1062F170 | db2_get_element_details | 获取元素详细信息 |
| 0x1062F810 | db2_get_element_definition | 获取元素定义 |
| 0x1062F9E0 | db2_disable_element_type | 禁用元素类型 |
| 0x1062FBF0 | db2_get_next_table | 获取下一个表 |
| 0x1062FDA0 | db2_get_next_fileid | 获取下一个文件ID |

### 标记与回退

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1062FF30 | db2_set_roots_for_mark | 为标记设置根节点 |
| 0x10630120 | db2_remove_roots_above_mark | 移除标记之上的根节点 |
| 0x10630360 | db2_remove_all_but_newest_roots | 移除除最新外的所有根节点 |
| 0x106305D0 | db2_clear_mark | 清除标记 |
| 0x10630890 | db2_has_db_been_changed_since_mark | 检查标记后数据库是否已更改 |
| 0x10630990 | db2_rewind_db | 回退数据库 |

### 会话属性读写

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1062C1F0 | db2_get_session_pgid | 获取会话页ID |
| 0x10630D50 | db2_get_current_session_num | 获取当前会话编号 |
| 0x10630EA0 | db2_get_ses_int_att | 获取会话整数属性 |
| 0x106311B0 | db2_get_session_int_att | 获取会话整数属性(按会话) |
| 0x10631320 | db2_get_ses_arr_att | 获取会话数组属性 |
| 0x10631620 | db2_get_session_arr_att | 获取会话数组属性(按会话) |
| 0x10632C90 | db2_put_ses_arr_att | 写入会话数组属性 |
| 0x10633000 | db2_put_ses_int_att | 写入会话整数属性 |
| 0x10632980 | db2_get_latest_valid_sesno | 获取最新有效会话编号 |
| 0x10639920 | db2_set_base_session | 设置基础会话 |
| 0x10639A70 | db2_get_tok_session_int_att | 获取令牌会话整数属性 |
| 0x10639BC0 | db2_get_tok_session_arr_att | 获取令牌会话数组属性 |

### Page0属性读写

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10631790 | db2_get_p0_int_att | 获取Page0整数属性 |
| 0x10631B40 | db2_get_page0_int_att | 获取Page0整数属性(按令牌) |
| 0x10631CC0 | db2_get_tok_page0_arr_att | 获取令牌Page0数组属性 |
| 0x10631EC0 | db2_get_p0_arr_att | 获取Page0数组属性 |
| 0x106321E0 | db2_get_page0_arr_att | 获取Page0数组属性(按令牌) |
| 0x10632480 | db2_get_p0_ref_arr_att | 获取Page0引用数组属性 |
| 0x106325F0 | db2_get_page0_ref_arr_att | 获取Page0引用数组属性(按令牌) |
| 0x10632770 | db2_get_tok_page0_ref_arr_att | 获取令牌Page0引用数组属性 |
| 0x106332A0 | db2_put_p0_arr_att | 写入Page0数组属性 |
| 0x10633500 | db2_put_page0_arr_att | 写入Page0数组属性(按令牌) |
| 0x10633650 | db2_put_p0_int_att | 写入Page0整数属性 |
| 0x106338C0 | db2_put_page0_int_att | 写入Page0整数属性(按令牌) |
| 0x10633A10 | db2_put_tok_page0_int_att | 写入令牌Page0整数属性 |

### 页面读写

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10633C10 | db2_read_page | 读取数据库页 |
| 0x10639D10 | db2_write_page | 写入数据库页 |
| 0x1063A640 | db2_open_db | 打开数据库 |

### 文件与会话检查

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x106340E0 | db2_get_db_file_details | 获取数据库文件详细信息 |
| 0x106343E0 | db2_check_and_update_session | 检查并更新会话 |
| 0x10634870 | db2_check_session_pages | 检查会话页 |
| 0x10634B80 | db2_get_next_extref | 获取下一个外部引用 |
| 0x10634F70 | db2_update_page1_userid_or_ref | 更新Page1的用户ID或引用 |

### 桶(Bucket)管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10635290 | db2_create_or_update_bucket_entry | 创建或更新桶条目 |
| 0x10635780 | db2_adjust_bucket_entry | 调整桶条目 |
| 0x10635900 | db2_find_unused_bucket | 查找未使用的桶 |
| 0x10635C30 | db2_get_bucket | 获取桶 |
| 0x10635D40 | db2_add_bucket | 添加桶 |
| 0x10635E50 | db2_remove_bucket_entry | 移除桶条目 |
| 0x106360A0 | db2_get_bucket_from_reference | 从引用获取桶 |
| 0x10636170 | db2_build_reference | 构建引用 |

### 数据库挂起/恢复

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x10638FE0 | db2_suspend_db | 挂起数据库 |
| 0x10639320 | db2_restore_db | 恢复数据库 |
| 0x106397B0 | db2_rewind_owning_ext | 回退拥有的extract |

---

## db1 - 物理页管理层

最底层的页面 I/O 管理，负责物理页的读写、缓存管理、页框分配、锁定/解锁和令牌机制。

### 初始化与生命周期

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1063E930 | db1_init | 数据库第1层初始化(物理页管理层) |
| 0x1063E070 | db1_finish | 数据库第1层结束/清理 |

### 页查找表(PLU)操作

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1063B000 | db1_plu_add_entry | 页查找表添加条目 |
| 0x1063B130 | db1_plu_remove_entry | 页查找表移除条目 |
| 0x1063B2E0 | db1_plu_locate_entry | 页查找表定位条目 |

### 页面读写

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1063B3F0 | db1_write_page_basic | 基本页写入 |
| 0x1063B980 | db1_read_page | 读取物理页 |
| 0x1063C280 | db1_write_db_page | 写入数据库页到磁盘 |
| 0x1063EEF0 | db1_write_page | 写入页 |
| 0x10640D80 | db1_update_page | 更新页 |
| 0x10641540 | db1_write_update_db_pages | 写入更新的数据库页 |
| 0x106410F0 | db1_open_extract_file | 打开extract文件 |

### 页框与缓存管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1063B610 | db1_get_no_of_free_page_frames | 获取空闲页框数量 |
| 0x1063B6F0 | db1_dehash | 页哈希反解 |
| 0x1063B850 | db1_get_no_of_free_pages | 获取空闲页数量 |
| 0x1063BC40 | db1_set_n_pages_to_read | 设置要读取的页数 |
| 0x1063C170 | db1_is_page_incore | 检查页是否在内存中 |
| 0x1063F560 | db1_get_page_frame | 获取页框 |
| 0x1063F850 | db1_increase_incore_pages | 增加内存页数量 |
| 0x1063FD10 | db1_get_page | 获取页(含缓存命中逻辑) |
| 0x10640A50 | db1_get_new_page | 获取新页 |
| 0x10640C00 | db1_check_page | 检查页有效性 |

### 页面锁定/更新

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1063BD20 | db1_page_may_be_updated | 检查页是否可更新 |
| 0x1063BE60 | db1_unlock_page | 解锁页 |
| 0x1063BFA0 | db1_lock_page | 锁定页 |
| 0x1063C0A0 | db1_set_update_flag_on_page | 设置页更新标志 |

### 页面释放

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1063C540 | db1_reassign_page | 重新分配页 |
| 0x1063C6D0 | db1_lose_pages | 释放多个页 |
| 0x1063C830 | db1_lose_given_page | 释放指定页 |
| 0x1063C950 | db1_lose_unlocked_pages | 释放未锁定的页 |
| 0x1063CAA0 | db1_lose_claim_pages_for_db | 释放数据库的声明页 |

### 令牌管理

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1063CD80 | db1_set_token | 设置页令牌 |
| 0x1063D060 | db1_get_token | 获取页令牌 |
| 0x1063D8C0 | db1_unset_token | 取消设置页令牌 |
| 0x1063DA80 | db1_unset_tokens_for_db | 取消设置数据库的所有页令牌 |

### 临时页与标记

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1063CC40 | db1_init_temp_pages | 初始化临时页 |
| 0x1063E870 | db1_set_temp_page_for_mark | 为标记设置临时页 |
| 0x1063DFB0 | db1_return_last_page | 返回最后一页 |

### 模式与标志

| 函数地址 | 函数名 | 说明 |
|----------|--------|------|
| 0x1063D190 | db1_file_is_open_as_db | 检查文件是否作为数据库打开 |
| 0x1063D290 | db1_get_cache_file_details | 获取缓存文件详情 |
| 0x1063D3A0 | db1_switch_mode | 切换数据库模式 |
| 0x1063D5C0 | db1_refresh | 刷新页缓存 |
| 0x1063DC30 | db1_set_update_db_direct | 设置直接更新数据库模式 |
| 0x1063DDB0 | db1_suspend_update_db_direct | 挂起直接更新数据库模式 |
| 0x1063DEB0 | db1_restore_update_db_direct | 恢复直接更新数据库模式 |
| 0x10641760 | db1_unset_update_db_direct | 取消直接更新数据库模式 |
| 0x1063E510 | db1_set_keep_incore_flag | 设置保持内存标志 |
| 0x1063E5E0 | db1_unset_keep_incore_flag | 取消保持内存标志 |
| 0x1063E6B0 | db1_get_overwrite_dbno | 获取覆写数据库编号 |
| 0x1063E770 | db1_db_pages_are_updated | 检查数据库页是否已更新 |

---

## 关键概念说明

| 概念 | 说明 |
|------|------|
| **CE (Current Element)** | 当前元素，数据库引擎维护的当前操作目标元素 |
| **Extract** | 数据库的子集/提取，用于多用户协作编辑 |
| **Master** | 主数据库，包含完整数据 |
| **Session** | 会话，记录一次编辑操作的变更 |
| **Claim/Release** | 声明/释放机制，用于多写模式下的元素锁定 |
| **Flush** | 将本地extract的变更推送到主数据库 |
| **Refresh** | 从主数据库拉取最新变更到本地extract |
| **Page** | 数据库的物理存储单元，固定大小的数据块 |
| **Page Frame** | 内存中的页缓存槽位 |
| **Token** | 页访问令牌，用于标识页的访问上下文 |
| **B-Tree** | B树索引结构，用于高效查找表条目 |
| **Bucket** | 桶，用于管理元素引用的哈希结构 |
| **DBNo** | 数据库编号，标识特定的数据库实例 |
| **ExtRef** | 外部引用，跨数据库的元素引用 |
| **DA (Data Attribute)** | 数据属性，元素上附加的数据 |
