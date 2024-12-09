use super::BuilderPersistence;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::SqlitePool;
use std::time::{Instant, SystemTime};

#[allow(unused_imports)]
use super::get_sqlite_test_db_path;

#[derive(Debug)]
pub struct SqliteTxnDb {
    pool: SqlitePool,
}
impl SqliteTxnDb {
    #[allow(dead_code)]
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

    #[allow(dead_code)]
    async fn clear(&self) -> Result<(), sqlx::Error> {
        // Execute a SQL statement to delete all rows from the `transactions` table
        sqlx::query("DELETE FROM transactions")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl BuilderPersistence for SqliteTxnDb {
    async fn append(&self, tx_data: Vec<u8>) -> Result<(), sqlx::Error> {
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

    async fn load(&self, timeout_after: Instant) -> Result<Vec<Vec<u8>>, sqlx::Error> {
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
            WHERE created_at <= ? ;
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
mod test {
    use super::get_sqlite_test_db_path;
    use super::BuilderPersistence;
    use super::SqliteTxnDb;
    use std::time::Instant;

    /// This test checks we can set up sqlite properly
    /// and can do basic append() and load()
    #[tokio::test]
    async fn test_persistence_append_and_load_txn() {
        // Initialize the database
        tracing::debug!(
            "get_sqlite_test_db_path() = {:?}",
            get_sqlite_test_db_path()
        );
        let db = SqliteTxnDb::new(get_sqlite_test_db_path()).await.expect(
            "In test_persistence_append_and_load_txn, it should be able to initiate a sqlite db.",
        );

        db.clear()
            .await
            .expect("In test_persistence_remove_txn, it should be able to clear all transactions.");

        // Append a few transactions
        let test_tx_data_list = vec![vec![1, 2, 3], vec![4, 5, 6]];
        for tx in test_tx_data_list.clone() {
            db.append(tx).await.expect("In test_persistence_append_and_load_txn, there shouldn't be any error when doing append");
        }

        // Set timeout_after to the current time
        let timeout_after = Instant::now();

        // Simulate some delay
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Append one more transaction
        db.append(vec![7, 8, 9]).await.expect("In test_persistence_append_and_load_txn, there shouldn't be any error when doing append");

        // Load transactions before timeout_after
        let tx_data_list = db.load(timeout_after).await.expect(
            "In test_persistence_append_and_load_txn, it should be able to load some transactions.",
        );
        tracing::debug!("Transaction data before timeout: {:?}", tx_data_list);

        assert_eq!(tx_data_list, test_tx_data_list);

        db.clear()
            .await
            .expect("In test_persistence_remove_txn, it should be able to clear all transactions.");
    }

    #[tokio::test]
    /// This test checks we can remove transaction from database properly
    async fn test_persistence_remove_txn() {
        // Initialize the database
        let db = SqliteTxnDb::new(get_sqlite_test_db_path())
            .await
            .expect("In test_persistence_remove_txn, it should be able to initiate a sqlite db.");

        db.clear()
            .await
            .expect("In test_persistence_remove_txn, it should be able to clear all transactions.");

        // Append some transactions
        let test_tx_data_list = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];
        for tx in test_tx_data_list.clone() {
            db.append(tx).await.expect(
                "In test_persistence_remove_txn, there shouldn't be any error when doing append",
            );
        }

        // Load all transactions
        let mut all_transactions = db
            .load(Instant::now())
            .await
            .expect("In test_persistence_remove_txn, it should be able to load some transactions.");
        tracing::debug!("All transactions before removal: {:?}", all_transactions);
        assert_eq!(all_transactions, test_tx_data_list);

        // Remove a specific transaction
        let tx_to_remove = test_tx_data_list[1].clone();
        if let Err(e) = db.remove(tx_to_remove).await {
            panic!("Failed to remove transaction: {}", e);
        } else {
            tracing::debug!("Transaction removed.");
        }

        // Load all transactions after removal

        let remaining_transactions = db
            .load(Instant::now())
            .await
            .expect("In test_persistence_remove_txn, it should be able to load some transactions.");
        tracing::debug!(
            "All transactions after removal: {:?}",
            remaining_transactions
        );
        all_transactions.remove(1);
        assert_eq!(remaining_transactions, all_transactions);

        db.clear()
            .await
            .expect("In test_persistence_remove_txn, it should be able to clear all transactions.");
    }
}
