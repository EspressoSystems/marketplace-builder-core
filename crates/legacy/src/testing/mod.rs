// I can't "mark this sync", clippy
#![allow(clippy::declare_interior_mutable_const)]
#![allow(clippy::borrow_interior_mutable_const)]

use std::cell::LazyCell;

use async_broadcast::Sender;
use hotshot::rand::{thread_rng, Rng};
use hotshot::types::{BLSPubKey, Event, EventType, SignatureKey};
use hotshot_builder_api::v0_1::builder::BuildError;
use hotshot_builder_api::v0_1::{block_info::AvailableBlockInfo, data_source::BuilderDataSource};
use hotshot_builder_api::v0_2::block_info::{AvailableBlockData, AvailableBlockHeaderInput};
use hotshot_builder_api::v0_3::data_source::AcceptsTxnSubmits;
use hotshot_example_types::block_types::TestTransaction;
use hotshot_example_types::node_types::TestTypes;
use hotshot_types::data::ViewNumber;
use hotshot_types::traits::node_implementation::NodeType;
use marketplace_builder_shared::block::{BlockId, BuilderStateId};
use marketplace_builder_shared::error::Error;

use crate::service::{BuilderKeys, ProxyGlobalState};

mod basic;
mod block_size;
mod finalization;
mod integration;

const MOCK_LEADER_KEYS: LazyCell<BuilderKeys<TestTypes>> =
    LazyCell::new(|| BLSPubKey::generated_from_seed_indexed([0; 32], 0));

fn sign(
    data: &[u8],
) -> <<TestTypes as NodeType>::SignatureKey as SignatureKey>::PureAssembledSignatureType {
    <<TestTypes as NodeType>::SignatureKey as SignatureKey>::sign(&MOCK_LEADER_KEYS.1, data)
        .unwrap()
}

// We need to extract the error strings by hand because BuildError doesn't implement Eq
fn assert_eq_generic_err(err: BuildError, expected_err: Error<TestTypes>) {
    let BuildError::Error(expected_err_str) = expected_err.into() else {
        panic!("Unexpected conversion of Error to BuildError");
    };
    println!("{:#?}", err);
    let BuildError::Error(err_str) = err else {
        panic!("Unexpected BuildError by builder");
    };
    assert_eq!(expected_err_str, err_str);
}

struct TestProxyGlobalState(ProxyGlobalState<TestTypes>);

impl TestProxyGlobalState {
    pub(crate) async fn get_available_blocks(
        &self,
        state_id: &BuilderStateId<TestTypes>,
    ) -> Result<Vec<AvailableBlockInfo<TestTypes>>, BuildError> {
        self.0
            .available_blocks(
                &state_id.parent_commitment,
                *state_id.parent_view,
                MOCK_LEADER_KEYS.0,
                &BLSPubKey::sign(&MOCK_LEADER_KEYS.1, state_id.parent_commitment.as_ref()).unwrap(),
            )
            .await
    }

    pub(crate) async fn get_block(
        &self,
        block_id: &BlockId<TestTypes>,
    ) -> Result<AvailableBlockData<TestTypes>, BuildError> {
        self.0
            .claim_block(
                &block_id.hash,
                *block_id.view,
                MOCK_LEADER_KEYS.0,
                &BLSPubKey::sign(&MOCK_LEADER_KEYS.1, block_id.hash.as_ref()).unwrap(),
            )
            .await
    }

    pub(crate) async fn get_block_header_input(
        &self,
        block_id: &BlockId<TestTypes>,
    ) -> Result<AvailableBlockHeaderInput<TestTypes>, BuildError> {
        self.0
            .claim_block_header_input(
                &block_id.hash,
                *block_id.view,
                MOCK_LEADER_KEYS.0,
                &BLSPubKey::sign(&MOCK_LEADER_KEYS.1, block_id.hash.as_ref()).unwrap(),
            )
            .await
    }

    pub(crate) async fn get_transactions(
        &self,
        builder_state_id: &BuilderStateId<TestTypes>,
    ) -> Vec<TestTransaction> {
        let mut available_states = self.get_available_blocks(builder_state_id).await.unwrap();

        let Some(block_info) = available_states.pop() else {
            return vec![];
        };

        let block_id = BlockId {
            hash: block_info.block_hash,
            view: builder_state_id.parent_view,
        };
        // Get block for its transactions
        let block = self.get_block(&block_id).await.unwrap();
        block.block_payload.transactions
    }

    pub(crate) async fn submit_transactions(
        &self,
        event_sender: &Sender<Event<TestTypes>>,
        view_number: ViewNumber,
        transactions: Vec<TestTransaction>,
    ) {
        if thread_rng().gen() {
            self.0.submit_txns(transactions).await.unwrap();
        } else {
            event_sender
                .broadcast(Event {
                    view_number,
                    event: EventType::Transactions { transactions },
                })
                .await
                .unwrap();
        }
    }
}
