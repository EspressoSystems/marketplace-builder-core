use std::{hash::Hash, marker::PhantomData};

use crate::{
    builder_state::{
        BuilderState, DaProposalMessage, MessageType, QuorumProposalMessage, RequestMessage,
        ResponseMessage,
    },
    BuilderStateId, LegacyCommit,
};
use async_broadcast::{Sender as BroadcastSender};
use async_broadcast::broadcast;
use async_compatibility_layer::channel::{unbounded, UnboundedReceiver};
use hotshot::{
    traits::{election::static_committee::StaticCommittee, BlockPayload},
    types::{BLSPubKey, SignatureKey},
};
use hotshot_types::{
    data::{Leaf, QuorumProposal, ViewNumber},
    message::Proposal,
    signature_key::BuilderKey,
    simple_certificate::{QuorumCertificate, SimpleCertificate, SuccessThreshold},
    simple_vote::QuorumData,
    traits::{
        block_contents::vid_commitment,
        node_implementation::{ConsensusTime, NodeType},
    },
    utils::BuilderCommitment,
};

use hotshot_example_types::{
    auction_results_provider_types::TestAuctionResult,
    block_types::{TestBlockHeader, TestBlockPayload, TestMetadata, TestTransaction},
    node_types::TestVersions,
    state_types::{TestInstanceState, TestValidatedState},
};
use serde::{Deserialize, Serialize};

use crate::service::{GlobalState};
use crate::ParentBlockReferences;
use async_lock::RwLock;
use committable::{Commitment, CommitmentBoundsArkless, Committable};
use std::sync::Arc;
use std::time::Duration;


#[derive(
    Copy, Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct TestTypes;
impl NodeType for TestTypes {
    type Time = ViewNumber;
    type BlockHeader = TestBlockHeader;
    type BlockPayload = TestBlockPayload;
    type SignatureKey = BLSPubKey;
    type Transaction = TestTransaction;
    type ValidatedState = TestValidatedState;
    type InstanceState = TestInstanceState;
    type Membership = StaticCommittee<Self>;
    type BuilderSignatureKey = BuilderKey;
    type AuctionResult = TestAuctionResult;
}

pub async fn start_builder_state_without_event_loop(
    channel_capacity: usize,
    num_storage_nodes: usize,
) -> (
    BroadcastSender<TestTypes>,
    Arc<RwLock<GlobalState<TestTypes>>>,
    BuilderState<TestTypes>,
) {
    // set up the broadcast channels
    let (bootstrap_sender, bootstrap_receiver) =
        broadcast::<MessageType<TestTypes>>(channel_capacity);
    let (senders, receivers) = broadcast(channel_capacity);

    let genesis_vid_commitment = vid_commitment(&[], num_storage_nodes);
    let genesis_builder_commitment = BuilderCommitment::from_bytes([]);
    let parent_block_references = ParentBlockReferences {
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
    )));

    // instantiate the bootstrap builder state
    let bootstrap_builder_state = BuilderState::new(
        ParentBlockReferences {
            view_number: ViewNumber::new(0),
            vid_commitment: vid_commitment(&[], 8),
            leaf_commit: Commitment::<Leaf<TestTypes>>::default_commitment_no_preimage(),
            builder_commitment: BuilderCommitment::from_bytes([]),
        },
        decide_receiver.clone(),
        da_receiver.clone(),
        quorum_proposal_receiver.clone(),
        bootstrap_receiver,
        tx_receiver,
        tx_queue,
        global_state.clone(),
        NonZeroUsize::new(TEST_NUM_NODES_IN_VID_COMPUTATION).unwrap(),
        Duration::from_millis(100),
        1,
        Arc::new(TestInstanceState::default()),
        Duration::from_millis(100),
        Arc::new(TestValidatedState::default()),
    );

    (senders, global_state, builder_state)
}

/// set up the broadcast channels and instatiate the global state with fixed channel capacity and num nodes
pub async fn start_builder_state(
    channel_capacity: usize,
    num_storage_nodes: usize,
) -> (
    BroadcastSenders<TestTypes>,
    Arc<RwLock<GlobalState<TestTypes>>>,
) {
    let (senders, global_state, builder_state) =
        start_builder_state_without_event_loop(channel_capacity, num_storage_nodes).await;

    // start the event loop
    builder_state.event_loop();

    (senders, global_state)
}

/// get transactions submitted in previous rounds, [] for genesis
/// and simulate the block built from those
pub async fn calc_proposal_msg(
    num_storage_nodes: usize,
    round: usize,
    prev_quorum_proposal: Option<QuorumProposal<TestTypes>>,
    transactions: Vec<TestTransaction>,
) -> (
    QuorumProposal<TestTypes>,
    MessageType<TestTypes>,
    MessageType<TestTypes>,
    BuilderStateId<TestTypes>,
) {
    // get transactions submitted in previous rounds, [] for genesis
    // and simulate the block built from those
    let num_transactions = transactions.len() as u64;
    let txn_commitments = transactions.iter().map(Committable::commit).collect();
    let encoded_transactions = TestTransaction::encode(&transactions);
    let block_payload = TestBlockPayload { transactions };
    let block_vid_commitment = vid_commitment(&encoded_transactions, num_storage_nodes);
    let metadata = TestMetadata { num_transactions };
    let block_builder_commitment =
        <TestBlockPayload as BlockPayload<TestTypes>>::builder_commitment(
            &block_payload,
            &metadata,
        );

    // generate key for leader of this round
    let seed = [round as u8; 32];
    let (pub_key, private_key) = BLSPubKey::generated_from_seed_indexed(seed, round as u64);

    let da_proposal = Arc::new(DaProposalMessage {
        view_number: ViewNumber::new(round as u64),
        txn_commitments,
        sender: pub_key,
        builder_commitment: block_builder_commitment.clone(),
    });

    let block_header = TestBlockHeader {
        block_number: round as u64,
        payload_commitment: block_vid_commitment,
        builder_commitment: block_builder_commitment,
        timestamp: round as u64,
        metadata,
        random: 1, // arbitrary
    };

    let justify_qc = match prev_quorum_proposal.as_ref() {
        None => {
            QuorumCertificate::<TestTypes>::genesis::<TestVersions>(
                &TestValidatedState::default(),
                &TestInstanceState::default(),
            )
            .await
        }
        Some(prev_proposal) => {
            let prev_justify_qc = &prev_proposal.justify_qc;
            let quorum_data = QuorumData::<TestTypes> {
                leaf_commit: Leaf::from_quorum_proposal(prev_proposal).legacy_commit(),
            };

            // form a justify qc
            SimpleCertificate::<TestTypes, QuorumData<TestTypes>, SuccessThreshold>::new(
                quorum_data.clone(),
                quorum_data.commit(),
                prev_proposal.view_number,
                prev_justify_qc.signatures.clone(),
                PhantomData,
            )
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

    let quorum_signature =
        <TestTypes as hotshot_types::traits::node_implementation::NodeType>::SignatureKey::sign(
            &private_key,
            block_vid_commitment.as_ref(),
        )
        .expect("Failed to sign payload commitment while preparing Quorum proposal");

    let quorum_proposal_msg =
        MessageType::QuorumProposalMessage(QuorumProposalMessage::<TestTypes> {
            proposal: Arc::new(Proposal {
                data: quorum_proposal.clone(),
                signature: quorum_signature,
                _pd: PhantomData,
            }),
            sender: pub_key,
        });

    let da_proposal_msg = MessageType::DaProposalMessage(Arc::clone(&da_proposal));
    let builder_state_id = BuilderStateId {
        parent_commitment: block_vid_commitment,
        parent_view: ViewNumber::new(round as u64),
    };
    (
        quorum_proposal,
        quorum_proposal_msg,
        da_proposal_msg,
        builder_state_id,
    )
}

/// get request message
/// it contains receiver, builder state id ( which helps looking up builder state in global state) and request message in view number and response channel
async fn get_req_msg(
    round: u64,
    builder_state_id: BuilderStateId<TestTypes>,
) -> (
    UnboundedReceiver<ResponseMessage<TestTypes>>,
    BuilderStateId<TestTypes>,
    MessageType<TestTypes>,
) {
    let (response_sender, response_receiver) = unbounded();
    let request_message = MessageType::<TestTypes>::RequestMessage(RequestMessage {
        requested_view_number: ViewNumber::new(round),
        response_channel: response_sender,
    });

    (response_receiver, builder_state_id, request_message)
}