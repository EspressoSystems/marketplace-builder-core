use std::marker::PhantomData;

use committable::Committable;
use hotshot::{
    traits::BlockPayload,
    types::{BLSPubKey, EventType, SignatureKey},
};
use hotshot_example_types::{
    block_types::{TestBlockHeader, TestBlockPayload, TestMetadata, TestTransaction},
    node_types::{TestTypes, TestVersions},
    state_types::{TestInstanceState, TestValidatedState},
};
use hotshot_types::{
    data::{DaProposal, Leaf, QuorumProposal, ViewNumber},
    message::Proposal,
    simple_certificate::{QuorumCertificate, SimpleCertificate, SuccessThreshold},
    simple_vote::QuorumData,
    traits::{block_contents::vid_commitment, node_implementation::ConsensusTime},
};
use marketplace_builder_shared::block::BuilderStateId;
use marketplace_builder_shared::testing::constants::TEST_NUM_NODES_IN_VID_COMPUTATION;
use sha2::{Digest, Sha256};

pub mod basic_test;
pub mod integration;
pub mod order_test;

pub async fn calc_proposal_events(
    round: usize,
    prev_quorum_proposal: Option<QuorumProposal<TestTypes>>,
    transactions: Vec<TestTransaction>,
) -> (
    QuorumProposal<TestTypes>,
    Vec<EventType<TestTypes>>,
    BuilderStateId<TestTypes>,
) {
    // get transactions submitted in previous rounds, [] for genesis
    // and simulate the block built from those
    let num_transactions = transactions.len() as u64;
    let encoded_transactions = TestTransaction::encode(&transactions);
    let block_payload = TestBlockPayload { transactions };
    let block_vid_commitment =
        vid_commitment(&encoded_transactions, TEST_NUM_NODES_IN_VID_COMPUTATION);
    let metadata = TestMetadata { num_transactions };
    let block_builder_commitment =
        <TestBlockPayload as BlockPayload<TestTypes>>::builder_commitment(
            &block_payload,
            &metadata,
        );

    // generate key for leader of this round
    let seed = [round as u8; 32];
    let (pub_key, private_key) = BLSPubKey::generated_from_seed_indexed(seed, round as u64);

    let quorum_signature =
        <TestTypes as hotshot_types::traits::node_implementation::NodeType>::SignatureKey::sign(
            &private_key,
            block_vid_commitment.as_ref(),
        )
        .expect("Failed to sign payload commitment while preparing Quorum proposal");
    let da_signature =
        <TestTypes as hotshot_types::traits::node_implementation::NodeType>::SignatureKey::sign(
            &private_key,
            Sha256::digest(&encoded_transactions).as_ref(),
        )
        .expect("Failed to sign payload commitment while preparing DA proposal");

    let da_proposal = DaProposal {
        encoded_transactions: encoded_transactions.into(),
        metadata,
        view_number: ViewNumber::new(round as u64),
    };

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
                leaf_commit: Committable::commit(&Leaf::from_quorum_proposal(prev_proposal)),
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

    let quorum_proposal_event = EventType::QuorumProposal {
        proposal: Proposal {
            data: quorum_proposal.clone(),
            signature: quorum_signature,
            _pd: PhantomData,
        },
        sender: pub_key,
    };

    let da_proposal_event = EventType::DaProposal {
        proposal: Proposal {
            data: da_proposal,
            signature: da_signature,
            _pd: PhantomData,
        },
        sender: pub_key,
    };

    let builder_state_id = BuilderStateId {
        parent_commitment: block_vid_commitment,
        parent_view: ViewNumber::new(round as u64),
    };

    (
        quorum_proposal,
        vec![quorum_proposal_event, da_proposal_event],
        builder_state_id,
    )
}
