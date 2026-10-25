use std::collections::{BTreeMap, BTreeSet};

pub use crate::automata::terms_multitape::{MultiTapeState};

/// `offset -> states reachable at that offset`
pub type OffsetOverlaps = BTreeMap<i64, BTreeSet<MultiTapeState>>;

/// Read-oriented port of the Python `TapeOverlaps`.
///
/// Only the accessors needed by `ProductWritesMap` are provided here; the
/// full propagation/validation logic still lives in
/// `automata_builder/tape_overlaps.py`.
#[derive(Debug, Clone, Default)]
pub struct TapeOverlaps {
    overlaps: BTreeMap<MultiTapeState, OffsetOverlaps>,
}

impl TapeOverlaps {
    pub fn new() -> Self {
        Self { overlaps: BTreeMap::new() }
    }

    pub fn contains(&self, state: &MultiTapeState) -> bool {
        self.overlaps.contains_key(state)
    }

    /// Python `tape_overlaps[source_state]`.
    pub fn get(&self, source_state: &MultiTapeState) -> Option<&OffsetOverlaps> {
        self.overlaps.get(source_state)
    }

    pub fn get_overlaps_for_offset(
        &self, source_state: &MultiTapeState, offset: i64,
    ) -> Option<&BTreeSet<MultiTapeState>> {
        self.overlaps.get(source_state)?.get(&offset)
    }

    pub fn get_all_states(&self) -> BTreeSet<MultiTapeState> {
        self.overlaps.keys().copied().collect()
    }

    /// Symmetric insert: if A overlaps B at `offset`,
    /// then B overlaps A at `-offset`.
    pub fn insert_direct_overlap(
        &mut self,
        source_state: MultiTapeState,
        target_state: MultiTapeState,
        offset: i64,
    ) -> bool {
        let has_tape_mutual_exclusion = source_state.tape_no == target_state.tape_no
            && source_state.tape_cell_state != target_state.tape_cell_state
            && offset == 0;

        if has_tape_mutual_exclusion {
            return false;
        }

        let inserted = self
            .overlaps
            .entry(source_state)
            .or_default()
            .entry(offset)
            .or_default()
            .insert(target_state);

        self.overlaps
            .entry(target_state)
            .or_default()
            .entry(-offset)
            .or_default()
            .insert(source_state);

        inserted
    }
}
