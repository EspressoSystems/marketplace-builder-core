//! This module contains an optimizide implementation of a
//! [`BuilderStateId`] to [`BuilderState`] map.

use std::{
    collections::{btree_map::Entry, BTreeMap},
    ops::RangeBounds,
    sync::Arc,
};

use hotshot_types::{traits::node_implementation::NodeType, vid::VidCommitment};
use nonempty_collections::{nem, NEMap};

use crate::{block::BuilderStateId, state::BuilderState};

/// A map from [`BuilderStateId`] to [`BuilderState`], implemented as a tiered map
/// with the first tier being [`BTreeMap`] keyed by view number of [`BuilderStateId`]
/// and the second [`NEMap`] keyed by VID commitment of [`BuilderStateId`].
///
/// Usage of [`BTreeMap`] means that the map has an convenient property of always being
/// sorted by view number, which makes common operations such as pruning old views more efficient.
///
/// Second tier being non-empty by construction [`NEMap`] ensures that we can't accidentally
/// create phantom entries with empty maps in the first tier.
pub struct BuilderStateMap<Types: NodeType>(
    BTreeMap<<Types as NodeType>::View, NEMap<VidCommitment, Arc<BuilderState<Types>>>>,
);

impl<Types: NodeType> BuilderStateMap<Types> {
    /// Create a new empty map
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns an iterator visiting all values in this map
    pub fn values(&self) -> impl Iterator<Item = &Arc<BuilderState<Types>>> {
        self.0
            .values()
            .flat_map(|bucket| bucket.values().into_iter())
    }

    /// Returns a nested iterator visiting all [`BuilderState`]s for view numbers in given range
    pub fn range<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = impl Iterator<Item = &Arc<BuilderState<Types>>>>
    where
        R: RangeBounds<Types::View>,
    {
        self.0
            .range(range)
            .map(|(_, bucket)| bucket.values().into_iter())
    }

    /// Returns an iterator visiting all [`BuilderState`]s for given view number
    pub fn bucket(
        &self,
        view_number: &Types::View,
    ) -> impl Iterator<Item = &Arc<BuilderState<Types>>> {
        self.0
            .get(view_number)
            .into_iter()
            .flat_map(|bucket| bucket.values().into_iter())
    }

    /// Returns the number of builder states stored
    pub fn len(&self) -> usize {
        self.0.values().map(|bucket| bucket.len().get()).sum()
    }

    /// Returns whether this map is empty
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }

    /// Get builder state by ID
    pub fn get(&self, key: &BuilderStateId<Types>) -> Option<&Arc<BuilderState<Types>>> {
        self.0.get(&key.parent_view)?.get(&key.parent_commitment)
    }

    /// Get highest view builder state
    ///
    /// Returns `None` if the map is empty
    pub fn highest_view_builder(&self) -> Option<&Arc<BuilderState<Types>>> {
        Some(&self.0.last_key_value()?.1.head_val)
    }

    /// Insert a new builder state
    pub fn insert(&mut self, key: BuilderStateId<Types>, value: Arc<BuilderState<Types>>) {
        match self.0.entry(key.parent_view) {
            Entry::Vacant(entry) => {
                entry.insert(nem![key.parent_commitment => value]);
            }
            Entry::Occupied(mut entry) => {
                entry.get_mut().insert(key.parent_commitment, value);
            }
        }
    }

    /// Returns highest view number for which we have a builder state
    pub fn highest_view(&self) -> Option<Types::View> {
        Some(*self.0.last_key_value()?.0)
    }

    /// Returns lowest view number for which we have a builder state
    pub fn lowest_view(&self) -> Option<Types::View> {
        Some(*self.0.first_key_value()?.0)
    }

    /// Removes every view lower than to `cutoff_view` (exclusive) from self and returns all removed views.
    pub fn prune(&mut self, cutoff_view: Types::View) -> Self {
        let high = self.0.split_off(&cutoff_view);
        let low = std::mem::replace(&mut self.0, high);
        Self(low)
    }
}

impl<Types: NodeType> Default for BuilderStateMap<Types> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Bound, time::Duration};

    use crate::block::ParentBlockReferences;

    use super::*;
    use async_broadcast::broadcast;
    use hotshot_example_types::node_types::TestTypes;
    use hotshot_types::{data::ViewNumber, traits::node_implementation::ConsensusTime};

    type View = ViewNumber;
    type BuilderStateMap = super::BuilderStateMap<TestTypes>;

    #[test]
    fn test_new_map() {
        let map = BuilderStateMap::new();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_insert_and_get() {
        let mut map = BuilderStateMap::new();
        let view = View::new(1);
        let commitment = VidCommitment::default();
        let state_id = BuilderStateId {
            parent_view: view,
            parent_commitment: commitment,
        };
        let builder_state = create_mock_builder_state(view);

        map.insert(state_id.clone(), builder_state.clone());

        assert!(!map.is_empty());
        assert_eq!(map.len(), 1);
        assert!(Arc::ptr_eq(map.get(&state_id).unwrap(), &builder_state));
    }

    #[test]
    fn test_range_iteration() {
        let mut map = BuilderStateMap::new();
        for i in 0..5 {
            let view = View::new(i);
            let commitment = VidCommitment::default();
            let state_id = BuilderStateId {
                parent_view: view,
                parent_commitment: commitment,
            };
            map.insert(state_id, create_mock_builder_state(view));
        }
        let start = View::new(1);
        let end = View::new(3);

        let collected: Vec<_> = map
            .range((Bound::Included(start), Bound::Excluded(end)))
            .flatten()
            .collect();

        assert_eq!(collected.len(), 2);
        assert_eq!(*collected[0].parent_block_references.view_number, 1);
        assert_eq!(*collected[1].parent_block_references.view_number, 2);
    }

    #[test]
    fn test_pruning() {
        let mut map = BuilderStateMap::new();
        for i in 0..10 {
            let view = View::new(i);
            let commitment = VidCommitment::default();
            let state_id = BuilderStateId {
                parent_view: view,
                parent_commitment: commitment,
            };
            map.insert(state_id, create_mock_builder_state(view));
        }

        let pruned_map = map.prune(View::new(5));
        assert_eq!(pruned_map.len(), 5);
        assert_eq!(map.len(), 5);

        assert!(pruned_map.bucket(&View::new(4)).next().is_some());
        assert!(map.bucket(&View::new(5)).next().is_some());

        assert!(pruned_map.bucket(&View::new(5)).next().is_none());
        assert!(map.bucket(&View::new(4)).next().is_none());
    }

    #[test]
    fn test_highest_and_lowest_view() {
        let mut map = BuilderStateMap::new();
        assert_eq!(map.highest_view(), None);
        assert_eq!(map.lowest_view(), None);

        for i in 1..10 {
            let view = View::new(i);
            let commitment = VidCommitment::default();
            let state_id = BuilderStateId {
                parent_view: view,
                parent_commitment: commitment,
            };
            map.insert(state_id, create_mock_builder_state(view));
        }

        assert_eq!(*map.highest_view().unwrap(), 9);
        assert_eq!(*map.lowest_view().unwrap(), 1);
    }

    #[test]
    fn test_highest_view_builder() {
        let mut map = BuilderStateMap::new();
        assert!(map.highest_view_builder().is_none());

        let view = View::new(10);
        let commitment = VidCommitment::default();
        let state_id = BuilderStateId {
            parent_view: view,
            parent_commitment: commitment,
        };
        let builder_state = create_mock_builder_state(view);

        map.insert(state_id, builder_state.clone());

        assert!(Arc::ptr_eq(
            map.highest_view_builder().unwrap(),
            &builder_state
        ));
    }

    fn create_mock_builder_state(view: View) -> Arc<BuilderState<TestTypes>> {
        let references = ParentBlockReferences::random_with_view(view);
        let (_, receiver) = broadcast(1);
        BuilderState::new(references, Duration::from_secs(1), receiver)
    }
}
