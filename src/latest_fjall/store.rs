use crate::latest_fjall::model::{
    normalize_attr_name, normalize_attr_value, BuildMeta, LatestElementRecord, META_BUILD_INFO_KEY,
};
use anyhow::Result;
use fjall::{Database, Keyspace, KeyspaceCreateOptions, PersistMode};
use std::path::Path;

const KEYSPACE_LATEST_ELE: &str = "latest_ele";
const KEYSPACE_IDX_ATTR_EXACT: &str = "idx_attr_exact";
const KEYSPACE_META: &str = "meta";
const SEP: char = '\u{1f}';

pub struct LatestFjallStore {
    db: Database,
    latest_ele: Keyspace,
    idx_attr_exact: Keyspace,
    meta: Keyspace,
}

impl LatestFjallStore {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Database::builder(path).open()?;
        let latest_ele = db.keyspace(KEYSPACE_LATEST_ELE, || KeyspaceCreateOptions::default())?;
        let idx_attr_exact = db.keyspace(KEYSPACE_IDX_ATTR_EXACT, || KeyspaceCreateOptions::default())?;
        let meta = db.keyspace(KEYSPACE_META, || KeyspaceCreateOptions::default())?;

        Ok(Self {
            db,
            latest_ele,
            idx_attr_exact,
            meta,
        })
    }

    pub fn reset(&self) -> Result<()> {
        self.latest_ele.clear()?;
        self.idx_attr_exact.clear()?;
        self.meta.clear()?;
        Ok(())
    }

    pub fn write_records_batch(&self, records: &[LatestElementRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let mut batch = self.db.batch();
        for record in records {
            let data_key = Self::latest_data_key(&record.refno);
            let data_val = serde_json::to_vec(record)?;
            batch.insert(&self.latest_ele, data_key.into_bytes(), data_val);

            for (attr, value) in record.whitelist_index_pairs() {
                let idx_key = Self::attr_exact_key(&attr, &value, &record.refno);
                batch.insert(&self.idx_attr_exact, idx_key.into_bytes(), Vec::<u8>::new());
            }
        }
        batch.commit()?;
        Ok(())
    }

    pub fn write_meta(&self, meta: &BuildMeta) -> Result<()> {
        let val = serde_json::to_vec(meta)?;
        self.meta.insert(META_BUILD_INFO_KEY, val)?;
        Ok(())
    }

    pub fn persist(&self) -> Result<()> {
        self.db.persist(PersistMode::SyncAll)?;
        Ok(())
    }

    pub fn query_by_refno(&self, refno: &str) -> Result<Option<LatestElementRecord>> {
        let key = Self::latest_data_key(refno);
        let Some(val) = self.latest_ele.get(key.as_bytes())? else {
            return Ok(None);
        };
        let record = serde_json::from_slice::<LatestElementRecord>(val.as_ref())?;
        Ok(Some(record))
    }

    pub fn query_refnos_by_attr_exact(
        &self,
        attr: &str,
        value: &str,
        limit: usize,
    ) -> Result<Vec<String>> {
        let prefix = Self::attr_exact_prefix(attr, value);
        let mut out = Vec::new();

        for kv in self.idx_attr_exact.prefix(prefix.as_bytes()) {
            let key = kv.key()?;
            let key_str = match std::str::from_utf8(key.as_ref()) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if let Some((_, refno)) = key_str.rsplit_once(SEP) {
                out.push(refno.to_string());
                if limit > 0 && out.len() >= limit {
                    break;
                }
            }
        }

        Ok(out)
    }

    fn latest_data_key(refno: &str) -> String {
        format!("r:{refno}")
    }

    fn attr_exact_prefix(attr: &str, value: &str) -> String {
        let attr = normalize_attr_name(attr);
        let value = normalize_attr_value(value);
        format!("{attr}{SEP}{value}{SEP}")
    }

    fn attr_exact_key(attr: &str, value: &str, refno: &str) -> String {
        format!("{}{refno}", Self::attr_exact_prefix(attr, value))
    }
}
