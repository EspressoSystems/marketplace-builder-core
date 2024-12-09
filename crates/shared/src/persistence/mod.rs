use async_trait::async_trait;
use std::env;
use std::time::Instant;

pub mod sqlite;

#[async_trait]
pub trait BuilderPersistence {
    async fn append(&self, tx_data: Vec<u8>) -> Result<(), sqlx::Error>;
    async fn load(&self, timeout_after: Instant) -> Result<Vec<Vec<u8>>, sqlx::Error>;
    async fn remove(&self, tx: Vec<u8>) -> Result<(), sqlx::Error>;
}

pub fn get_sqlite_test_db_path() -> String {
    // Sishan TODO: make it more clean and find a more reliable way
    let current_dir = env::current_dir().expect("Failed to get current working directory");
    let mut path = current_dir.clone();
    path.push("src/persistence"); // Add "persistence" directory
    path.push("test_data"); // Add "test_data" directory
    path.push("sqlite"); // Add "sqlite" directory
    path.push("transactions.db"); // Database file name
    path.to_string_lossy().to_string()
}
