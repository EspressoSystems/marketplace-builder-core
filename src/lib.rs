// Copyright (c) 2024 Espresso Systems (espressosys.com)
// This file is part of the HotShot Builder Protocol.
//

// Builder Phase 1
// It mainly provides three API services to hotshot proposers:
// 1. Serves a proposer(leader)'s request to provide blocks information
// 2. Serves a proposer(leader)'s request to provide the full blocks information
// 3. Serves a proposer(leader)'s request to provide the block header information

// It also provides one API services external users:
// 1. Serves a user's request to submit a private transaction

use hotshot_types::{
    traits::node_implementation::NodeType, utils::BuilderCommitment, vid::VidCommitment,
};

// providing the core services to support above API services
pub mod builder_state;

// Core interaction with the HotShot network
pub mod service;

// tracking the testing
pub mod testing;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct BlockId<TYPES: NodeType> {
    hash: BuilderCommitment,
    view: TYPES::Time,
}

impl<TYPES: NodeType> std::fmt::Display for BlockId<TYPES> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Block({}@{})",
            hex::encode(self.hash.as_ref()),
            *self.view
        )
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct BuilderStateId<TYPES: NodeType> {
    parent_commitment: VidCommitment,
    view: TYPES::Time,
}

impl<TYPES: NodeType> std::fmt::Display for BuilderStateId<TYPES> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BuilderState({}@{})", self.parent_commitment, *self.view)
    }
}
