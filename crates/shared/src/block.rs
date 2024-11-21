//! Shared types dealing with block information

use std::time::Instant;

use committable::{Commitment, Committable};
use hotshot_types::data::fake_commitment;
use hotshot_types::traits::node_implementation::ConsensusTime;
use hotshot_types::{
    data::Leaf,
    traits::{block_contents::Transaction, node_implementation::NodeType},
    utils::BuilderCommitment,
    vid::VidCommitment,
};

/// Enum to hold the different sources of the transaction
#[derive(Clone, Debug, PartialEq)]
pub enum TransactionSource {
    /// Transaction from private mempool
    Private,
    /// Transaction from public mempool
    Public,
}

/// [`ReceivedTransaction`] represents receipt information concerning a received
/// [`NodeType::Transaction`].
#[derive(Debug, Clone)]
pub struct ReceivedTransaction<Types: NodeType> {
    /// the transaction
    pub transaction: Types::Transaction,
    /// transaction's hash
    pub commit: Commitment<Types::Transaction>,
    /// transaction's esitmated length
    pub min_block_size: u64,
    /// transaction's source
    pub source: TransactionSource,
    /// received time
    pub time_in: Instant,
}

impl<Types: NodeType> ReceivedTransaction<Types> {
    pub fn new(transaction: Types::Transaction, source: TransactionSource) -> Self {
        Self {
            commit: transaction.commit(),
            min_block_size: transaction.minimum_block_size(),
            source,
            time_in: Instant::now(),
            transaction,
        }
    }
}

/// Unique identifier for a block
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct BlockId<Types: NodeType> {
    /// Block hash
    pub hash: BuilderCommitment,
    /// Block view
    pub view: Types::View,
}

impl<Types: NodeType> std::fmt::Display for BlockId<Types> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Block({}@{})",
            hex::encode(self.hash.as_ref()),
            *self.view
        )
    }
}

/// Unique identifier for a builder state
///
/// Builder state is identified by the VID commitment
/// and view of the block it targets to extend, i.e.
/// builder with given state ID assumes blocks/bundles it's building
/// are going to be included immediately after the parent block.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuilderStateId<Types: NodeType> {
    /// View number of the parent block
    pub parent_view: Types::View,
    /// VID commitment of the parent block
    pub parent_commitment: VidCommitment,
}

impl<Types: NodeType> std::fmt::Display for BuilderStateId<Types> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BuilderState({}@{})",
            self.parent_commitment, *self.parent_view
        )
    }
}

/// References to the parent block that is extended to spawn the new builder state.
#[derive(derive_more::Debug, Clone, PartialEq, Eq)]
pub struct ParentBlockReferences<Types: NodeType> {
    /// View on which the parent block was proposed
    pub view_number: Types::View,
    /// VID commitment of the parent block payload
    pub vid_commitment: VidCommitment,
    /// Leaf commitment of the parent leaf
    pub leaf_commit: Commitment<Leaf<Types>>,
    /// Builder commitment of the parent block payload
    pub builder_commitment: BuilderCommitment,
    /// Number of transactions included in the parent block
    pub tx_number: usize,
    /// Number of views since the last non-empty block, if known
    /// Can be interpreted as offset from parent block to the
    /// last non-empty one, i.e. 0 indicates that the parent block is
    /// not empty, and 1 would indicate that this block is empty,
    /// but grandparent isn't
    pub views_since_nonempty_block: Option<u64>,
}

impl<Types> ParentBlockReferences<Types>
where
    Types: NodeType,
{
    /// Create mock references for bootstrap (don't correspond to a real block)
    pub fn bootstrap() -> Self {
        Self {
            view_number: Types::View::genesis(),
            vid_commitment: VidCommitment::default(),
            leaf_commit: fake_commitment(),
            builder_commitment: BuilderCommitment::from_bytes([0; 32]),
            tx_number: 0,
            views_since_nonempty_block: None,
        }
    }
}

// implement display for the derived info
impl<Types: NodeType> std::fmt::Display for ParentBlockReferences<Types> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "View Number: {:?}", self.view_number)
    }
}
