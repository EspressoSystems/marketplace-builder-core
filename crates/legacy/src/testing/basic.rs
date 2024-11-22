use async_broadcast::broadcast;
use hotshot_builder_api::v0_1::data_source::AcceptsTxnSubmits;

use hotshot_example_types::block_types::TestTransaction;
use hotshot_example_types::state_types::TestInstanceState;
use marketplace_builder_shared::block::BlockId;
use marketplace_builder_shared::testing::consensus::SimulatedChainState;
use marketplace_builder_shared::testing::constants::{
    TEST_NUM_NODES_IN_VID_COMPUTATION, TEST_PROTOCOL_MAX_BLOCK_SIZE,
};
use tracing_subscriber::EnvFilter;

use crate::service::{BuilderConfig, GlobalState, ProxyGlobalState};
use std::sync::Arc;

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

    let global_state = GlobalState::new(
        BuilderConfig::test(),
        TestInstanceState::default(),
        TEST_PROTOCOL_MAX_BLOCK_SIZE,
        TEST_NUM_NODES_IN_VID_COMPUTATION,
    );
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

        // get response
        let mut available_states =
            super::get_available_blocks(&proxy_global_state, &builder_state_id)
                .await
                .unwrap();

        let transactions = match available_states.pop() {
            Some(block_info) => {
                let block_id = BlockId {
                    hash: block_info.block_hash,
                    view: builder_state_id.parent_view,
                };
                // Get block for its transactions
                let block = super::get_block(&proxy_global_state, &block_id)
                    .await
                    .unwrap();
                // Get header input just to check it's in working order
                super::get_block_header_input(&proxy_global_state, &block_id)
                    .await
                    .expect("Failed to claim header input");
                block.block_payload.transactions
            }
            None => vec![],
        };

        // in the next round we will use received transactions to simulate
        // the block being proposed
        prev_proposed_transactions = Some(transactions.clone());
        // save transactions to history
        transaction_history.extend(transactions);
    }

    // we should've served all transactions submitted, and in correct order
    assert_eq!(
        transaction_history,
        all_transactions.into_iter().flatten().collect::<Vec<_>>()
    );
}
