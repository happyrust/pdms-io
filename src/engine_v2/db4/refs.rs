use super::attrs::{AttrReader, AttrWriter};
use crate::engine_v2::types::RefNo;

/// 引用关系管理 (对齐 db4_insert_ref / db4_remove_ref)
///
/// 维护 FOWN/LOWN/PREX/NEXX 四向链表：
/// - FOWN: 第一个子元素 (First Owner)
/// - LOWN: 最后一个子元素 (Last Owner)
/// - PREX: 前一个兄弟 (Previous)
/// - NEXX: 下一个兄弟 (Next)
pub struct RefManager;

/// 固定属性偏移 (相对于元素记录头部之后)
pub const FOWN_OFFSET: usize = 0;
pub const LOWN_OFFSET: usize = 8;
pub const PREX_OFFSET: usize = 16;
pub const NEXX_OFFSET: usize = 24;

impl RefManager {
    /// 读取 FOWN (第一个子元素)
    pub fn get_fown(record_data: &[u8], attrs_start: usize) -> RefNo {
        AttrReader::get_reference(record_data, attrs_start + FOWN_OFFSET)
    }

    /// 读取 LOWN (最后一个子元素)
    pub fn get_lown(record_data: &[u8], attrs_start: usize) -> RefNo {
        AttrReader::get_reference(record_data, attrs_start + LOWN_OFFSET)
    }

    /// 读取 PREX (前一个兄弟)
    pub fn get_prex(record_data: &[u8], attrs_start: usize) -> RefNo {
        AttrReader::get_reference(record_data, attrs_start + PREX_OFFSET)
    }

    /// 读取 NEXX (下一个兄弟)
    pub fn get_nexx(record_data: &[u8], attrs_start: usize) -> RefNo {
        AttrReader::get_reference(record_data, attrs_start + NEXX_OFFSET)
    }

    /// 设置 FOWN
    pub fn set_fown(record_data: &mut [u8], attrs_start: usize, refno: RefNo) {
        AttrWriter::put_reference(record_data, attrs_start + FOWN_OFFSET, refno);
    }

    /// 设置 LOWN
    pub fn set_lown(record_data: &mut [u8], attrs_start: usize, refno: RefNo) {
        AttrWriter::put_reference(record_data, attrs_start + LOWN_OFFSET, refno);
    }

    /// 设置 PREX
    pub fn set_prex(record_data: &mut [u8], attrs_start: usize, refno: RefNo) {
        AttrWriter::put_reference(record_data, attrs_start + PREX_OFFSET, refno);
    }

    /// 设置 NEXX
    pub fn set_nexx(record_data: &mut [u8], attrs_start: usize, refno: RefNo) {
        AttrWriter::put_reference(record_data, attrs_start + NEXX_OFFSET, refno);
    }
}
