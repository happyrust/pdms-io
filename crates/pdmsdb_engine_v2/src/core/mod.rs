use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use crate::db1::PageStore;
use crate::db2::HeaderView;
use crate::db5::mark::TransactionManager;

mod error;

pub use error::EngineError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId {
    pub ext_no: u32,
    pub page_no: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordLoc {
    pub ext_no: u32,
    pub page_no: u32,
    pub byte_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordWriteResult {
    pub start: RecordLoc,
    pub total_len: usize,
    pub end_page: PageId,
    pub pages_used: usize,
}

#[derive(Debug, Clone)]
pub struct CommitSessionRequest {
    pub sesno: u32,
    pub last_session: Option<PageId>,
    pub end_page: PageId,
    pub index_root: PageId,
    pub claim_root: Option<PageId>,
    pub computer_name: String,
    pub comments: String,
}

#[derive(Debug, Clone)]
pub(crate) struct WriteSessionContext {
    sesno: u32,
    last_session: Option<PageId>,
    claim_root: Option<PageId>,
    end_page: Option<PageId>,
    index_root: Option<PageId>,
    index_root_initialized: bool,
    inserted_refnos: BTreeSet<RefNo>,
    computer_name: String,
    comments: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RefNo(u64);

impl RefNo {
    pub fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub fn from_parts(hi: u32, lo: u32) -> Self {
        Self(((hi as u64) << 32) | lo as u64)
    }

    pub fn hi(self) -> u32 {
        (self.0 >> 32) as u32
    }

    pub fn lo(self) -> u32 {
        self.0 as u32
    }

    pub fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchHit {
    pub sesno: u32,
    pub loc: RecordLoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub sesno: u32,
    pub page: PageId,
    pub last_session: Option<PageId>,
    pub end_page: PageId,
    pub index_root: PageId,
    pub claim_root: Option<PageId>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EngineOptions {
    pub page_size_hint: Option<usize>,
    pub prefetch_pages: usize,
}

pub struct DbHandle {
    pub(crate) path: PathBuf,
    pub(crate) file: RefCell<File>,
    pub(crate) page_store: RefCell<PageStore>,
    pub(crate) header: HeaderView,
    pub(crate) sessions: Vec<SessionSnapshot>,
    pub(crate) session_ranges: BTreeMap<u32, RangeInclusive<u32>>,
    pub(crate) write_context: RefCell<Option<WriteSessionContext>>,
    pub(crate) latest_session_cache: RefCell<Option<SessionSnapshot>>,
    pub(crate) latest_session_range: RefCell<Option<(u32, RangeInclusive<u32>)>>,
    pub(crate) current_index_root: RefCell<Option<PageId>>,
    pub(crate) transaction_manager: RefCell<TransactionManager>,
}

pub struct EngineV2;

impl EngineV2 {
    pub fn open_read(
        path: impl AsRef<Path>,
        options: EngineOptions,
    ) -> Result<DbHandle, EngineError> {
        crate::db5::open_read_db(path.as_ref(), options)
    }

    pub fn open_write(
        path: impl AsRef<Path>,
        options: EngineOptions,
    ) -> Result<DbHandle, EngineError> {
        crate::db5::open_write_db(path.as_ref(), options)
    }
}

impl DbHandle {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn page_size(&self) -> usize {
        self.page_store.borrow().page_size()
    }

    pub fn header(&self) -> &HeaderView {
        &self.header
    }

    pub fn latest_session(&self) -> Result<SessionSnapshot, EngineError> {
        if let Some(snapshot) = self.latest_session_cache.borrow().clone() {
            return Ok(snapshot);
        }

        self.sessions
            .last()
            .cloned()
            .ok_or_else(|| EngineError::Format("未发现任何 session".into()))
    }

    pub fn sessions(&self) -> Vec<SessionSnapshot> {
        let mut out = self.sessions.clone();
        if let Some(snapshot) = self.latest_session_cache.borrow().clone() {
            if out.last().map(|item| item.page.page_no) != Some(snapshot.page.page_no) {
                out.push(snapshot);
            }
        }
        out
    }

    pub fn read_page(&self, page_id: PageId) -> Result<Vec<u8>, EngineError> {
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        store.read_page(&mut file, page_id)
    }

    pub fn find_refno(
        &self,
        refno: RefNo,
        sesno: Option<u32>,
    ) -> Result<Option<SearchHit>, EngineError> {
        let root = match sesno {
            Some(target) => self
                .sessions()
                .iter()
                .find(|session| session.sesno == target)
                .map(|session| session.index_root)
                .ok_or_else(|| EngineError::NotFound(format!("session {} 不存在", target)))?,
            None => self.latest_session()?.index_root,
        };

        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        let loc = crate::db3::search_refno(&mut file, &mut store, root, refno)?;
        Ok(loc.map(|loc| SearchHit {
            sesno: self.sesno_for_page(loc.page_no).unwrap_or_default(),
            loc,
        }))
    }

    pub fn find_refno_from_root(
        &self,
        root: PageId,
        refno: RefNo,
    ) -> Result<Option<RecordLoc>, EngineError> {
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        crate::db3::search_refno(&mut file, &mut store, root, refno)
    }

    pub fn read_record(&self, loc: RecordLoc) -> Result<Vec<u8>, EngineError> {
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        crate::db4::read_record_from_loc(&mut file, &mut store, loc)
    }

    pub fn allocate_page(&self, ext_no: u32) -> Result<PageId, EngineError> {
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        store.allocate_page(&mut file, ext_no)
    }

    pub fn write_page(&self, page_id: PageId, data: &[u8]) -> Result<(), EngineError> {
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        store.write_page(&mut file, page_id, data)
    }

    pub fn flush_dirty(&self) -> Result<usize, EngineError> {
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        store.flush_dirty(&mut file)
    }

    pub fn write_record(
        &self,
        ext_no: u32,
        record: &[u8],
    ) -> Result<RecordWriteResult, EngineError> {
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        let result = crate::db4::write_record(&mut file, &mut store, ext_no, record)?;
        if let Some(context) = self.write_context.borrow_mut().as_mut() {
            context.claim_root.get_or_insert(PageId {
                ext_no: result.start.ext_no,
                page_no: result.start.page_no,
            });
            context.end_page = Some(result.end_page);
        }
        Ok(result)
    }

    pub fn current_index_root(&self) -> Option<PageId> {
        *self.current_index_root.borrow()
    }

    pub fn commit_session(
        &self,
        request: CommitSessionRequest,
    ) -> Result<SessionSnapshot, EngineError> {
        let page_size = self.page_size();
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        crate::db5::commit_session(&mut file, &mut store, page_size, request)
    }

    pub fn begin_write_session(
        &self,
        sesno: u32,
        computer_name: impl Into<String>,
        comments: impl Into<String>,
    ) -> Result<(), EngineError> {
        let mut context = self.write_context.borrow_mut();
        if context.is_some() {
            return Err(EngineError::InvalidState(
                "已有活动写会话，不能重复 begin_write_session".into(),
            ));
        }
        *context = Some(WriteSessionContext {
            sesno,
            last_session: self.latest_session().ok().map(|session| session.page),
            claim_root: None,
            end_page: None,
            index_root: None,
            index_root_initialized: false,
            inserted_refnos: BTreeSet::new(),
            computer_name: computer_name.into(),
            comments: comments.into(),
        });
        Ok(())
    }

    pub fn ensure_index_root(&self) -> Result<PageId, EngineError> {
        if let Some(root) = *self.current_index_root.borrow() {
            return Ok(root);
        }

        if let Some(root) = self
            .write_context
            .borrow()
            .as_ref()
            .and_then(|context| context.index_root)
        {
            *self.current_index_root.borrow_mut() = Some(root);
            return Ok(root);
        }

        if let Ok(latest) = self.latest_session() {
            let root = latest.index_root;
            if root.page_no != 0 {
                *self.current_index_root.borrow_mut() = Some(root);
                if let Some(context) = self.write_context.borrow_mut().as_mut() {
                    context.index_root = Some(root);
                    context.index_root_initialized = true;
                }
                return Ok(root);
            }
        }

        let root = self.allocate_page(1)?;
        self.write_empty_index_root(root)?;
        Ok(root)
    }

    pub fn create_empty_database_layout(&self) -> Result<PageId, EngineError> {
        self.ensure_index_root()
    }

    pub fn insert_record(
        &self,
        refno: RefNo,
        record: &[u8],
    ) -> Result<RecordWriteResult, EngineError> {
        if self.write_context.borrow().is_none() {
            return Err(EngineError::InvalidState(
                "调用 insert_record 前必须先 begin_write_session".into(),
            ));
        }

        let root = self.ensure_index_root()?;
        let result = self.write_record(1, record)?;
        let updated_root = self.upsert_refno(root, refno, result.start)?;
        *self.current_index_root.borrow_mut() = Some(updated_root);

        if let Some(context) = self.write_context.borrow_mut().as_mut() {
            context.index_root = Some(updated_root);
            context.index_root_initialized = true;
            context.inserted_refnos.insert(refno);
        }

        Ok(result)
    }

    pub fn commit_current_session(&self) -> Result<SessionSnapshot, EngineError> {
        let context = self
            .write_context
            .borrow_mut()
            .take()
            .ok_or_else(|| EngineError::InvalidState("当前无活动写会话".into()))?;

        let index_root = context.index_root.ok_or_else(|| {
            EngineError::InvalidState("写会话尚未建立 index_root，无法提交".into())
        })?;
        let end_page = context.end_page.ok_or_else(|| {
            EngineError::InvalidState("写会话尚未写入任何 record，无法提交".into())
        })?;

        let request = CommitSessionRequest {
            sesno: context.sesno,
            last_session: context.last_session,
            end_page,
            index_root,
            claim_root: context.claim_root,
            computer_name: context.computer_name,
            comments: context.comments,
        };

        let request_backup = request.clone();
        match self.commit_session(request) {
            Ok(snapshot) => {
                let range_start = self
                    .latest_session()
                    .ok()
                    .map(|session| session.end_page.page_no.saturating_add(1))
                    .unwrap_or(0);
                let range = range_start..=snapshot.end_page.page_no.max(range_start);
                *self.latest_session_range.borrow_mut() = Some((snapshot.sesno, range));
                *self.latest_session_cache.borrow_mut() = Some(snapshot.clone());
                *self.current_index_root.borrow_mut() = Some(snapshot.index_root);
                Ok(snapshot)
            }
            Err(err) => {
                *self.write_context.borrow_mut() = Some(WriteSessionContext {
                    sesno: request_backup.sesno,
                    last_session: request_backup.last_session,
                    claim_root: request_backup.claim_root,
                    end_page: Some(request_backup.end_page),
                    index_root: Some(request_backup.index_root),
                    index_root_initialized: true,
                    inserted_refnos: BTreeSet::new(),
                    computer_name: request_backup.computer_name,
                    comments: request_backup.comments,
                });
                Err(err)
            }
        }
    }

    pub fn save_work(&self) -> Result<SessionSnapshot, EngineError> {
        self.commit_current_session()
    }

    pub fn write_empty_index_root(&self, root: PageId) -> Result<(), EngineError> {
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        let result = crate::db3::write_empty_root(&mut file, &mut store, root);
        if result.is_ok() {
            if let Some(context) = self.write_context.borrow_mut().as_mut() {
                context.index_root = Some(root);
                context.index_root_initialized = true;
            }
            *self.current_index_root.borrow_mut() = Some(root);
        }
        result
    }

    pub fn upsert_refno(
        &self,
        root: PageId,
        refno: RefNo,
        loc: RecordLoc,
    ) -> Result<PageId, EngineError> {
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        let root = crate::db3::upsert_refno(&mut file, &mut store, root, refno, loc)?;
        if let Some(context) = self.write_context.borrow_mut().as_mut() {
            context.index_root = Some(root);
            context.inserted_refnos.insert(refno);
        }
        *self.current_index_root.borrow_mut() = Some(root);
        Ok(root)
    }

    pub fn set_mark(&self) -> Result<u32, EngineError> {
        let session = self.latest_session()?;
        let index_root = self
            .current_index_root
            .borrow()
            .unwrap_or(session.index_root);
        self.page_store.borrow_mut().snapshot_cow();
        let mark_id = self
            .transaction_manager
            .borrow_mut()
            .set_mark(session, index_root);
        Ok(mark_id)
    }

    pub fn undo_to_mark(&self, mark_id: u32) -> Result<(), EngineError> {
        let mark = self
            .transaction_manager
            .borrow_mut()
            .undo_to_mark(mark_id)?;
        self.page_store.borrow_mut().rollback_cow();
        *self.current_index_root.borrow_mut() = Some(mark.index_root_at_mark);
        *self.latest_session_cache.borrow_mut() = Some(mark.session_at_mark);
        Ok(())
    }

    pub fn update_element(
        &self,
        refno: RefNo,
        new_record: &[u8],
    ) -> Result<RecordWriteResult, EngineError> {
        if self.write_context.borrow().is_none() {
            return Err(EngineError::InvalidState(
                "调用 update_element 前必须先 begin_write_session".into(),
            ));
        }

        let root = self.ensure_index_root()?;
        let result = self.write_record(1, new_record)?;
        let updated_root = self.upsert_refno(root, refno, result.start)?;
        *self.current_index_root.borrow_mut() = Some(updated_root);

        if let Some(context) = self.write_context.borrow_mut().as_mut() {
            context.index_root = Some(updated_root);
            context.index_root_initialized = true;
        }

        Ok(result)
    }

    pub fn delete_element(&self, refno: RefNo) -> Result<bool, EngineError> {
        if self.write_context.borrow().is_none() {
            return Err(EngineError::InvalidState(
                "调用 delete_element 前必须先 begin_write_session".into(),
            ));
        }

        let root = self.ensure_index_root()?;
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        let deleted = crate::db3::delete_refno(&mut file, &mut store, root, refno)?;
        Ok(deleted)
    }

    pub fn iter_all_refnos(&self) -> Result<Vec<crate::db3::IndexIteratorEntry>, EngineError> {
        let root = self.latest_session()?.index_root;
        let mut file = self.file.borrow_mut();
        let mut store = self.page_store.borrow_mut();
        crate::db3::scan_all_entries(&mut file, &mut store, root)
    }

    pub fn sesno_for_page(&self, page_no: u32) -> Option<u32> {
        if let Some((sesno, range)) = self.latest_session_range.borrow().as_ref() {
            if range.contains(&page_no) {
                return Some(*sesno);
            }
        }
        self.session_ranges.iter().find_map(|(sesno, range)| {
            if range.contains(&page_no) {
                Some(*sesno)
            } else {
                None
            }
        })
    }
}
