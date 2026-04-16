use std::path::Path;

use crate::engine_v2::db1::PageCache;
use crate::engine_v2::db3::BTreeSearch;
use crate::engine_v2::db4::ce::CurrentElement;
use crate::engine_v2::io_layer::FileHandle;
use crate::engine_v2::types::*;

use super::open::DbOpen;
use super::close::DbClose;
use super::save::DbSave;

/// 数据库引擎 V2 统一入口
///
/// 整合 db1~db5 五层为单一 API，替代旧 PdmsIO。
pub struct Database {
    pub handle: FileHandle,
    pub header: DbHeader,
    pub cache: PageCache,
    pub ce: CurrentElement,
    dbno: u32,
    extent: u32,
}

impl Database {
    /// 只读打开数据库
    pub fn open_read(path: impl AsRef<Path>, cache_size: usize) -> DbResult<Self> {
        let (handle, header) = DbOpen::open_read(&path)?;
        let dbno = header.db_num as u32;
        let extent = header.ext_no;
        Ok(Self {
            handle,
            header,
            cache: PageCache::new(cache_size),
            ce: CurrentElement::new(),
            dbno,
            extent,
        })
    }

    /// 读写打开数据库
    pub fn open_write(path: impl AsRef<Path>, cache_size: usize) -> DbResult<Self> {
        let (handle, header) = DbOpen::open_write(&path)?;
        let dbno = header.db_num as u32;
        let extent = header.ext_no;
        Ok(Self {
            handle,
            header,
            cache: PageCache::new(cache_size),
            ce: CurrentElement::new(),
            dbno,
            extent,
        })
    }

    /// B-树搜索定位元素
    pub fn find_element(&mut self, refno: RefNo, root_pgno: u32) -> DbResult<Option<RefnoDataLoc>> {
        BTreeSearch::find(
            &mut self.cache, &mut self.handle,
            self.dbno, self.extent, root_pgno, refno,
        )
    }

    /// 导航到指定元素 (opcode 108)
    pub fn go_to_element(&mut self, refno: RefNo, root_pgno: u32) -> DbResult<bool> {
        if let Some(loc) = self.find_element(refno, root_pgno)? {
            self.ce.go_to(refno, loc.dbno, loc.page_no, loc.offset);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 读取页面数据
    pub fn get_page(&mut self, page_no: u32) -> DbResult<&[u8]> {
        self.cache.get_page(&mut self.handle, self.dbno, self.extent, page_no)
    }

    /// 保存所有修改
    pub fn save(&mut self) -> DbResult<u32> {
        DbSave::save_work(&mut self.cache, &mut self.handle, &mut self.header)
    }

    /// 关闭数据库
    pub fn close(mut self) -> DbResult<()> {
        DbClose::close(&mut self.cache, &mut self.handle)
    }

    pub fn dbno(&self) -> u32 { self.dbno }
    pub fn extent(&self) -> u32 { self.extent }
    pub fn page_size(&self) -> usize { self.handle.page_size() }
}
