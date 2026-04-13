use crate::core::EngineError;

#[derive(Debug, Clone)]
pub struct ExtractRecord {
    pub db_no: i32,
    pub user_name: String,
    pub timestamp: u32,
}

pub struct ExtractManager {
    records: Vec<ExtractRecord>,
}

impl ExtractManager {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn insert(&mut self, record: ExtractRecord) -> Result<(), EngineError> {
        if self.records.iter().any(|r| r.db_no == record.db_no) {
            return Err(EngineError::InvalidState(format!(
                "db_no {} 已存在 Extract 记录",
                record.db_no
            )));
        }
        self.records.push(record);
        Ok(())
    }

    pub fn remove(&mut self, db_no: i32) -> Result<ExtractRecord, EngineError> {
        let idx = self
            .records
            .iter()
            .position(|r| r.db_no == db_no)
            .ok_or_else(|| {
                EngineError::NotFound(format!("db_no {} 不存在 Extract 记录", db_no))
            })?;
        Ok(self.records.remove(idx))
    }

    pub fn find(&self, db_no: i32) -> Option<&ExtractRecord> {
        self.records.iter().find(|r| r.db_no == db_no)
    }

    pub fn list(&self) -> &[ExtractRecord] {
        &self.records
    }

    pub fn is_extracted(&self, db_no: i32) -> bool {
        self.records.iter().any(|r| r.db_no == db_no)
    }

    pub fn count(&self) -> usize {
        self.records.len()
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }
}

impl Default for ExtractManager {
    fn default() -> Self {
        Self::new()
    }
}
