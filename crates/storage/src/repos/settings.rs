use serde::{Deserialize, Serialize};

use crate::{
    db::Db,
    error::{Result, StorageError},
};

pub struct SettingsRepo {
    db: Db,
}

impl SettingsRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        #[derive(Deserialize)]
        struct Row {
            value: serde_json::Value,
        }
        let k = key.to_string();
        let mut resp = self
            .db
            .query("SELECT value FROM settings WHERE key = $key LIMIT 1")
            .bind(("key", k))
            .await
            .map_err(StorageError::Surreal)?;
        let row: Option<Row> = resp.take(0).map_err(StorageError::Surreal)?;
        Ok(row.map(|r| r.value))
    }

    pub async fn set(&self, key: &str, value: serde_json::Value) -> Result<()> {
        let k = key.to_string();
        self.db
            .query(
                "INSERT INTO settings (key, value) VALUES ($key, $value)
                 ON DUPLICATE KEY UPDATE value = $value",
            )
            .bind(("key", k))
            .bind(("value", value))
            .await
            .map_err(StorageError::Surreal)?;
        Ok(())
    }

    pub async fn get_typed<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.get(key).await? {
            None => Ok(None),
            Some(v) => serde_json::from_value(v).map(Some).map_err(StorageError::Serde),
        }
    }

    pub async fn set_typed<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let v = serde_json::to_value(value).map_err(StorageError::Serde)?;
        self.set(key, v).await
    }
}
