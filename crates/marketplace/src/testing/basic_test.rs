use async_broadcast::broadcast;
use hotshot::types::{BLSPubKey, SignatureKey};
use hotshot_builder_api::v0_3::data_source::{AcceptsTxnSubmits, BuilderDataSource};

use hotshot_example_types::block_types::TestTransaction;
use marketplace_builder_shared::testing::constants::{
    TEST_API_TIMEOUT, TEST_BASE_FEE, TEST_INCLUDED_TX_GC_PERIOD, TEST_MAXIMIZE_TX_CAPTURE_TIMEOUT,
};
use tokio::time::sleep;
use tracing_subscriber::EnvFilter;

use crate::hooks::NoHooks;
use crate::service::{GlobalState, ProxyGlobalState};
use marketplace_builder_shared::testing::consensus::SimulatedChainState;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

/// This test simulates multiple builder states receiving messages from the channels and processing them
#[tokio::test]
async fn test_builder() {
    // Setup logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    tracing::info!("Testing the builder core with multiple messages from the channels");

    // Number of views to simulate
    const NUM_ROUNDS: usize = 5;
    // Number of transactions to submit per round
    const NUM_TXNS_PER_ROUND: usize = 4;
    // Capacity of broadcast channels
    const CHANNEL_CAPACITY: usize = NUM_ROUNDS * 5;

    let global_state = Arc::new(GlobalState::new(
        BLSPubKey::generated_from_seed_indexed([0; 32], 0),
        TEST_API_TIMEOUT,
        TEST_MAXIMIZE_TX_CAPTURE_TIMEOUT,
        TEST_INCLUDED_TX_GC_PERIOD,
        CHANNEL_CAPACITY,
        TEST_BASE_FEE,
        NoHooks(PhantomData),
    ));
    let proxy_global_state = ProxyGlobalState(Arc::clone(&global_state));

    let (event_stream_sender, event_stream) = broadcast(1024);
    global_state.start_event_loop(event_stream);

    // Transactions to send
    let all_transactions = (0..NUM_ROUNDS)
        .map(|round| {
            (0..NUM_TXNS_PER_ROUND)
                .map(|tx_num| TestTransaction::new(vec![round as u8, tx_num as u8]))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    // set up state to track between simulated consensus rounds
    let mut prev_proposed_transactions: Option<Vec<TestTransaction>> = None;
    let mut transaction_history = Vec::new();

    let mut chain_state = SimulatedChainState::new(event_stream_sender);

    // Simulate NUM_ROUNDS of consensus. First we submit the transactions for this round to the builder,
    // then construct DA and Quorum Proposals based on what we received from builder in the previous round
    // and request a new bundle.
    #[allow(clippy::needless_range_loop)] // intent is clearer this way
    for round in 0..NUM_ROUNDS {
        // simulate transaction being submitted to the builder
        proxy_global_state
            .submit_txns(all_transactions[round].clone())
            .await
            .unwrap();

        // get transactions submitted in previous rounds, [] for genesis
        // and simulate the block built from those
        let builder_state_id = chain_state
            .simulate_consensus_round(prev_proposed_transactions)
            .await;

        // give builder state time to fork
        sleep(Duration::from_millis(100)).await;

        // get response
        let bundle = proxy_global_state
            .bundle(
                *builder_state_id.parent_view,
                &builder_state_id.parent_commitment,
                round as u64 + 1,
            )
            .await
            .unwrap();

        // in the next round we will use received transactions to simulate
        // the block being proposed
        prev_proposed_transactions = Some(bundle.transactions.clone());
        // save transactions to history
        transaction_history.extend(bundle.transactions);
    }

    // we should've served all transactions submitted, and in correct order
    assert_eq!(
        transaction_history,
        all_transactions.into_iter().flatten().collect::<Vec<_>>()
    );
}
