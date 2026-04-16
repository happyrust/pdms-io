use crate::engine_v2::db1::PageCache;
use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;
use super::btree::{BTreeNode, START_MARKER, max_entries_per_page};

/// B-树节点分裂 (对齐 FHSPLT)
pub struct BTreeSplit;

impl BTreeSplit {
    /// 叶子页分裂
    ///
    /// 将满叶子节点中点拆分为两个节点，返回 (promoted_entry, new_page_no)。
    /// promoted_entry 的 refno 是右半部分的最小 key，page_no 指向新页。
    pub fn split_leaf(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        old_page_no: u32,
        node: BTreeNode,
        new_entry: IndexEntry,
        page_size: usize,
    ) -> DbResult<(IndexEntry, u32)> {
        let mut all = node.entries.clone();
        let pos = all.partition_point(|e| {
            (e.refno.hi, e.refno.lo) < (new_entry.refno.hi, new_entry.refno.lo)
        });
        all.insert(pos, new_entry);

        let mid = all.len() / 2;
        let left = &all[..mid];
        let right = &all[mid..];

        Self::write_entries_to_page(cache, handle, dbno, extent, old_page_no, left, 0, page_size)?;

        let new_page_no = handle.total_pages();
        let mut new_data = vec![0u8; page_size];
        Self::init_leaf_page(&mut new_data, right);
        handle.write_page(new_page_no, &new_data)?;
        handle.refresh_len()?;

        let promoted = IndexEntry {
            refno: right[0].refno,
            page_no: new_page_no,
            packed: 0,
        };

        Ok((promoted, new_page_no))
    }

    /// 内部节点分裂
    pub fn split_internal(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        old_page_no: u32,
        node: BTreeNode,
        new_entry: IndexEntry,
        page_size: usize,
    ) -> DbResult<(IndexEntry, u32)> {
        let mut all = node.entries.clone();
        let pos = all.partition_point(|e| {
            if e.refno == START_MARKER { return true; }
            (e.refno.hi, e.refno.lo) < (new_entry.refno.hi, new_entry.refno.lo)
        });
        all.insert(pos, new_entry);

        let mid = all.len() / 2;
        let left = &all[..mid];
        let right = &all[mid..];

        let level = node.header.level;
        Self::write_entries_to_page(cache, handle, dbno, extent, old_page_no, left, level, page_size)?;

        let new_page_no = handle.total_pages();
        let mut new_data = vec![0u8; page_size];
        Self::init_internal_page(&mut new_data, right, level);
        handle.write_page(new_page_no, &new_data)?;
        handle.refresh_len()?;

        let promoted_refno = if let Some(first_valid) = right.iter().find(|e| e.refno != START_MARKER) {
            first_valid.refno
        } else {
            right[0].refno
        };

        let promoted = IndexEntry {
            refno: promoted_refno,
            page_no: new_page_no,
            packed: 0,
        };

        Ok((promoted, new_page_no))
    }

    /// 根节点分裂：创建新根，树高+1
    pub fn split_root(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        old_root: u32,
        promoted: IndexEntry,
        _new_page: u32,
        page_size: usize,
    ) -> DbResult<u32> {
        let new_root_pgno = handle.total_pages();
        let mut root_data = vec![0u8; page_size];

        let page_data = cache.get_page(handle, dbno, extent, old_root)?;
        let old_node = BTreeNode::from_page_data(old_root, page_data);
        let old_id = PageId::new(dbno, old_root, extent);
        cache.unlock_page(&old_id);

        let new_level = old_node.header.level + 1;

        let start_entry = IndexEntry {
            refno: START_MARKER,
            page_no: old_root,
            packed: 0,
        };

        let entries = [start_entry, promoted];
        Self::init_internal_page(&mut root_data, &entries, new_level);

        handle.write_page(new_root_pgno, &root_data)?;
        handle.refresh_len()?;

        Ok(new_root_pgno)
    }

    fn write_entries_to_page(
        cache: &mut PageCache,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        page_no: u32,
        entries: &[IndexEntry],
        level: u32,
        _page_size: usize,
    ) -> DbResult<()> {
        let data = cache.get_page_mut(handle, dbno, extent, page_no)?;

        data[8..12].copy_from_slice(&level.to_be_bytes());
        let count = entries.len() as u32;
        data[16..20].copy_from_slice(&count.to_be_bytes());

        for (i, e) in entries.iter().enumerate() {
            let off = INDEX_PAGE_HEADER_SIZE + i * IndexEntry::SIZE;
            data[off..off + IndexEntry::SIZE].copy_from_slice(&e.to_be_bytes());
        }

        Ok(())
    }

    fn init_leaf_page(data: &mut [u8], entries: &[IndexEntry]) {
        let page_type: u32 = 1;
        data[0..4].copy_from_slice(&page_type.to_be_bytes());
        data[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
        let level: u32 = 0;
        data[8..12].copy_from_slice(&level.to_be_bytes());
        let count = entries.len() as u32;
        data[16..20].copy_from_slice(&count.to_be_bytes());

        for (i, e) in entries.iter().enumerate() {
            let off = INDEX_PAGE_HEADER_SIZE + i * IndexEntry::SIZE;
            data[off..off + IndexEntry::SIZE].copy_from_slice(&e.to_be_bytes());
        }
    }

    fn init_internal_page(data: &mut [u8], entries: &[IndexEntry], level: u32) {
        let page_type: u32 = 1;
        data[0..4].copy_from_slice(&page_type.to_be_bytes());
        data[4..8].copy_from_slice(&INDEX_PAGE_NOUN.to_be_bytes());
        data[8..12].copy_from_slice(&level.to_be_bytes());
        let count = entries.len() as u32;
        data[16..20].copy_from_slice(&count.to_be_bytes());

        for (i, e) in entries.iter().enumerate() {
            let off = INDEX_PAGE_HEADER_SIZE + i * IndexEntry::SIZE;
            data[off..off + IndexEntry::SIZE].copy_from_slice(&e.to_be_bytes());
        }
    }
}
