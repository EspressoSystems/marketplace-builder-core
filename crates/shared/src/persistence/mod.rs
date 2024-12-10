use anyhow::Context;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub mod sqlite;

#[async_trait]
pub trait BuilderPersistence {
    async fn append(&self, tx_data: Vec<u8>) -> Result<(), sqlx::Error>;
    async fn load(&self, timeout_after: Instant) -> Result<Vec<Vec<u8>>, sqlx::Error>;
    async fn remove(&self, tx: Vec<u8>) -> Result<(), sqlx::Error>;
}

pub fn build_sqlite_path(path: &Path) -> anyhow::Result<PathBuf> {
    let sub_dir = path.join("sqlite");

    // if `sqlite` sub dir does not exist then create it
    if !sub_dir.exists() {
        std::fs::create_dir_all(&sub_dir)
            .with_context(|| format!("failed to create directory: {:?}", sub_dir))?;
    }

    // Return the full path to the SQLite database file
    let db_path = sub_dir.join("database.sqlite");

    // Ensure the file exists (create it if it doesn’t)
    if !db_path.exists() {
        std::fs::File::create(&db_path)
            .with_context(|| format!("Failed to create SQLite database file: {:?}", db_path))?;
    }

    Ok(db_path)
}
