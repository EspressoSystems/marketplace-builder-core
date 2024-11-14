use async_broadcast::broadcast;
use hotshot_builder_api::v0_3::data_source::{AcceptsTxnSubmits, BuilderDataSource};
use hotshot_types::{
    bundle::Bundle,
    data::{QuorumProposal, ViewNumber},
    traits::node_implementation::ConsensusTime,
};
use marketplace_builder_shared::{
    block::BuilderStateId,
    testing::constants::{
        TEST_API_TIMEOUT, TEST_BASE_FEE, TEST_INCLUDED_TX_GC_PERIOD,
        TEST_MAXIMIZE_TX_CAPTURE_TIMEOUT,
    },
};
use tracing_subscriber::EnvFilter;

use crate::{
    hooks::NoHooks,
    service::{GlobalState, ProxyGlobalState},
    testing::calc_proposal_events,
};

use std::{fmt::Debug, marker::PhantomData, sync::Arc};

use hotshot_example_types::block_types::TestTransaction;
use hotshot_example_types::node_types::TestTypes;

use hotshot::{
    rand::{self, seq::SliceRandom, thread_rng},
    types::{BLSPubKey, Event, SignatureKey},
};

/// [`RoundTransactionBehavior`] is an enum that is used to represent different
/// behaviors that we may want to simulate during a round.  This applies to
/// determining which transactions are included in the block, and how their
/// order is adjusted before being included for consensus.
#[derive(Clone, Debug)]
enum RoundTransactionBehavior {
    /// [`NoAdjust`] indicates that the transactions should be passed through
    /// without any adjustment
    NoAdjust,

    /// [Skip] indicates that the transactions should be omitted entirely
    Skip,

    /// [`AjdustAdd`] indicates that a new transaction should be added to the
    /// transactions submitted
    AdjustAdd(usize),

    /// [`AdjustRemoveTail`] indicates that the last transaction should be removed
    /// from the transactions submitted
    AdjustRemoveTail,

    /// [`ProposeInAdvance`] indicates that a transaction should be added to the
    /// transactions submitted that indicates that it is for the next round
    /// (i.e. the round after the one being processed)
    ProposeInAdvance(usize),

    /// [`AdjustRemove`] indicates that a random transaction (not the last one)
    /// should be removed from the transactions submitted
    AdjustRemove,
}

impl RoundTransactionBehavior {
    /// [`process_transactions`] is a helper method that takes a vector of transactions
    /// and applies the behavior specified by the [`RoundTransactionBehavior`] enum
    /// to the transactions before returning them.
    fn process_transactions(&self, transactions: Vec<TestTransaction>) -> Vec<TestTransaction> {
        match self {
            RoundTransactionBehavior::NoAdjust => transactions,
            RoundTransactionBehavior::Skip => vec![],
            RoundTransactionBehavior::AdjustAdd(adjust_add_round) => {
                let mut transactions = transactions.clone();
                transactions.insert(
                    rand::random::<usize>() % transactions.len(),
                    TestTransaction::new(vec![
                        *adjust_add_round as u8,
                        (transactions.len() + 1) as u8,
                    ]),
                );
                transactions
            }
            RoundTransactionBehavior::AdjustRemoveTail => {
                let mut transactions = transactions.clone();
                transactions.pop();
                transactions
            }
            RoundTransactionBehavior::ProposeInAdvance(propose_in_advance_round) => {
                let mut transactions = transactions.clone();
                transactions.push(TestTransaction::new(vec![
                    (propose_in_advance_round + 1) as u8,
                    0_u8,
                ]));
                transactions
            }
            RoundTransactionBehavior::AdjustRemove => {
                let mut transactions = transactions.clone();
                transactions.remove(rand::random::<usize>() % (transactions.len() - 1));
                transactions
            }
        }
    }
}

/// The function checks whether the common part of two transaction vectors have the same order
fn order_check<T: Eq + Clone + Debug>(
    transaction_history: Vec<T>,
    all_transactions: Vec<Vec<T>>,
) -> bool {
    let all_transactions_vec = all_transactions.into_iter().flatten().collect::<Vec<_>>();
    tracing::debug!(
        "Doing order check:\n\ttransaction_history = {:?}\n\tall_transactions = {:?}",
        transaction_history,
        all_transactions_vec
    );
    let common_txs: Vec<_> = transaction_history
        .iter()
        .filter(|item| all_transactions_vec.contains(item))
        .collect();

    let another_common_txs: Vec<_> = all_transactions_vec
        .iter()
        .filter(|item| transaction_history.contains(item))
        .collect();

    common_txs == another_common_txs
}

/// This test simulates multiple builder states receiving messages from the channels and processing them
/// and focus specifically on orders.
/// It's fine that leader doesn't include some of transactions we've given, or interspersed with other transactions,
/// as long as the order is correct it will be good.
#[tokio::test]
async fn test_builder_order() {
    // Setup logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    tracing::info!("Testing the builder core with multiple messages from the channels");

    /// Number of views to simulate, make sure it's larger than 5
    /// so that we have enough rounds to play with
    const NUM_ROUNDS: usize = 10;
    /// Number of transactions to submit per round
    const NUM_TXNS_PER_ROUND: usize = 5;
    /// Capacity of broadcast channels
    const CHANNEL_CAPACITY: usize = NUM_ROUNDS * 5;

    let global_state = GlobalState::new(
        BLSPubKey::generated_from_seed_indexed([0; 32], 0),
        TEST_API_TIMEOUT,
        TEST_MAXIMIZE_TX_CAPTURE_TIMEOUT,
        TEST_INCLUDED_TX_GC_PERIOD,
        CHANNEL_CAPACITY,
        TEST_BASE_FEE,
        NoHooks(PhantomData),
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

    // generate three different random number between (0..(NUM_ROUNDS-2)) to do some changes for output transactions
    // it's not the last two rounds as they'll be used to test propose_in_advance
    let round_range: Vec<_> = (0..(NUM_ROUNDS - 2)).collect();
    let mut rng = thread_rng();
    let random_rounds: Vec<_> = round_range.choose_multiple(&mut rng, 3).cloned().collect();
    // the round we want to skip all the transactions
    let skip_round = random_rounds[0];
    // the round we want to randomly add some transactions
    let adjust_add_round = random_rounds[1];
    // the round we want to cut off the end of the bundle
    let adjust_remove_tail_round = random_rounds[2];
    // the round we want to include tx in later round (NUM_ROUNDS -1 which is also the final round) to propose in advance
    let propose_in_advance_round = NUM_ROUNDS - 2;

    // determine_round_behavior is a helper function that takes a round number
    // and returns the desired [RoundTransactionBehavior] for that round.
    let determine_round_behavior = |round: usize| -> RoundTransactionBehavior {
        if round == skip_round {
            return RoundTransactionBehavior::Skip;
        }

        if round == adjust_add_round {
            return RoundTransactionBehavior::AdjustAdd(adjust_add_round);
        }

        if round == adjust_remove_tail_round {
            return RoundTransactionBehavior::AdjustRemoveTail;
        }

        if propose_in_advance_round == round {
            return RoundTransactionBehavior::ProposeInAdvance(propose_in_advance_round + 1);
        }

        RoundTransactionBehavior::NoAdjust
    };

    // set up state to track between simulated consensus rounds
    let mut prev_proposed_transactions: Option<Vec<TestTransaction>> = None;
    let mut prev_quorum_proposal: Option<QuorumProposal<TestTypes>> = None;
    let mut transaction_history = Vec::new();

    // Simulate NUM_ROUNDS of consensus. First we submit the transactions for this round to the builder,
    // then construct DA and Quorum Proposals based on what we received from builder in the previous round
    // and request a new bundle.
    for (round, round_transactions, round_behavior) in all_transactions
        .iter()
        .enumerate()
        .map(|(round, txns)| (round, txns, determine_round_behavior(round)))
    {
        // Simulate consensus deciding on the transactions that are included
        // in the block.
        let BuilderStateId {
            parent_view,
            parent_commitment,
        } = {
            // get transactions submitted in previous rounds, [] for genesis
            // and simulate the block built from those
            let transactions = prev_proposed_transactions.take().unwrap_or_default();
            let (quorum_proposal, events, builder_state_id) =
                calc_proposal_events(round, prev_quorum_proposal, transactions).await;

            prev_quorum_proposal = Some(quorum_proposal.clone());

            for evt in events {
                event_stream_sender
                    .broadcast(Event {
                        view_number: ViewNumber::new(round as u64),
                        event: evt,
                    })
                    .await
                    .unwrap();
            }

            builder_state_id
        };

        // simulate transaction being submitted to the builder
        proxy_global_state
            .submit_txns(round_transactions.clone())
            .await
            .unwrap();

        let Bundle { transactions, .. } = proxy_global_state
            .bundle(parent_view.u64(), &parent_commitment, parent_view.u64())
            .await
            .unwrap();

        // process the specific round behavior to modify the transactions we
        // received
        let transactions = round_behavior.process_transactions(transactions);

        prev_proposed_transactions = Some(transactions.clone());

        // save transactions to history
        transaction_history.extend(transactions);
    }

    // we should've served all transactions submitted, and in correct order
    // the test will fail if the common part of two vectors of transactions don't have the same order
    assert!(order_check(transaction_history, all_transactions));
}

/// This test simulates multiple builder states receiving messages from the channels and processing them
/// and focus specifically on orders with chain fork.
/// with one chain proposing transactions we've given and the other not
/// (we should give out the next batch if responding for first chain and both batches for the other)
#[tokio::test]
async fn test_builder_order_chain_fork() {
    // Setup logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    tracing::info!("Testing the builder core with multiple messages from the channels");

    // Number of views to simulate
    const NUM_ROUNDS: usize = 4;
    // Number of transactions to submit per round
    const NUM_TXNS_PER_ROUND: usize = 5;
    // Capacity of broadcast channels
    const CHANNEL_CAPACITY: usize = NUM_ROUNDS * 5;

    // the round we want to skip all the transactions for the fork chain
    // round 0 is pre-fork
    // round 1 is where the fork happens
    // round 2 is exactly after the fork, they have different parents and the builder should give out different batches
    // round 3 should be back to normal, there's no fork anymore
    let fork_round = 1;

    // determine_round_behavior is a helper function that takes a round number
    // and returns the desired [RoundTransactionBehavior] for that round.
    let determine_round_behavior = |round: usize| -> RoundTransactionBehavior {
        if round == fork_round {
            return RoundTransactionBehavior::Skip;
        }

        RoundTransactionBehavior::NoAdjust
    };

    let global_state = GlobalState::new(
        BLSPubKey::generated_from_seed_indexed([0; 32], 0),
        TEST_API_TIMEOUT,
        TEST_MAXIMIZE_TX_CAPTURE_TIMEOUT,
        TEST_INCLUDED_TX_GC_PERIOD,
        CHANNEL_CAPACITY,
        TEST_BASE_FEE,
        NoHooks(PhantomData),
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
    let mut prev_proposed_transactions_branch_1: Option<Vec<TestTransaction>> = None;
    let mut prev_quorum_proposal_branch_1: Option<QuorumProposal<TestTypes>> = None;
    let mut transaction_history_branch_1 = Vec::new();

    // set up state to track the fork-ed chain
    let mut prev_proposed_transactions_branch_2: Option<Vec<TestTransaction>> = None;
    let mut prev_quorum_proposal_branch_2: Option<QuorumProposal<TestTypes>> = None;
    let mut transaction_history_branch_2 = Vec::new();

    // Simulate NUM_ROUNDS of consensus. First we submit the transactions for this round to the builder,
    // then construct DA and Quorum Proposals based on what we received from builder in the previous round
    // and request a new bundle.
    for (round, transactions, fork_round_behavior) in all_transactions
        .iter()
        .enumerate()
        .map(|(round, txns)| (round, txns, determine_round_behavior(round)))
    {
        // Simulate consensus deciding on the transactions that are included
        // in the block, branch 1
        let BuilderStateId {
            parent_view: parent_view_branch_1,
            parent_commitment: parent_commitment_branch_1,
        } = {
            // get transactions submitted in previous rounds, [] for genesis
            // and simulate the block built from those
            let transactions = prev_proposed_transactions_branch_1
                .clone()
                .unwrap_or_default();
            let (quorum_proposal, events, builder_state_id) =
                calc_proposal_events(round, prev_quorum_proposal_branch_1, transactions).await;

            prev_quorum_proposal_branch_1 = Some(quorum_proposal.clone());

            // send quorum and DA proposals for this round
            for evt in events {
                event_stream_sender
                    .broadcast(Event {
                        view_number: ViewNumber::new(round as u64),
                        event: evt,
                    })
                    .await
                    .unwrap();
            }

            builder_state_id
        };

        // Simulate consensus deciding on the transactions that are included
        // in the block, branch 2
        let BuilderStateId {
            parent_view: parent_view_branch_2,
            parent_commitment: parent_commitment_branch_2,
        } = {
            // get transactions submitted in previous rounds, [] for genesis
            // and simulate the block built from those
            let transactions = prev_proposed_transactions_branch_2
                .clone()
                .unwrap_or_default();

            let (quorum_proposal, events, builder_state_id) =
                calc_proposal_events(round, prev_quorum_proposal_branch_2, transactions).await;

            prev_quorum_proposal_branch_2 = Some(quorum_proposal.clone());

            // send quorum and DA proposals for this round
            // we also need to send out the message for the fork-ed chain although it's not forked yet
            // to prevent builders resend the transactions we've already committed
            for evt in events {
                event_stream_sender
                    .broadcast(Event {
                        view_number: ViewNumber::new(round as u64),
                        event: evt,
                    })
                    .await
                    .unwrap();
            }

            builder_state_id
        };

        // simulate transaction being submitted to the builder
        proxy_global_state
            .submit_txns(transactions.clone())
            .await
            .unwrap();

        let Bundle {
            transactions: transactions_branch_1,
            ..
        } = proxy_global_state
            .bundle(
                parent_view_branch_1.u64(),
                &parent_commitment_branch_1,
                parent_view_branch_1.u64(),
            )
            .await
            .unwrap();

        let Bundle {
            transactions: transactions_branch_2,
            ..
        } = proxy_global_state
            .bundle(
                parent_view_branch_2.u64(),
                &parent_commitment_branch_2,
                parent_view_branch_2.u64(),
            )
            .await
            .unwrap();

        let transactions_branch_2 = fork_round_behavior.process_transactions(transactions_branch_2);
        if transactions_branch_2 != transactions_branch_1 {
            tracing::debug!("Fork Exist.")
        } else {
            tracing::debug!("No fork.");
        }

        prev_proposed_transactions_branch_1 = Some(transactions_branch_1.clone());
        prev_proposed_transactions_branch_2 = Some(transactions_branch_2.clone());

        // save transactions to history
        transaction_history_branch_1.extend(transactions_branch_1);
        transaction_history_branch_2.extend(transactions_branch_2);
    }

    // With a fork, the transaction history should match once all transactions
    // have been processed.
    assert_eq!(
        transaction_history_branch_1, transaction_history_branch_2,
        "even with a fork, the transaction history branches should match"
    );
    // the test will fail if any transaction is re-ordered
    assert!(order_check(
        transaction_history_branch_1,
        all_transactions.clone()
    ));
    assert!(order_check(transaction_history_branch_2, all_transactions));
}

/// This test simulates multiple builder states receiving messages from the channels and processing them
/// and focus specifically on orders.
/// It should fail as the proposer randomly drop a subset of transactions within a bundle,
/// which leads to different order of transaction.
#[tokio::test]
async fn test_builder_order_should_fail() {
    // Setup logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    tracing::info!("Testing the builder core with multiple messages from the channels");

    // Number of views to simulate
    const NUM_ROUNDS: usize = 10;
    // Number of transactions to submit per round
    const NUM_TXNS_PER_ROUND: usize = 5;
    // Capacity of broadcast channels
    const CHANNEL_CAPACITY: usize = NUM_ROUNDS * 5;

    let global_state = GlobalState::new(
        BLSPubKey::generated_from_seed_indexed([0; 32], 0),
        TEST_API_TIMEOUT,
        TEST_MAXIMIZE_TX_CAPTURE_TIMEOUT,
        TEST_INCLUDED_TX_GC_PERIOD,
        CHANNEL_CAPACITY,
        TEST_BASE_FEE,
        NoHooks(PhantomData),
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

    // generate a random number between (0..NUM_ROUNDS) to do some changes for output transactions
    // the round we want to skip some transactions (cannot be the final round), after it is enabled the test is expected to fail
    let adjust_remove_round = rand::random::<usize>() % (NUM_ROUNDS - 1);
    // determine_round_behavior is a helper function that takes a round number
    // and returns the desired [RoundTransactionBehavior] for that round.
    let determine_round_behavior = |round: usize| -> RoundTransactionBehavior {
        if round == adjust_remove_round {
            return RoundTransactionBehavior::AdjustRemove;
        }

        RoundTransactionBehavior::NoAdjust
    };
    // set up state to track between simulated consensus rounds
    let mut prev_proposed_transactions: Option<Vec<TestTransaction>> = None;
    let mut prev_quorum_proposal: Option<QuorumProposal<TestTypes>> = None;
    let mut transaction_history = Vec::new();

    // Simulate NUM_ROUNDS of consensus. First we submit the transactions for this round to the builder,
    // then construct DA and Quorum Proposals based on what we received from builder in the previous round
    // and request a new bundle.
    for (round, round_transactions, round_behavior) in all_transactions
        .iter()
        .enumerate()
        .map(|(round, txns)| (round, txns, determine_round_behavior(round)))
    {
        // Simulate consensus deciding on the transactions that are included
        // in the block.
        let BuilderStateId {
            parent_view,
            parent_commitment,
        } = {
            // get transactions submitted in previous rounds, [] for genesis
            // and simulate the block built from those
            let transactions = prev_proposed_transactions.take().unwrap_or_default();
            let (quorum_proposal, events, builder_state_id) =
                calc_proposal_events(round, prev_quorum_proposal, transactions).await;

            prev_quorum_proposal = Some(quorum_proposal.clone());

            // send quorum and DA proposals for this round
            for evt in events {
                event_stream_sender
                    .broadcast(Event {
                        view_number: ViewNumber::new(round as u64),
                        event: evt,
                    })
                    .await
                    .unwrap();
            }

            builder_state_id
        };

        // simulate transaction being submitted to the builder
        proxy_global_state
            .submit_txns(round_transactions.clone())
            .await
            .unwrap();

        let Bundle { transactions, .. } = proxy_global_state
            .bundle(parent_view.u64(), &parent_commitment, parent_view.u64())
            .await
            .unwrap();

        let transactions = round_behavior.process_transactions(transactions);

        prev_proposed_transactions = Some(transactions.clone());

        // save transactions to history
        transaction_history.extend(transactions);
    }
    // we should've served all transactions submitted, and in correct order
    // the test will fail if the common part of two vectors of transactions don't have the same order
    assert!(!order_check(transaction_history, all_transactions));
}
