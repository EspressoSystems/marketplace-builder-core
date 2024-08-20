pub use hotshot::traits::election::static_committee::GeneralStaticCommittee;
pub use hotshot_types::{
    data::{DaProposal, Leaf, QuorumProposal, ViewNumber},
    message::Proposal,
    signature_key::{BLSPrivKey, BLSPubKey},
    simple_certificate::{QuorumCertificate, SimpleCertificate, SuccessThreshold},
    traits::{
        block_contents::BlockPayload,
        node_implementation::{ConsensusTime, NodeType},
    },
};

pub use crate::builder_state::{BuilderState, MessageType, ResponseMessage};
pub use async_broadcast::{
    broadcast, Receiver as BroadcastReceiver, RecvError, Sender as BroadcastSender, TryRecvError,
};
/// The following tests are performed:
#[cfg(test)]
mod tests {
    use super::*;
    use std::{hash::Hash, marker::PhantomData, num::NonZeroUsize};

    use async_compatibility_layer::art::async_sleep;
    use async_compatibility_layer::channel::unbounded;
    use async_std::prelude::FutureExt;
    use hotshot::types::SignatureKey;
    use hotshot_types::{
        signature_key::BuilderKey, simple_vote::QuorumData, traits::block_contents::vid_commitment,
        utils::BuilderCommitment,
    };

    use hotshot_example_types::{
        auction_results_provider_types::TestAuctionResult,
        block_types::{TestBlockHeader, TestBlockPayload, TestMetadata, TestTransaction},
        state_types::{TestInstanceState, TestValidatedState},
    };

    use crate::builder_state::{
        BuiltFromProposedBlock, DaProposalMessage, QuorumProposalMessage, RequestMessage,
        TransactionSource,
    };
    use crate::service::{broadcast_channels, handle_received_txns, GlobalState};
    use crate::utils::BuilderStateId;
    use async_lock::RwLock;
    use committable::{Commitment, CommitmentBoundsArkless, Committable};
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(
        Copy, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
    )]
    struct TestTypes;
    impl NodeType for TestTypes {
        type Time = ViewNumber;
        type BlockHeader = TestBlockHeader;
        type BlockPayload = TestBlockPayload;
        type SignatureKey = BLSPubKey;
        type Transaction = TestTransaction;
        type ValidatedState = TestValidatedState;
        type InstanceState = TestInstanceState;
        type Membership = GeneralStaticCommittee<TestTypes, Self::SignatureKey>;
        type BuilderSignatureKey = BuilderKey;
        type AuctionResult = TestAuctionResult;
    }

    use serde::{Deserialize, Serialize};
    /// This test simulates multiple builder states receiving messages from the channels and processing them
    #[async_std::test]
    //#[instrument]
    async fn test_builder() {
        async_compatibility_layer::logging::setup_logging();
        async_compatibility_layer::logging::setup_backtrace();
        tracing::info!("Testing the builder core with multiple messages from the channels");

        // Number of views to simulate
        const NUM_ROUNDS: usize = 5;
        // Number of transactions to submit per round
        const NUM_TXNS_PER_ROUND: usize = 5;
        // Capacity of broadcast channels
        const CHANNEL_CAPACITY: usize = NUM_ROUNDS * 5;
        // Number of nodes on DA commetee
        const NUM_STORAGE_NODES: usize = 4;

        // Transactions to send
        let all_transactions = (0..NUM_ROUNDS)
            .map(|round| {
                (0..NUM_TXNS_PER_ROUND)
                    .map(|tx_num| TestTransaction::new(vec![round as u8, tx_num as u8]))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        // set up the broadcast channels
        let (bootstrap_sender, bootstrap_receiver) =
            broadcast::<MessageType<TestTypes>>(CHANNEL_CAPACITY);
        let (senders, receivers) = broadcast_channels(CHANNEL_CAPACITY);

        let genesis_vid_commitment = vid_commitment(&[], NUM_STORAGE_NODES);
        let genesis_builder_commitment = BuilderCommitment::from_bytes([]);
        let built_from_info = BuiltFromProposedBlock {
            view_number: ViewNumber::genesis(),
            vid_commitment: genesis_vid_commitment,
            leaf_commit: Commitment::<Leaf<TestTypes>>::default_commitment_no_preimage(),
            builder_commitment: genesis_builder_commitment,
        };

        // instantiate the global state
        let global_state = Arc::new(RwLock::new(GlobalState::<TestTypes>::new(
            bootstrap_sender,
            senders.transactions.clone(),
            genesis_vid_commitment,
            ViewNumber::genesis(),
            ViewNumber::genesis(),
            0,
        )));

        // instantiate the bootstrap builder state
        let builder_state = BuilderState::<TestTypes>::new(
            built_from_info,
            &receivers,
            bootstrap_receiver,
            Vec::new(),
            Arc::clone(&global_state),
            NonZeroUsize::new(NUM_STORAGE_NODES).unwrap(),
            Duration::from_millis(10), // max time to wait for non-zero txn block
            0,                         // base fee
            Arc::new(TestInstanceState::default()),
            Duration::from_secs(3600), // duration for txn garbage collection
            Arc::new(TestValidatedState::default()),
        );

        // start the event loop
        builder_state.event_loop();

        // to store all the sent messages
        let mut sqc_msgs: Vec<QuorumProposalMessage<TestTypes>> = Vec::new();

        let mut proposed_transactions: Option<Vec<TestTransaction>> = None;
        let mut transaction_history = Vec::new();

        // generate num_test messages for each type and send it to the respective channels;
        for round in 0..NUM_ROUNDS {
            let prev_round = round.checked_sub(1);

            // get transactions submitted in previous rounds, [] for genesis
            let transactions = proposed_transactions.take().unwrap_or_default();
            let txn_commitments = transactions.iter().map(Committable::commit).collect();
            let encoded_transactions = TestTransaction::encode(&transactions);
            let block_payload = TestBlockPayload { transactions };
            let block_vid_commitment = vid_commitment(&encoded_transactions, NUM_STORAGE_NODES);
            let block_builder_commitment =
                <TestBlockPayload as BlockPayload<TestTypes>>::builder_commitment(
                    &block_payload,
                    &TestMetadata,
                );

            let transactions_to_submit = &all_transactions[round];

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

            let justify_qc = match prev_round {
                None => {
                    QuorumCertificate::<TestTypes>::genesis(
                        &TestValidatedState::default(),
                        &TestInstanceState::default(),
                    )
                    .await
                }
                Some(prev_round) => {
                    let prev_proposal = &sqc_msgs[prev_round].proposal;
                    let prev_justify_qc = &prev_proposal.data.justify_qc;
                    let leaf = Leaf::from_quorum_proposal(&prev_proposal.data);

                    let q_data = QuorumData::<TestTypes> {
                        leaf_commit: leaf.commit(),
                    };

                    let prev_view_number = prev_proposal.data.view_number.u64();
                    let view_number =
                        if prev_view_number == 0 && prev_justify_qc.view_number.u64() == 0 {
                            ViewNumber::new(0)
                        } else {
                            ViewNumber::new(1 + prev_justify_qc.view_number.u64())
                        };
                    // form a justify qc
                    SimpleCertificate::<TestTypes, QuorumData<TestTypes>, SuccessThreshold> {
                        vote_commitment: q_data.commit(),
                        data: q_data,
                        view_number,
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

            let qc_signature = <TestTypes as hotshot_types::traits::node_implementation::NodeType>::SignatureKey::sign(
                        &private_key,
                        block_vid_commitment.as_ref(),
                        ).expect("Failed to sign payload commitment while preparing QC proposal");

            let sqc_msg = QuorumProposalMessage::<TestTypes> {
                proposal: Arc::new(Proposal {
                    data: quorum_proposal.clone(),
                    signature: qc_signature,
                    _pd: PhantomData,
                }),
                sender: pub_key,
            };

            for res in handle_received_txns(
                &senders.transactions,
                transactions_to_submit.clone(),
                TransactionSource::HotShot,
            )
            .await
            {
                res.unwrap();
            }

            senders
                .da_proposal
                .broadcast(MessageType::DaProposalMessage(Arc::clone(&da_proposal)))
                .await
                .unwrap();
            senders
                .quorum_proposal
                .broadcast(MessageType::QuorumProposalMessage(sqc_msg.clone()))
                .await
                .unwrap();

            let (response_sender, response_receiver) = unbounded();
            let request_message = MessageType::<TestTypes>::RequestMessage(RequestMessage {
                requested_view_number: ViewNumber::new(round as u64),
                response_channel: response_sender,
            });

            sqc_msgs.push(sqc_msg);
            let req_msg = (
                response_receiver,
                BuilderStateId {
                    parent_commitment: block_vid_commitment,
                    view: ViewNumber::new(round as u64),
                },
                request_message,
            );

            async_sleep(Duration::from_millis(100)).await;

            global_state
                .read_arc()
                .await
                .spawned_builder_states
                .get(&req_msg.1)
                .expect("Failed to get channel for matching builder")
                .broadcast(req_msg.2.clone())
                .await
                .unwrap();

            let res_msg = req_msg
                .0
                .recv()
                .timeout(Duration::from_secs(10))
                .await
                .unwrap()
                .unwrap();
            proposed_transactions = Some(res_msg.transactions.clone());
            transaction_history.extend(res_msg.transactions);
        }

        assert_eq!(
            transaction_history,
            all_transactions.into_iter().flatten().collect::<Vec<_>>()
        );
    }
}
