use crate::core::EngineError;

#[derive(Debug, Clone)]
pub struct ExtractRecord {
    pub db_no: i32,
    pub user_name: String,
    pub timestamp: u32,
    pub status: ExtractStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractStatus {
    Active,
    Released,
}

const EXTRACT_RECORD_SIZE: usize = 64;

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
        if self.records.iter().any(|r| r.db_no == record.db_no && r.status == ExtractStatus::Active) {
            return Err(EngineError::InvalidState(format!(
                "db_no {} 已存在活跃 Extract 记录",
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
            .position(|r| r.db_no == db_no && r.status == ExtractStatus::Active)
            .ok_or_else(|| {
                EngineError::NotFound(format!("db_no {} 不存在活跃 Extract 记录", db_no))
            })?;
        let mut record = self.records.remove(idx);
        record.status = ExtractStatus::Released;
        Ok(record)
    }

    pub fn find(&self, db_no: i32) -> Option<&ExtractRecord> {
        self.records
            .iter()
            .find(|r| r.db_no == db_no && r.status == ExtractStatus::Active)
    }

    pub fn list(&self) -> &[ExtractRecord] {
        &self.records
    }

    pub fn active_records(&self) -> Vec<&ExtractRecord> {
        self.records
            .iter()
            .filter(|r| r.status == ExtractStatus::Active)
            .collect()
    }

    pub fn is_extracted(&self, db_no: i32) -> bool {
        self.records
            .iter()
            .any(|r| r.db_no == db_no && r.status == ExtractStatus::Active)
    }

    pub fn count(&self) -> usize {
        self.records.len()
    }

    pub fn active_count(&self) -> usize {
        self.records
            .iter()
            .filter(|r| r.status == ExtractStatus::Active)
            .count()
    }

    pub fn clear(&mut self) {
        self.records.clear();
    }

    pub fn serialize(&self) -> Vec<u8> {
        let active: Vec<_> = self.active_records();
        let count = active.len() as u32;
        let mut data = Vec::with_capacity(4 + active.len() * EXTRACT_RECORD_SIZE);

        data.extend_from_slice(&count.to_be_bytes());

        for record in &active {
            let mut entry = vec![0u8; EXTRACT_RECORD_SIZE];
            entry[0..4].copy_from_slice(&(record.db_no as u32).to_be_bytes());
            entry[4..8].copy_from_slice(&record.timestamp.to_be_bytes());
            entry[8] = match record.status {
                ExtractStatus::Active => 1,
                ExtractStatus::Released => 0,
            };
            let name_bytes = record.user_name.as_bytes();
            let copy_len = name_bytes.len().min(EXTRACT_RECORD_SIZE - 12);
            entry[12..12 + copy_len].copy_from_slice(&name_bytes[..copy_len]);
            data.extend_from_slice(&entry);
        }

        data
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, EngineError> {
        if data.len() < 4 {
            return Ok(Self::new());
        }

        let count = u32::from_be_bytes(data[0..4].try_into().unwrap()) as usize;
        let mut records = Vec::with_capacity(count);
        let mut pos = 4;

        for _ in 0..count {
            if pos + EXTRACT_RECORD_SIZE > data.len() {
                break;
            }
            let entry = &data[pos..pos + EXTRACT_RECORD_SIZE];
            let db_no = u32::from_be_bytes(entry[0..4].try_into().unwrap()) as i32;
            let timestamp = u32::from_be_bytes(entry[4..8].try_into().unwrap());
            let status = if entry[8] == 1 {
                ExtractStatus::Active
            } else {
                ExtractStatus::Released
            };
            let name_end = entry[12..]
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(EXTRACT_RECORD_SIZE - 12);
            let user_name = String::from_utf8_lossy(&entry[12..12 + name_end]).to_string();

            records.push(ExtractRecord {
                db_no,
                user_name,
                timestamp,
                status,
            });
            pos += EXTRACT_RECORD_SIZE;
        }

        Ok(Self { records })
    }
}

impl Default for ExtractManager {
    fn default() -> Self {
        Self::new()
    }
}
