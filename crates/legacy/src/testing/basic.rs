use async_broadcast::broadcast;
use hotshot::types::EventType;
use hotshot_builder_api::v0_1::data_source::AcceptsTxnSubmits;

use hotshot_example_types::block_types::{TestBlockHeader, TestMetadata, TestTransaction};
use hotshot_example_types::node_types::{TestTypes, TestVersions};
use hotshot_example_types::state_types::{TestInstanceState, TestValidatedState};
use hotshot_types::data::{Leaf, QuorumProposal, ViewNumber};
use hotshot_types::event::LeafInfo;
use hotshot_types::simple_certificate::QuorumCertificate;
use hotshot_types::traits::block_contents::BlockHeader;
use hotshot_types::traits::node_implementation::ConsensusTime;
use hotshot_types::utils::BuilderCommitment;
use marketplace_builder_shared::testing::consensus::SimulatedChainState;
use marketplace_builder_shared::testing::constants::{
    TEST_NUM_NODES_IN_VID_COMPUTATION, TEST_PROTOCOL_MAX_BLOCK_SIZE,
};
use tokio::time::sleep;
use tracing_subscriber::EnvFilter;

use crate::service::{BuilderConfig, GlobalState, ProxyGlobalState};
use std::sync::Arc;
use std::time::Duration;

/// This test simulates consensus performing as expected and builder processing a number
/// of transactions
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
        let transactions = super::get_transactions(&proxy_global_state, &builder_state_id).await;

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

// This test checks that builder prunes saved blocks on decide
#[tokio::test]
async fn test_pruning() {
    // Setup logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    // Number of views to simulate
    const NUM_ROUNDS: usize = 10;
    // View number of decide event
    const DECIDE_VIEW: u64 = 5;
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
    Arc::clone(&global_state).start_event_loop(event_stream);

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

    let mut chain_state = SimulatedChainState::new(event_stream_sender.clone());

    // Simulate NUM_ROUNDS of consensus. First we submit the transactions for this round to the builder,
    // then construct DA and Quorum Proposals based on what we received from builder in the previous round
    // and request a new bundle.
    #[allow(clippy::needless_range_loop)] // intent is clearer this way
    for round in 0..NUM_ROUNDS {
        // All tiered maps shouldn't be pruned
        assert_eq!(
            *global_state
                .block_store
                .read()
                .await
                .blocks
                .lowest_view()
                .unwrap_or(ViewNumber::genesis()),
            0,
        );
        assert_eq!(
            *global_state
                .block_store
                .read()
                .await
                .block_cache
                .lowest_view()
                .unwrap_or(ViewNumber::genesis()),
            0,
        );
        assert_eq!(*global_state.coordinator.lowest_view().await, 0);

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
        let transactions = super::get_transactions(&proxy_global_state, &builder_state_id).await;

        // in the next round we will use received transactions to simulate
        // the block being proposed
        prev_proposed_transactions = Some(transactions.clone());
        // save transactions to history
        transaction_history.extend(transactions);
    }

    // Send a bogus decide event. The only thing we care about is the leaf's view number,
    // everything else is boilerplate.

    let mock_qc =
        QuorumCertificate::genesis::<TestVersions>(&Default::default(), &Default::default()).await;
    let leaf = Leaf::from_quorum_proposal(&QuorumProposal {
        block_header: <TestBlockHeader as BlockHeader<TestTypes>>::genesis(
            &Default::default(),
            Default::default(),
            BuilderCommitment::from_bytes([]),
            TestMetadata {
                num_transactions: 0,
            },
        ),
        view_number: ViewNumber::new(DECIDE_VIEW), // <- This is the only thing we're interested in
        justify_qc: mock_qc.clone(),
        upgrade_certificate: None,
        proposal_certificate: None,
    });
    event_stream_sender
        .broadcast(hotshot::types::Event {
            view_number: ViewNumber::new(NUM_ROUNDS as u64),
            event: EventType::Decide {
                leaf_chain: Arc::new(vec![LeafInfo {
                    leaf,
                    state: Arc::new(TestValidatedState::default()),
                    delta: None,
                    vid_share: None,
                }]),
                qc: Arc::new(mock_qc),
                block_size: None,
            },
        })
        .await
        .unwrap();

    // Give builder time to handle decide event
    sleep(Duration::from_millis(100)).await;

    // Tiered maps should be pruned now
    assert_eq!(
        *global_state
            .block_store
            .read()
            .await
            .blocks
            .lowest_view()
            .unwrap(),
        DECIDE_VIEW
    );
    assert_eq!(
        *global_state
            .block_store
            .read()
            .await
            .block_cache
            .lowest_view()
            .unwrap(),
        DECIDE_VIEW
    );
    assert_eq!(*global_state.coordinator.lowest_view().await, DECIDE_VIEW);
}
