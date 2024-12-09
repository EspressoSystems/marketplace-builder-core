
#![cfg(feature = "sqlite")]

use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use sqlx::SqlitePool;

// Sishan TODO: link to the transaction type we already had
#[derive(Debug)]
struct BuilderDbTransaction {
    id: i64,
    tx_data: Vec<u8>,
    created_at: Instance,
}

pub struct SqliteTxnDb {
    pool: SqlitePool,
}

trait BuilderPersistence {
    async fn new(database_url: String) -> Result<Self, sqlx::Error>;
    async fn append(
        &self,
        tx_data: Vec<u8>,
    ) -> Result<(), sqlx::Error>;
    async fn load(
        &self,
        timeout_after: Instance,
    ) -> Result<Option<Vec<u8>>, sqlx::Error>;
    async fn remove(
        &self,
        tx: Vec<u8>,
    ) -> Result<()>;
}

#[async_trait]
impl BuilderPersistence for SqliteTxnDb {
    async fn new(database_url: String) -> Result<Self, sqlx::Error> {
        let pool = SqlitePool::connect(&database_url).await?;
        // it will handle the default CURRENT_TIMESTAMP automatically and assign to transaction's created_at
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tx_data BLOB NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )
        .execute(&pool)
        .await?;
        Ok(Self { pool })
    }

    async fn append(&self, tx_data: Vec<u8>) -> Result<(), sqlx::Error>
    {
        sqlx::query(
            r#"
            INSERT INTO transactions (tx_data) VALUES (?);
            "#,
        )
        .bind(tx_data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn load(&self, timeout_after: Instance) -> Result<Vec<Vec<u8>>, sqlx::Error>
    {
        // Convert Instant to SystemTime
        let now = SystemTime::now();
        let elapsed = timeout_after.elapsed();
        let target_time = now - elapsed;

        // Convert SystemTime to a format SQLite understands (RFC 3339)
        let target_timestamp = DateTime::<Utc>::from(target_time)
            .naive_utc()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let rows = sqlx::query(
            r#"
            SELECT id, tx_data, created_at FROM transactions
            WHERE created_at < ? 
            ORDER BY created_at DESC LIMIT 1;
            "#,
        )
        .bind(target_timestamp)
        .fetch_all(&self.pool)
        .await?;

        let tx_data_list = rows
            .into_iter()
            .map(|row| row.get::<Vec<u8>, _>("tx_data"))
            .collect();
        Ok(tx_data_list)
    }

    async fn remove(&self, tx_data: Vec<u8>) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
            r#"
            DELETE FROM transactions WHERE tx_data = ?;
            "#,
        )
        .bind(tx_data)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() > 0 {
            Ok(())
        } else {
            Err(sqlx::Error::RowNotFound)
        }
    }

}

#[cfg(test)]
mod test{
    use std::time::Instant;

    /// This test checks we can set up sqlite properly
    /// and can do basic append() and load()
    #[tokio::test]
	pub async fn test_persistence_append_and_load_txn<P: TestablePersistence>() {
		// Initialize the database
        let db = SqliteTxnDb::new("sqlite://transactions.db").await?;

        // Append a few transactions
        db.append(vec![1, 2, 3]).await?;
        db.append(vec![4, 5, 6]).await?;
    
        // Set timeout_after to the current time
        let timeout_after = Instant::now();

        // Simulate some delay
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Append more transactions
        db.append(vec![7, 8, 9]).await?;

        // Load transactions before timeout_after
        let tx_data_list = db.load(timeout_after).await?;
        println!("Transaction data before timeout:");
        for tx_data in tx_data_list {
            println!("{:?}", tx_data);
        }
        

        // Sishan TODO: add assertion
        // assert_eq!(
        //         storage.load_transaction().await.unwrap(),
        //         Some(test_transaction.clone())
        // );
	}

    #[tokio::test]
    /// This test checks we can remove transaction from database properly
	pub async fn test_persistence_remove_txn<P: TestablePersistence>() {
        // Initialize the database
        let db = SqliteTxnDb::new("sqlite://transactions.db").await?;

        // Append some transactions
        db.append(vec![1, 2, 3]).await?;
        db.append(vec![4, 5, 6]).await?;
        db.append(vec![7, 8, 9]).await?;

        // Load all transactions
        println!("All transactions before removal:");
        let all_transactions = db.load(Instant::now()).await?;
        for tx_data in &all_transactions {
            println!("{:?}", tx_data);
        }

        // Remove a specific transaction
        println!("\nRemoving transaction [4, 5, 6]...");
        if let Err(e) = db.remove(vec![4, 5, 6]).await {
            eprintln!("Failed to remove transaction: {}", e);
        } else {
            println!("Transaction [4, 5, 6] removed.");
        }

        // Load all transactions after removal
        println!("\nAll transactions after removal:");
        let remaining_transactions = db.load(Instant::now()).await?;
        for tx_data in remaining_transactions {
            println!("{:?}", tx_data);
        }

        Ok(())
    }
}