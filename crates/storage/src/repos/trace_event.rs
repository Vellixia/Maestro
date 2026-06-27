use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;

use crate::{db::Db, error::{Result, StorageError}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTraceEvent {
    pub id: Option<RecordId>,
    pub run_id: String,
    pub event_type: String,
    pub data: serde_json::Value,
    pub ts: DateTime<Utc>,
}

pub struct TraceEventRepo {
    db: Db,
}

impl TraceEventRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn append(&self, run_id: &str, event_type: &str, data: serde_json::Value, ts: DateTime<Utc>) -> Result<()> {
        let event = StoredTraceEvent {
            id: None,
            run_id: run_id.to_string(),
            event_type: event_type.to_string(),
            data,
            ts,
        };
        let _: Option<StoredTraceEvent> = self
            .db
            .create("trace_event")
            .content(event)
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }

    pub async fn list_for_run(&self, run_id: &str) -> Result<Vec<StoredTraceEvent>> {
        let id = run_id.to_string();
        let results: Vec<StoredTraceEvent> = self
            .db
            .query("SELECT * FROM trace_event WHERE run_id = $run_id ORDER BY ts ASC")
            .bind(("run_id", id))
            .await
            .map_err(StorageError::Surreal)?
            .take(0)
            .map_err(StorageError::Surreal)?;
        Ok(results)
    }

    pub async fn delete_for_run(&self, run_id: &str) -> Result<()> {
        let id = run_id.to_string();
        self.db
            .query("DELETE trace_event WHERE run_id = $run_id")
            .bind(("run_id", id))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }
}
