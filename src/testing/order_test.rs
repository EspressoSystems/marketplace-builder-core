use hotshot_types::{
    data::{Leaf, QuorumProposal, ViewNumber},
    message::Proposal,
    signature_key::BLSPubKey,
    simple_certificate::{QuorumCertificate, SimpleCertificate, SuccessThreshold},
    traits::{block_contents::BlockPayload, node_implementation::ConsensusTime},
};

use crate::builder_state::MessageType;

use std::{collections::HashSet, fmt::Debug, marker::PhantomData};

use async_compatibility_layer::art::async_sleep;
use async_compatibility_layer::channel::unbounded;
use async_std::prelude::FutureExt;
use hotshot::types::SignatureKey;
use hotshot_types::{simple_vote::QuorumData, traits::block_contents::vid_commitment};

use hotshot_example_types::{
    block_types::{TestBlockHeader, TestBlockPayload, TestMetadata, TestTransaction},
    state_types::{TestInstanceState, TestValidatedState},
};

use crate::utils::BuilderStateId;
use crate::{
    builder_state::{DaProposalMessage, QuorumProposalMessage, RequestMessage, TransactionSource},
    testing::TestTypes,
};
use crate::{service::handle_received_txns, testing::start_builder_state};
use committable::Committable;
use hotshot::rand;
use std::sync::Arc;
use std::time::Duration;

/// The function checks whether the common part of two transaction vectors have the same order
fn order_check<T: Eq + Clone + Debug>(
    transaction_history: Vec<T>,
    all_transactions: Vec<Vec<T>>,
) -> bool {
    let all_transactions_vec = all_transactions.into_iter().flatten().collect::<Vec<_>>();
    tracing::debug!(
        "Doing order check, transaction_history = {:?}, all_transactions = {:?}",
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
#[async_std::test]
async fn test_builder_order() {
    async_compatibility_layer::logging::setup_logging();
    async_compatibility_layer::logging::setup_backtrace();
    tracing::info!("Testing the builder core with multiple messages from the channels");

    // Number of views to simulate
    const NUM_ROUNDS: usize = 10;
    // Number of transactions to submit per round
    const NUM_TXNS_PER_ROUND: usize = 5;
    // Capacity of broadcast channels
    const CHANNEL_CAPACITY: usize = NUM_ROUNDS * 5;
    // Number of nodes on DA committee
    const NUM_STORAGE_NODES: usize = 4;

    let (senders, global_state) = start_builder_state(CHANNEL_CAPACITY, NUM_STORAGE_NODES).await;

    // Transactions to send
    let all_transactions = (0..NUM_ROUNDS)
        .map(|round| {
            (0..NUM_TXNS_PER_ROUND)
                .map(|tx_num| TestTransaction::new(vec![round as u8, tx_num as u8]))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    // generate three different random number between (0..NUM_ROUNDS) to do some changes for output transactions
    let mut unique_rounds = HashSet::new();
    while unique_rounds.len() < 3 {
        let random_round = rand::random::<usize>() % (NUM_ROUNDS - 2);
        unique_rounds.insert(random_round);
    }
    let random_rounds: Vec<_> = unique_rounds.into_iter().collect();
    let skip_round = 2; //random_rounds[0]; // the round we want to skip all the transactions
    let adjust_add_round = 0; // random_rounds[1]; // the round we want to randomly add some transactions
    let adjust_remove_round = -1; // the round we want to skip some transactions, after it is enabled the test is expected to fail
    let adjust_remove_tail_round = 1; //random_rounds[2]; // the round we want to cut off the end of the bundle
    let propose_in_advance_round = NUM_ROUNDS - 2; // the round we want to include tx in later round to propose in advance

    // set up state to track between simulated consensus rounds
    let mut prev_proposed_transactions: Option<Vec<TestTransaction>> = None;
    let mut prev_quorum_proposal: Option<QuorumProposal<TestTypes>> = None;
    let mut transaction_history = Vec::new();

    // Simulate NUM_ROUNDS of consensus. First we submit the transactions for this round to the builder,
    // then construct DA and Quorum Proposals based on what we received from builder in the previous round
    // and request a new bundle.
    #[allow(clippy::needless_range_loop)] // intent is clearer this way
    for round in 0..NUM_ROUNDS {
        // simulate transaction being submitted to the builder
        for res in handle_received_txns(
            &senders.transactions,
            all_transactions[round].clone(),
            TransactionSource::HotShot,
        )
        .await
        {
            res.unwrap();
        }

        // get transactions submitted in previous rounds, [] for genesis
        // and simulate the block built from those
        let transactions = prev_proposed_transactions.take().unwrap_or_default();
        let txn_commitments = transactions.iter().map(Committable::commit).collect();
        let encoded_transactions = TestTransaction::encode(&transactions);
        let block_payload = TestBlockPayload { transactions };
        let block_vid_commitment = vid_commitment(&encoded_transactions, NUM_STORAGE_NODES);
        let block_builder_commitment =
            <TestBlockPayload as BlockPayload<TestTypes>>::builder_commitment(
                &block_payload,
                &TestMetadata,
            );

        // generate key for leader of this round
        let seed = [round as u8; 32];
        let (pub_key, private_key) = BLSPubKey::generated_from_seed_indexed(seed, round as u64);

        let da_proposal = Arc::new(DaProposalMessage {
            view_number: ViewNumber::new(round as u64),
            txn_commitments,
            num_nodes: NUM_STORAGE_NODES,
            sender: pub_key,
            builder_commitment: block_builder_commitment.clone(),
        });

        let block_header = TestBlockHeader {
            block_number: round as u64,
            payload_commitment: block_vid_commitment,
            builder_commitment: block_builder_commitment,
            timestamp: round as u64,
        };

        let justify_qc = match prev_quorum_proposal.take() {
            None => {
                QuorumCertificate::<TestTypes>::genesis(
                    &TestValidatedState::default(),
                    &TestInstanceState::default(),
                )
                .await
            }
            Some(prev_proposal) => {
                let prev_justify_qc = &prev_proposal.justify_qc;
                let quorum_data = QuorumData::<TestTypes> {
                    leaf_commit: Leaf::from_quorum_proposal(&prev_proposal).commit(),
                };

                // form a justify qc
                SimpleCertificate::<TestTypes, QuorumData<TestTypes>, SuccessThreshold> {
                    vote_commitment: quorum_data.commit(),
                    data: quorum_data,
                    view_number: ViewNumber::new(round as u64),
                    signatures: prev_justify_qc.signatures.clone(),
                    _pd: PhantomData,
                }
            }
        };
        tracing::debug!("Iteration: {} justify_qc: {:?}", round, justify_qc);

        let quorum_proposal = QuorumProposal::<TestTypes> {
            block_header,
            view_number: ViewNumber::new(round as u64),
            justify_qc: justify_qc.clone(),
            upgrade_certificate: None,
            proposal_certificate: None,
        };

        prev_quorum_proposal = Some(quorum_proposal.clone());

        let qc_signature = <TestTypes as hotshot_types::traits::node_implementation::NodeType>::SignatureKey::sign(
                        &private_key,
                        block_vid_commitment.as_ref(),
                        ).expect("Failed to sign payload commitment while preparing QC proposal");

        let quorum_proposal_msg =
            MessageType::QuorumProposalMessage(QuorumProposalMessage::<TestTypes> {
                proposal: Arc::new(Proposal {
                    data: quorum_proposal.clone(),
                    signature: qc_signature,
                    _pd: PhantomData,
                }),
                sender: pub_key,
            });

        // send quorum and DA proposals for this round
        senders
            .da_proposal
            .broadcast(MessageType::DaProposalMessage(Arc::clone(&da_proposal)))
            .await
            .unwrap();
        senders
            .quorum_proposal
            .broadcast(quorum_proposal_msg)
            .await
            .unwrap();

        let (response_sender, response_receiver) = unbounded();
        let request_message = MessageType::<TestTypes>::RequestMessage(RequestMessage {
            requested_view_number: ViewNumber::new(round as u64),
            response_channel: response_sender,
        });

        let req_msg = (
            response_receiver,
            BuilderStateId {
                parent_commitment: block_vid_commitment,
                parent_view: ViewNumber::new(round as u64),
            },
            request_message,
        );

        // give builder state time to fork
        async_sleep(Duration::from_millis(100)).await;

        // get the builder state for parent view we've just simulated
        global_state
            .read_arc()
            .await
            .spawned_builder_states
            .get(&req_msg.1)
            .expect("Failed to get channel for matching builder")
            .broadcast(req_msg.2.clone())
            .await
            .unwrap();

        // get response
        let res_msg = req_msg
            .0
            .recv()
            .timeout(Duration::from_secs(10))
            .await
            .unwrap()
            .unwrap();
        // in the next round we will use received transactions to simulate
        // the block being proposed
        tracing::debug!(
            "Before assignment, prev_proposed_transactions = {:?}, res_msg = {:?}, req_msg = {:?}",
            prev_proposed_transactions,
            res_msg,
            req_msg
        );

        // play with transactions propsed by proposers: skip the whole round OR interspersed some txs randomly OR remove some txs randomly
        if let MessageType::<TestTypes>::RequestMessage(ref request) = req_msg.2 {
            let view_number = request.requested_view_number;
            if view_number == ViewNumber::new(skip_round as u64) {
                prev_proposed_transactions = None;
            } else {
                let mut proposed_transactions = res_msg.transactions.clone();
                if view_number == ViewNumber::new(adjust_add_round as u64) {
                    proposed_transactions.insert(
                        rand::random::<usize>() % NUM_TXNS_PER_ROUND,
                        TestTransaction::new(vec![
                            adjust_add_round as u8,
                            (NUM_TXNS_PER_ROUND + 1) as u8,
                        ]),
                    );
                } else if view_number == ViewNumber::new(adjust_remove_round as u64) {
                    proposed_transactions.remove(rand::random::<usize>() % NUM_TXNS_PER_ROUND);
                } else if view_number == ViewNumber::new(adjust_remove_tail_round as u64) {
                    proposed_transactions.pop();
                } else if view_number == ViewNumber::new(propose_in_advance_round as u64) {
                    proposed_transactions.push(TestTransaction::new(vec![(propose_in_advance_round + 1) as u8, 0 as u8,]));
                }
                prev_proposed_transactions = Some(proposed_transactions);
            }
        } else {
            tracing::error!("Unable to get request from RequestMessage");
        }
        // save transactions to history
        if prev_proposed_transactions != None {
            transaction_history.extend(prev_proposed_transactions.clone().unwrap());
        }
    }

    // we should've served all transactions submitted, and in correct order
    // the test will fail if the common part of two vectors of transactions don't have the same order
    assert!(order_check(transaction_history, all_transactions));
}
