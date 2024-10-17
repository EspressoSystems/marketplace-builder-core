fn main() {
    let _ = persistence_tests::test_transaction_append_and_load::<persistence::PostgresPersistence>();
}

#cfg(test)
mod persistence_tests{
	pub async fn test_transaction_append_and_load<P: TestablePersistence>() {
		setup_test();
		let storage = P::connect(&env::var("DATABASE_URL")?).await;
		assert_eq!(
            storage.load_transaction().await.unwrap(),
            None
        );
        
    let test_transaction = generate_test_transaction();
    storage.append_transaction(test_transaction).await.unwrap();
    assert_eq!(
            storage.load_transaction().await.unwrap(),
            Some(test_transaction.clone())
        );
	}
}