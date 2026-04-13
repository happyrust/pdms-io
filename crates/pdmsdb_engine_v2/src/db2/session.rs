use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::ops::RangeInclusive;

use chrono::{Datelike, Timelike};

use crate::core::{EngineError, PageId, SessionSnapshot};
use crate::db1::PageStore;
use crate::db2::HeaderView;

#[derive(Debug, Clone)]
pub struct SessionPageView {
    pub page_type: u32,
    pub last_ses_pageno: i32,
    pub last_ses_extno: u32,
    pub sesno: u32,
    pub end_pgno: u32,
    pub end_extno: u32,
    pub index_root_pageno: u32,
    pub index_root_extno: u32,
    pub claim_pageno: u32,
    pub claim_extno: u32,
}

impl SessionPageView {
    pub fn from_page(page: &[u8]) -> Result<Self, EngineError> {
        if page.len() < 0x34 {
            return Err(EngineError::Format("session 页面太短".into()));
        }

        let read_u32 = |start: usize| -> u32 {
            u32::from_be_bytes(page[start..start + 4].try_into().unwrap())
        };
        let read_i32 = |start: usize| -> i32 {
            i32::from_be_bytes(page[start..start + 4].try_into().unwrap())
        };

        let page_type = read_u32(0x00);
        if page_type != 3 {
            return Err(EngineError::Format(format!(
                "期望 session page_type=3，实际 {}",
                page_type
            )));
        }

        Ok(Self {
            page_type,
            last_ses_pageno: read_i32(0x04),
            last_ses_extno: read_u32(0x08),
            sesno: read_u32(0x0C),
            end_pgno: read_u32(0x14),
            end_extno: read_u32(0x18),
            index_root_pageno: read_u32(0x1C),
            index_root_extno: read_u32(0x20),
            claim_pageno: read_u32(0x24),
            claim_extno: read_u32(0x28),
        })
    }
}

pub struct SessionChain;

#[derive(Debug, Clone)]
pub struct SessionBuilderV2 {
    pub sesno: u32,
    pub last_ses_pageno: u32,
    pub last_ses_extno: u32,
    pub end_pgno: u32,
    pub end_extno: u32,
    pub index_root_pageno: u32,
    pub index_root_extno: u32,
    pub claim_pageno: u32,
    pub claim_extno: u32,
    pub unknown_1: i32,
    pub unknown_2: i32,
    pub computer_name: String,
    pub comments: String,
}

impl SessionBuilderV2 {
    pub fn new(sesno: u32, last_ses_pageno: u32) -> Self {
        Self {
            sesno,
            last_ses_pageno,
            last_ses_extno: 1,
            end_pgno: 0,
            end_extno: 1,
            index_root_pageno: 0,
            index_root_extno: 1,
            claim_pageno: 0,
            claim_extno: 1,
            unknown_1: 0,
            unknown_2: 0,
            computer_name: String::new(),
            comments: String::new(),
        }
    }

    pub fn end_page(mut self, page: PageId) -> Self {
        self.end_pgno = page.page_no;
        self.end_extno = page.ext_no;
        self
    }

    pub fn index_root(mut self, page: PageId) -> Self {
        self.index_root_pageno = page.page_no;
        self.index_root_extno = page.ext_no;
        self
    }

    pub fn claim_root(mut self, page: Option<PageId>) -> Self {
        if let Some(page) = page {
            self.claim_pageno = page.page_no;
            self.claim_extno = page.ext_no;
        }
        self
    }

    pub fn computer_name(mut self, name: impl Into<String>) -> Self {
        self.computer_name = name.into();
        self
    }

    pub fn comments(mut self, comments: impl Into<String>) -> Self {
        self.comments = comments.into();
        self
    }

    pub fn build(&self, page_size: usize) -> Vec<u8> {
        let mut data = vec![0u8; page_size];
        data[0..4].copy_from_slice(&3u32.to_be_bytes());
        data[4..8].copy_from_slice(&self.last_ses_pageno.to_be_bytes());
        data[8..12].copy_from_slice(&self.last_ses_extno.to_be_bytes());
        data[12..16].copy_from_slice(&self.sesno.to_be_bytes());
        data[16..20].copy_from_slice(&(-1i32).to_be_bytes());
        data[20..24].copy_from_slice(&self.end_pgno.to_be_bytes());
        data[24..28].copy_from_slice(&self.end_extno.to_be_bytes());
        data[28..32].copy_from_slice(&self.index_root_pageno.to_be_bytes());
        data[32..36].copy_from_slice(&self.index_root_extno.to_be_bytes());
        data[36..40].copy_from_slice(&self.claim_pageno.to_be_bytes());
        data[40..44].copy_from_slice(&self.claim_extno.to_be_bytes());
        data[44..48].copy_from_slice(&self.unknown_1.to_be_bytes());
        data[48..52].copy_from_slice(&self.unknown_2.to_be_bytes());

        let now = chrono::Local::now();
        let year = now.year() as u32;
        let month = now.month() as u32;
        let hours = now.day() * 24 + now.hour();
        let seconds = now.minute() * 60 + now.second();
        data[52..56].copy_from_slice(&year.to_be_bytes());
        data[56..60].copy_from_slice(&month.to_be_bytes());
        data[60..64].copy_from_slice(&hours.to_be_bytes());
        data[64..68].copy_from_slice(&seconds.to_be_bytes());

        let name_bytes = self.computer_name.as_bytes();
        let name_words = ((name_bytes.len() + 3) / 4).min(9);
        data[0x78..0x7C].copy_from_slice(&(name_words as u32).to_be_bytes());
        let name_start = 0x7C;
        let name_len = std::cmp::min(name_bytes.len(), name_words * 4);
        if name_len > 0 {
            data[name_start..name_start + name_len].copy_from_slice(&name_bytes[..name_len]);
        }

        let comments_start = 0x7C + 36;
        let comment_bytes = self.comments.as_bytes();
        let comments_words = ((comment_bytes.len() + 3) / 4).min(1024);
        if comments_start + 4 <= page_size {
            data[comments_start..comments_start + 4]
                .copy_from_slice(&(comments_words as u32).to_be_bytes());
            let max_payload = page_size - comments_start - 4;
            let comments_len = std::cmp::min(comments_words * 4, max_payload);
            let copy_len = std::cmp::min(comment_bytes.len(), comments_len);
            if copy_len > 0 {
                data[comments_start + 4..comments_start + 4 + copy_len]
                    .copy_from_slice(&comment_bytes[..copy_len]);
            }
        }

        data
    }
}

impl SessionChain {
    pub fn walk_latest_backwards(
        file: &mut File,
        store: &mut PageStore,
        header: &HeaderView,
    ) -> Result<Vec<SessionSnapshot>, EngineError> {
        let mut sessions = Vec::new();
        let mut seen = HashSet::new();
        let mut current = header.latest_ses_pgno;

        while current != 0 && seen.insert(current) {
            let page_id = PageId {
                ext_no: 1,
                page_no: current,
            };
            let page = store.read_page(file, page_id)?;
            let session = SessionPageView::from_page(&page)?;
            sessions.push(SessionSnapshot {
                sesno: session.sesno,
                page: page_id,
                last_session: (session.last_ses_pageno > 0).then_some(PageId {
                    ext_no: session.last_ses_extno.max(1),
                    page_no: session.last_ses_pageno as u32,
                }),
                end_page: PageId {
                    ext_no: session.end_extno.max(1),
                    page_no: session.end_pgno,
                },
                index_root: PageId {
                    ext_no: session.index_root_extno.max(1),
                    page_no: session.index_root_pageno,
                },
                claim_root: (session.claim_pageno != 0).then_some(PageId {
                    ext_no: session.claim_extno.max(1),
                    page_no: session.claim_pageno,
                }),
            });

            if session.last_ses_pageno <= 0 {
                break;
            }
            current = session.last_ses_pageno as u32;
        }

        sessions.reverse();
        Ok(sessions)
    }

    pub fn build_session_ranges(
        sessions: &[SessionSnapshot],
    ) -> BTreeMap<u32, RangeInclusive<u32>> {
        let mut ranges = BTreeMap::new();
        let mut prev_end = 0u32;

        for session in sessions {
            let start = if prev_end == 0 {
                0
            } else {
                prev_end.saturating_add(1)
            };
            let end = session.end_page.page_no.max(start);
            ranges.insert(session.sesno, start..=end);
            prev_end = end;
        }

        ranges
    }
}
