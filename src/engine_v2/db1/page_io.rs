use crate::engine_v2::io_layer::{FileHandle, RetryPolicy};
use crate::engine_v2::types::{DbResult, PageDescriptor, PageId, PageSize, PageType};

/// 页面物理 I/O (替代 db1_read_page / db1_write_page)
pub struct PageIO {
    retry: RetryPolicy,
}

impl PageIO {
    pub fn new() -> Self {
        Self { retry: RetryPolicy::default() }
    }

    pub fn with_retry(retry: RetryPolicy) -> Self {
        Self { retry }
    }

    /// 从磁盘读取单页并构造 PageDescriptor
    pub fn read_page(
        &self,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        page_no: u32,
    ) -> DbResult<PageDescriptor> {
        let mut buf = vec![0u8; handle.page_size()];
        self.retry.read_page_with_retry(handle, page_no, &mut buf)?;
        let id = PageId::new(dbno, page_no, extent);
        Ok(PageDescriptor::new(id, buf))
    }

    /// 批量预读连续页面
    pub fn prefetch_pages(
        &self,
        handle: &mut FileHandle,
        dbno: u32,
        extent: u32,
        start_page: u32,
        count: u32,
    ) -> DbResult<Vec<PageDescriptor>> {
        let ps = handle.page_size();
        let mut bulk = vec![0u8; count as usize * ps];
        let actual = handle.read_pages(start_page, count, &mut bulk)?;
        let mut result = Vec::with_capacity(actual as usize);
        for i in 0..actual {
            let offset = i as usize * ps;
            let data = bulk[offset..offset + ps].to_vec();
            let id = PageId::new(dbno, start_page + i, extent);
            result.push(PageDescriptor::new(id, data));
        }
        Ok(result)
    }

    /// 将脏页写回磁盘
    pub fn write_page(
        &self,
        handle: &mut FileHandle,
        desc: &PageDescriptor,
    ) -> DbResult<()> {
        handle.write_page(desc.id.page_no, &desc.data)
    }
}
