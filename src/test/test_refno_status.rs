//! 测试参考号状态判断功能
//!
//! 本测试模块验证`get_refno_status`和`get_refno_operation_status`方法是否能正确判断一个参考号的状态：
//! - 新增(Add)：参考号只在一个会话中出现，或者只有最新的会话中存在
//! - 修改(Modified)：参考号在多个会话中出现，且内容有变化
//! - 删除(Deleted)：参考号在历史会话中存在，但在最新会话中不存在

use crate::io::{EleOperationDetail, PdmsIO};
use crate::test::resolve_test_db_path;
use aios_core::pdms_types::{EleOperation, RefU64};
use std::time::Instant;
