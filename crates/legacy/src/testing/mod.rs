// I can't "mark this sync", clippy
#![allow(clippy::declare_interior_mutable_const)]
#![allow(clippy::borrow_interior_mutable_const)]

use std::cell::LazyCell;

use hotshot::types::{BLSPubKey, SignatureKey};
use hotshot_builder_api::v0_1::builder::BuildError;
use hotshot_builder_api::v0_1::{block_info::AvailableBlockInfo, data_source::BuilderDataSource};
use hotshot_builder_api::v0_2::block_info::{AvailableBlockData, AvailableBlockHeaderInput};
use hotshot_example_types::block_types::TestTransaction;
use hotshot_example_types::node_types::TestTypes;
use marketplace_builder_shared::block::{BlockId, BuilderStateId};

use crate::service::{BuilderKeys, ProxyGlobalState};

mod basic;
mod block_size;
mod finalization;
mod integration;

const MOCK_LEADER_KEYS: LazyCell<BuilderKeys<TestTypes>> =
    LazyCell::new(|| BLSPubKey::generated_from_seed_indexed([0; 32], 0));

pub(crate) async fn get_available_blocks(
    proxy_global_state: &ProxyGlobalState<TestTypes>,
    state_id: &BuilderStateId<TestTypes>,
) -> Result<Vec<AvailableBlockInfo<TestTypes>>, BuildError> {
    proxy_global_state
        .available_blocks(
            &state_id.parent_commitment,
            *state_id.parent_view,
            MOCK_LEADER_KEYS.0,
            &BLSPubKey::sign(&MOCK_LEADER_KEYS.1, state_id.parent_commitment.as_ref()).unwrap(),
        )
        .await
}

pub(crate) async fn get_block(
    proxy_global_state: &ProxyGlobalState<TestTypes>,
    block_id: &BlockId<TestTypes>,
) -> Result<AvailableBlockData<TestTypes>, BuildError> {
    proxy_global_state
        .claim_block(
            &block_id.hash,
            *block_id.view,
            MOCK_LEADER_KEYS.0,
            &BLSPubKey::sign(&MOCK_LEADER_KEYS.1, block_id.hash.as_ref()).unwrap(),
        )
        .await
}

pub(crate) async fn get_block_header_input(
    proxy_global_state: &ProxyGlobalState<TestTypes>,
    block_id: &BlockId<TestTypes>,
) -> Result<AvailableBlockHeaderInput<TestTypes>, BuildError> {
    proxy_global_state
        .claim_block_header_input(
            &block_id.hash,
            *block_id.view,
            MOCK_LEADER_KEYS.0,
            &BLSPubKey::sign(&MOCK_LEADER_KEYS.1, block_id.hash.as_ref()).unwrap(),
        )
        .await
}

pub(crate) async fn get_transactions(
    proxy_global_state: &ProxyGlobalState<TestTypes>,
    builder_state_id: &BuilderStateId<TestTypes>,
) -> Vec<TestTransaction> {
    let mut available_states = get_available_blocks(proxy_global_state, builder_state_id)
        .await
        .unwrap();

    let Some(block_info) = available_states.pop() else {
        return vec![];
    };

    let block_id = BlockId {
        hash: block_info.block_hash,
        view: builder_state_id.parent_view,
    };
    // Get block for its transactions
    let block = get_block(proxy_global_state, &block_id).await.unwrap();
    block.block_payload.transactions
}
