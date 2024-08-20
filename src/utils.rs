use std::{
    collections::HashSet,
    hash::Hash,
    mem,
    time::{Duration, Instant},
};

use hotshot_types::{
    traits::node_implementation::NodeType, utils::BuilderCommitment, vid::VidCommitment,
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
        tracing::error!("we did do the insert() for RotatingSet");
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
            tracing::error!("Real rotate happens!!!");
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

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct BlockId<TYPES: NodeType> {
    pub hash: BuilderCommitment,
    pub view: TYPES::Time,
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
    pub view: TYPES::Time,
    pub parent_commitment: VidCommitment,
}

impl<TYPES: NodeType> std::fmt::Display for BuilderStateId<TYPES> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BuilderState({}@{})", self.parent_commitment, *self.view)
    }
}
