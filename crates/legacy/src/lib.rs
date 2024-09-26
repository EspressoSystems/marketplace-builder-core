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

// providing the core services to support above API services
pub mod builder_state;

// Core interaction with the HotShot network
pub mod service;

// tracking the testing
#[cfg(test)]
pub mod testing;

use async_compatibility_layer::channel::UnboundedReceiver;
use committable::Commitment;
use hotshot_builder_api::v0_1::builder::BuildError;
use hotshot_types::{
    data::Leaf, traits::node_implementation::NodeType, utils::BuilderCommitment, vid::VidCommitment,
};
use std::{
    collections::HashSet,
    hash::Hash,
    mem,
    time::{Duration, Instant},
};

/// A set that allows for time-based garbage collection,
/// implemented as three sets that are periodically shifted right.
/// Garage collection is triggered by calling [`Self::rotate`].
#[derive(Clone, Debug)]
pub struct RotatingSet<T>
where
    T: PartialEq + Eq + Hash + Clone,
{
    fresh: HashSet<T>,
    stale: HashSet<T>,
    expiring: HashSet<T>,
    last_rotation: Instant,
    period: Duration,
}

impl<T> RotatingSet<T>
where
    T: PartialEq + Eq + Hash + Clone,
{
    /// Construct a new `RotatingSet`
    pub fn new(period: Duration) -> Self {
        Self {
            fresh: HashSet::new(),
            stale: HashSet::new(),
            expiring: HashSet::new(),
            last_rotation: Instant::now(),
            period,
        }
    }

    /// Returns `true` if the key is contained in the set
    pub fn contains(&self, key: &T) -> bool {
        self.fresh.contains(key) || self.stale.contains(key) || self.expiring.contains(key)
    }

    /// Insert a `key` into the set. Doesn't trigger garbage collection
    pub fn insert(&mut self, value: T) {
        self.fresh.insert(value);
    }

    /// Force garbage collection, even if the time elapsed since
    ///  the last garbage collection is less than `self.period`
    pub fn force_rotate(&mut self) {
        let now_stale = mem::take(&mut self.fresh);
        let now_expiring = mem::replace(&mut self.stale, now_stale);
        self.expiring = now_expiring;
        self.last_rotation = Instant::now();
    }

    /// Trigger garbage collection.
    pub fn rotate(&mut self) -> bool {
        if self.last_rotation.elapsed() > self.period {
            self.force_rotate();
            true
        } else {
            false
        }
    }
}

impl<T> Extend<T> for RotatingSet<T>
where
    T: PartialEq + Eq + Hash + Clone,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.fresh.extend(iter)
    }
}

#[derive(Debug, Clone)]
pub struct RotatingData<T> {
    pub current: T,
    pub fresh: T,
    pub stale: T,
    pub expiring: T,
}

impl<T> Copy for RotatingData<T> where T: Copy {}

impl<T> Default for RotatingData<T>
where
    T: Default,
{
    fn default() -> Self {
        Self {
            current: T::default(),
            fresh: T::default(),
            stale: T::default(),
            expiring: T::default(),
        }
    }
}

impl<T> RotatingData<T> where T: Clone + Default {}

impl<T> RotatingData<T>
where
    T: PartialEq + Eq + Hash + Clone + Default,
{
    pub fn new() -> Self {
        Self::with_initial_value(Default::default())
    }

    pub fn cycle_clone(&self) -> Self {
        Self {
            current: T::default(),
            fresh: self.current.clone(),
            stale: self.fresh.clone(),
            expiring: self.stale.clone(),
        }
    }

    pub fn with_initial_value(value: T) -> Self {
        Self {
            current: value.clone(),
            fresh: T::default(),
            stale: T::default(),
            expiring: T::default(),
        }
    }
}

impl RotatingData<usize> {
    pub fn is_zero(&self) -> bool {
        self.current == 0 && self.fresh == 0 && self.stale == 0 && self.expiring == 0
    }
}

/// WaitAndKeep is a helper enum that allows for the lazy polling of a single
/// value from an unbound receiver.
#[derive(Debug)]
pub enum WaitAndKeep<T> {
    Keep(T),
    Wait(UnboundedReceiver<T>),
}

#[derive(Debug)]
pub(crate) enum WaitAndKeepGetError {
    FailedToResolvedVidCommitmentFromChannel,
}

impl From<WaitAndKeepGetError> for BuildError {
    fn from(e: WaitAndKeepGetError) -> Self {
        match e {
            WaitAndKeepGetError::FailedToResolvedVidCommitmentFromChannel => BuildError::Error {
                message: "failed to resolve VidCommitment from channel".to_string(),
            },
        }
    }
}

impl<T: Clone> WaitAndKeep<T> {
    /// get will return a clone of the value that is already stored within the
    /// value of WaitAndKeep::Keep if the value is already resolved.  Otherwise
    /// it will poll the next value from the channel and replace the locally
    /// stored WaitAndKeep::Wait with the resolved value as a WaitAndKeep::Keep.
    ///
    /// Note: This pattern seems very similar to a Future, and ultimately
    /// returns a future. It's not clear why this needs to be implemented
    /// in such a way and not just implemented as a boxed future.
    pub(crate) async fn get(&mut self) -> Result<T, WaitAndKeepGetError> {
        match self {
            WaitAndKeep::Keep(t) => Ok(t.clone()),
            WaitAndKeep::Wait(fut) => {
                let got = fut
                    .recv()
                    .await
                    .map_err(|_| WaitAndKeepGetError::FailedToResolvedVidCommitmentFromChannel);
                if let Ok(got) = &got {
                    let mut replace = WaitAndKeep::Keep(got.clone());
                    core::mem::swap(self, &mut replace);
                }
                got
            }
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct BlockId<Types: NodeType> {
    hash: BuilderCommitment,
    view: Types::Time,
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

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct BuilderStateId<Types: NodeType> {
    parent_commitment: VidCommitment,
    view: Types::Time,
}

impl<Types: NodeType> std::fmt::Display for BuilderStateId<Types> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BuilderState({}@{})", self.parent_commitment, *self.view)
    }
}

/// References to the parent block that is extended to spawn the new builder state.
#[derive(Debug, Clone)]
pub struct ParentBlockReferences<TYPES: NodeType> {
    pub view_number: TYPES::Time,
    pub vid_commitment: VidCommitment,
    pub leaf_commit: Commitment<Leaf<TYPES>>,
    pub builder_commitment: BuilderCommitment,
}
// implement display for the referenced info
impl<TYPES: NodeType> std::fmt::Display for ParentBlockReferences<TYPES> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "View Number: {:?}", self.view_number)
    }
}

// TODO: Update commitment calculation with the new `commit`.
// <https://github.com/EspressoSystems/marketplace-builder-core/issues/143>
trait LegacyCommit<T: NodeType> {
    fn legacy_commit(&self) -> committable::Commitment<hotshot_types::data::Leaf<T>>;
}

impl<T: NodeType> LegacyCommit<T> for hotshot_types::data::Leaf<T> {
    fn legacy_commit(&self) -> committable::Commitment<hotshot_types::data::Leaf<T>> {
        <hotshot_types::data::Leaf<T> as committable::Committable>::commit(self)
    }
}
