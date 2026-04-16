//! pdmsdb_adapter — 面向应用层的 PDMS 数据库读写统一接口
//!
//! 封装 pdmsdb_engine_v2::DbHandle，提供与旧 PdmsIO 兼容的高层 API。
//! 解决 pdms_io ↔ pdmsdb_engine_v2 循环依赖问题。

use std::path::{Path, PathBuf};

use pdmsdb_engine_v2::{
    DbHandle, EngineOptions, EngineV2, RecordLoc, RefNo, SearchHit, SessionSnapshot,
};
use pdmsdb_engine_v2::db4::page_layout::ElementRecordView;
use pdmsdb_engine_v2::db4::refs::parse_member_refs;

pub use pdmsdb_engine_v2::{EngineError, RecordWriteResult};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ElementData {
    pub refno_hi: u32,
    pub refno_lo: u32,
    pub noun_hash: u32,
    pub owner_hi: u32,
    pub owner_lo: u32,
    pub impl_len_words: i32,
    pub children: Vec<(u32, u32)>,
    pub has_explicit: bool,
    pub raw_len: usize,
}

pub struct PdmsReader {
    handle: DbHandle,
    path: PathBuf,
}

impl PdmsReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EngineError> {
        let path = path.as_ref().to_path_buf();
        let handle = EngineV2::open_read(&path, EngineOptions::default())?;
        Ok(Self { handle, path })
    }

    pub fn open_write(path: impl AsRef<Path>) -> Result<Self, EngineError> {
        let path = path.as_ref().to_path_buf();
        let handle = EngineV2::open_write(&path, EngineOptions::default())?;
        Ok(Self { handle, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn page_size(&self) -> usize {
        self.handle.page_size()
    }

    pub fn latest_sesno(&self) -> Result<u32, EngineError> {
        Ok(self.handle.latest_session()?.sesno)
    }

    pub fn sessions(&self) -> Vec<SessionSnapshot> {
        self.handle.sessions()
    }

    pub fn sesno_for_page(&self, page_no: u32) -> Option<u32> {
        self.handle.sesno_for_page(page_no)
    }

    pub fn search_refno(&self, refno_hi: u32, refno_lo: u32) -> Result<Option<SearchHit>, EngineError> {
        let refno = RefNo::from_parts(refno_hi, refno_lo);
        self.handle.find_refno(refno, None)
    }

    pub fn search_refno_in_session(
        &self,
        refno_hi: u32,
        refno_lo: u32,
        sesno: u32,
    ) -> Result<Option<SearchHit>, EngineError> {
        let refno = RefNo::from_parts(refno_hi, refno_lo);
        self.handle.find_refno(refno, Some(sesno))
    }

    pub fn read_raw_record(&self, loc: RecordLoc) -> Result<Vec<u8>, EngineError> {
        self.handle.read_record(loc)
    }

    pub fn parse_element(&self, refno_hi: u32, refno_lo: u32) -> Result<ElementData, EngineError> {
        let refno = RefNo::from_parts(refno_hi, refno_lo);
        let hit = self
            .handle
            .find_refno(refno, None)?
            .ok_or_else(|| EngineError::NotFound(format!("refno {}:{} 不存在", refno_hi, refno_lo)))?;
        let raw = self.handle.read_record(hit.loc)?;
        let view = ElementRecordView::from_raw(&raw)?;
        let children = parse_member_refs(&view.members_data);

        Ok(ElementData {
            refno_hi: view.refno.hi(),
            refno_lo: view.refno.lo(),
            noun_hash: view.noun_hash,
            owner_hi: view.owner.hi(),
            owner_lo: view.owner.lo(),
            impl_len_words: view.impl_len_words,
            children: children.iter().map(|r| (r.hi(), r.lo())).collect(),
            has_explicit: !view.explicit_data.is_empty(),
            raw_len: raw.len(),
        })
    }

    pub fn parse_element_view(
        &self,
        refno_hi: u32,
        refno_lo: u32,
    ) -> Result<ElementRecordView, EngineError> {
        let refno = RefNo::from_parts(refno_hi, refno_lo);
        let hit = self
            .handle
            .find_refno(refno, None)?
            .ok_or_else(|| EngineError::NotFound(format!("refno {}:{} 不存在", refno_hi, refno_lo)))?;
        let raw = self.handle.read_record(hit.loc)?;
        ElementRecordView::from_raw(&raw)
    }

    pub fn iter_all_refnos(&self) -> Result<Vec<(u32, u32, RecordLoc)>, EngineError> {
        let entries = self.handle.iter_all_refnos()?;
        Ok(entries
            .into_iter()
            .map(|e| (e.refno.hi(), e.refno.lo(), e.loc))
            .collect())
    }

    pub fn navigate_to(&self, refno_hi: u32, refno_lo: u32) -> Result<(), EngineError> {
        self.handle.navigate_to(RefNo::from_parts(refno_hi, refno_lo))
    }

    pub fn navigate_to_owner(&self) -> Result<(u32, u32), EngineError> {
        let refno = self.handle.navigate_to_owner()?;
        Ok((refno.hi(), refno.lo()))
    }

    pub fn navigate_to_first_member(&self) -> Result<(u32, u32), EngineError> {
        let refno = self.handle.navigate_to_first_member()?;
        Ok((refno.hi(), refno.lo()))
    }

    pub fn navigate_to_next_sibling(&self) -> Result<(u32, u32), EngineError> {
        let refno = self.handle.navigate_to_next_sibling()?;
        Ok((refno.hi(), refno.lo()))
    }

    pub fn navigate_back(&self) -> Result<(u32, u32), EngineError> {
        let refno = self.handle.navigate_back()?;
        Ok((refno.hi(), refno.lo()))
    }

    pub fn ce_refno(&self) -> Result<(u32, u32), EngineError> {
        let refno = self.handle.ce_refno()?;
        Ok((refno.hi(), refno.lo()))
    }

    pub fn ce_members(&self) -> Result<Vec<(u32, u32)>, EngineError> {
        Ok(self
            .handle
            .ce_members()?
            .iter()
            .map(|r| (r.hi(), r.lo()))
            .collect())
    }

    pub fn handle(&self) -> &DbHandle {
        &self.handle
    }

    pub fn close(self) -> Result<(), EngineError> {
        self.handle.close()
    }
}

pub fn to_element_json(data: &ElementData) -> serde_json::Value {
    serde_json::to_value(data).unwrap_or(serde_json::Value::Null)
}
