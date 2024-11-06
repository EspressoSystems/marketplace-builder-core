use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    block::{BuilderStateId, ParentBlockReferences, ReceivedTransaction},
    utils::{LegacyCommit, RotatingSet},
};
use async_broadcast::Receiver;
use async_lock::{Mutex, RwLock};
use committable::Commitment;
use hotshot::traits::BlockPayload;
use hotshot_types::{
    data::{DaProposal, Leaf, QuorumProposal},
    traits::{block_contents::BlockHeader, node_implementation::NodeType},
};

#[derive(Debug, Clone)]
pub struct TransactionQueue<Types>
where
    Types: NodeType,
{
    /// txn commits currently in the `tx_queue`.  This is used as a quick
    /// check for whether a transaction is already in the `tx_queue` or
    /// not.
    ///
    /// This should be kept up-to-date with the `tx_queue` as it acts as an
    /// accessory to the `tx_queue`.
    pub commits: HashSet<Commitment<Types::Transaction>>,

    /// filtered queue of available transactions, taken from `tx_receiver`
    pub transactions: VecDeque<Arc<ReceivedTransaction<Types>>>,
}

impl<Types> Default for TransactionQueue<Types>
where
    Types: NodeType,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Types> TransactionQueue<Types>
where
    Types: NodeType,
{
    pub fn new() -> Self {
        Self {
            commits: HashSet::new(),
            transactions: VecDeque::new(),
        }
    }

    pub fn prune<'a>(&mut self, commits: impl Iterator<Item = &'a Commitment<Types::Transaction>>) {
        for commit in commits {
            self.commits.remove(commit);
        }
        self.transactions
            .retain(|txn| self.commits.contains(&txn.commit));
    }

    pub fn insert(&mut self, transaction: Arc<ReceivedTransaction<Types>>) -> bool {
        if !self.commits.contains(&transaction.commit) {
            self.commits.insert(transaction.commit);
            self.transactions.push_back(transaction);
            true
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }
}

#[derive(derive_more::Debug)]
pub struct BuilderState<Types: NodeType> {
    /// Spawned-from references to the parent block.
    pub parent_block_references: ParentBlockReferences<Types>,

    /// txns that have been included in recent blocks that have
    /// been built.  This is used to try and guarantee that a transaction
    /// isn't duplicated.
    /// Keeps a history of the last 3 proposals.
    #[debug(skip)]
    pub included_txns: RotatingSet<Commitment<Types::Transaction>>,

    /// transaction queue
    #[debug(skip)]
    pub txn_queue: RwLock<TransactionQueue<Types>>,

    #[debug(skip)]
    pub txn_receiver: Mutex<Receiver<Arc<ReceivedTransaction<Types>>>>,
}

impl<Types> BuilderState<Types>
where
    Types: NodeType,
{
    pub fn new(
        parent: ParentBlockReferences<Types>,
        txn_garbage_collect_duration: Duration,
        txn_receiver: Receiver<Arc<ReceivedTransaction<Types>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            parent_block_references: parent,
            included_txns: RotatingSet::new(txn_garbage_collect_duration),
            txn_queue: RwLock::new(TransactionQueue::new()),
            txn_receiver: Mutex::new(txn_receiver),
        })
    }

    pub fn id(&self) -> BuilderStateId<Types> {
        BuilderStateId {
            parent_view: self.parent_block_references.view_number,
            parent_commitment: self.parent_block_references.vid_commitment,
        }
    }

    pub(crate) async fn new_child(
        self: Arc<Self>,
        quorum_proposal: QuorumProposal<Types>,
        da_proposal: DaProposal<Types>,
    ) -> Arc<Self> {
        let leaf = Leaf::from_quorum_proposal(&quorum_proposal);

        // We replace our parent_block_references with information from the
        // quorum proposal.  This is identifying the block that this specific
        // instance of [BuilderState] is attempting to build for.
        let parent_block_references = ParentBlockReferences {
            view_number: quorum_proposal.view_number,
            vid_commitment: quorum_proposal.block_header.payload_commitment(),
            leaf_commit: leaf.legacy_commit(),
            builder_commitment: quorum_proposal.block_header.builder_commitment(),
        };

        let mut included_txns = self.included_txns.clone();
        included_txns.rotate();

        let encoded_txns = &da_proposal.encoded_transactions;
        let metadata = &da_proposal.metadata;

        let block_payload =
            <Types::BlockPayload as BlockPayload<Types>>::from_bytes(encoded_txns, metadata);
        let txn_commitments = block_payload.transaction_commitments(metadata);

        let mut txn_queue = self.txn_queue.read().await.clone();
        txn_queue.prune(txn_commitments.iter());

        included_txns.extend(txn_commitments.into_iter());

        Arc::new(BuilderState {
            parent_block_references,
            included_txns,
            txn_queue: RwLock::new(txn_queue),
            txn_receiver: Mutex::new(self.txn_receiver.lock().await.clone()),
        })
    }

    // collect outstanding transactions
    pub async fn collect_txns(&self, timeout_after: Instant) -> bool {
        let mut queue_empty = self.txn_queue.read().await.is_empty();
        while Instant::now() <= timeout_after {
            let mut receiver_guard = self.txn_receiver.lock().await;
            match receiver_guard.try_recv() {
                Ok(txn) => {
                    if self.included_txns.contains(&txn.commit) {
                        // We've included this transaction in one of our
                        // recent blocks, and we do not wish to include it
                        // again.
                        continue;
                    }

                    self.txn_queue.write().await.insert(txn);
                    queue_empty = false;
                }

                Err(async_broadcast::TryRecvError::Empty)
                | Err(async_broadcast::TryRecvError::Closed) => {
                    // The transaction receiver is empty, or it's been closed.
                    // If it's closed that's a big problem and we should
                    // probably indicate it as such.
                    break;
                }

                Err(async_broadcast::TryRecvError::Overflowed(lost)) => {
                    tracing::warn!("Missed {lost} transactions due to backlog");
                    continue;
                }
            }
        }
        queue_empty
    }
}
