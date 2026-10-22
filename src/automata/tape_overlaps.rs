use std::collections::{BTreeMap, BTreeSet};

use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{MultiTapeProduct, MultiTapeState, TapeNo};

/// `offset -> states reachable at that offset`
pub type OffsetOverlaps = BTreeMap<i64, BTreeSet<MultiTapeState>>;

/// Port of the python `MultiTapeStatesMap`.
///
/// Intended to represent what tape states can occur at each
/// tape from any possible source state at a given offset
/// (`tape_no -> set of tape cell states`).
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct MultiTapeStatesMap {
    whitelist: BTreeMap<TapeNo, BTreeSet<CellState>>,
}

impl MultiTapeStatesMap {
    pub fn new() -> Self {
        Self { whitelist: BTreeMap::new() }
    }

    /// Sorted tape numbers present in the map.
    pub fn get_tape_nos(&self) -> Vec<TapeNo> {
        self.whitelist.keys().copied().collect()
    }

    pub fn contains_tape(&self, tape_no: TapeNo) -> bool {
        self.whitelist.contains_key(&tape_no)
    }

    /// Python `__getitem__` (missing tapes yield an empty set).
    pub fn get(&self, tape_no: TapeNo) -> BTreeSet<CellState> {
        self.whitelist.get(&tape_no).cloned().unwrap_or_default()
    }

    pub fn get_ref(&self, tape_no: TapeNo) -> Option<&BTreeSet<CellState>> {
        self.whitelist.get(&tape_no)
    }

    /// Python `__setitem__`.
    pub fn set(&mut self, tape_no: TapeNo, states: BTreeSet<CellState>) {
        self.whitelist.insert(tape_no, states);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&TapeNo, &BTreeSet<CellState>)> {
        self.whitelist.iter()
    }

    pub fn get_multi_tape_states_for(&self, tape_no: TapeNo) -> BTreeSet<MultiTapeState> {
        self.whitelist
            .get(&tape_no)
            .map(|states| {
                states
                    .iter()
                    .map(|cell_state| MultiTapeState::new(tape_no, *cell_state))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn insert(&mut self, tape_no: TapeNo, tape_cell_state: CellState) {
        self.whitelist.entry(tape_no).or_default().insert(tape_cell_state);
    }

    pub fn insert_multi_tape_state(&mut self, state: MultiTapeState) {
        self.insert(state.tape_no, state.tape_cell_state);
    }

    pub fn insert_multi_tape_states<'a>(
        &mut self, states: impl IntoIterator<Item = &'a MultiTapeState>,
    ) {
        for state in states {
            self.insert_multi_tape_state(*state);
        }
    }

    /// For each tape (sorted by tape number), the states that can
    /// exist on said tape.
    pub fn to_flat_states(&self) -> Vec<Vec<MultiTapeState>> {
        self.get_tape_nos()
            .into_iter()
            .map(|tape_no| {
                self.get_multi_tape_states_for(tape_no).into_iter().collect()
            })
            .collect()
    }
}

/// Port of the python `TapeOverlaps`.
///
/// We say that a tape state A can overlap with tape state B at offset k if
/// in the history of the automata it is possible that:
/// 1. A is present at some position p on the tape and
/// 2. B to be present at position p + k.
///
/// All overlaps are symmetrical, since if tape state A
/// can overlap with tape state B at offset k,
/// then tape state B can overlap with tape state A at offset -k
///
/// NOTE: the python implementation is backed by `defaultdict`s, which
/// implicitly create (empty) entries whenever a missing key is accessed.
/// Those empty entries are observable (e.g. they show up as empty overlap
/// combinations in `ProductWritesMap::build_flat_offset_path_combos`, and
/// participate in equality checks between tape overlaps FSM states), so the
/// `touch*` helpers are used to replicate the python access patterns.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct TapeOverlaps {
    overlaps: BTreeMap<MultiTapeState, OffsetOverlaps>,
}

impl TapeOverlaps {
    pub fn new() -> Self {
        Self { overlaps: BTreeMap::new() }
    }

    pub fn len(&self) -> usize {
        self.overlaps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.overlaps.is_empty()
    }

    pub fn contains(&self, state: &MultiTapeState) -> bool {
        self.overlaps.contains_key(state)
    }

    /// Python `tape_overlaps[source_state]` (without creating missing entries).
    pub fn get(&self, source_state: &MultiTapeState) -> Option<&OffsetOverlaps> {
        self.overlaps.get(source_state)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&MultiTapeState, &OffsetOverlaps)> {
        self.overlaps.iter()
    }

    pub fn get_overlaps_for_offset(
        &self, source_state: &MultiTapeState, offset: i64,
    ) -> Option<&BTreeSet<MultiTapeState>> {
        self.overlaps.get(source_state)?.get(&offset)
    }

    pub fn get_all_states(&self) -> BTreeSet<MultiTapeState> {
        self.overlaps.keys().copied().collect()
    }

    pub fn get_all_tape_nos(&self) -> BTreeSet<TapeNo> {
        self.overlaps.keys().map(|state| state.tape_no).collect()
    }

    pub fn group_states_by_tape(&self) -> BTreeMap<TapeNo, BTreeSet<MultiTapeState>> {
        let mut tape_states: BTreeMap<TapeNo, BTreeSet<MultiTapeState>> = BTreeMap::new();
        for source_state in self.overlaps.keys() {
            tape_states.entry(source_state.tape_no).or_default().insert(*source_state);
        }
        tape_states
    }

    pub fn get_states_for_tape(&self, tape_no: TapeNo) -> BTreeSet<MultiTapeState> {
        self.overlaps
            .keys()
            .filter(|state| state.tape_no == tape_no)
            .copied()
            .collect()
    }

    pub fn get_cell_states_for_tape(&self, tape_no: TapeNo) -> BTreeSet<CellState> {
        self.get_states_for_tape(tape_no)
            .into_iter()
            .map(|state| state.tape_cell_state)
            .collect()
    }

    /// Equivalent of the python `self._overlaps[state]` defaultdict access.
    fn touch_state(&mut self, state: MultiTapeState) -> &mut OffsetOverlaps {
        self.overlaps.entry(state).or_default()
    }

    /// Equivalent of the python `self._overlaps[state][offset]`
    /// defaultdict access.
    fn touch(&mut self, state: MultiTapeState, offset: i64) -> &mut BTreeSet<MultiTapeState> {
        self.overlaps.entry(state).or_default().entry(offset).or_default()
    }

    /// Python `get_overlaps(source_state)[offset]`, creating missing
    /// entries like the python defaultdict implementation does.
    pub fn get_overlaps_for_offset_mut(
        &mut self, source_state: MultiTapeState, offset: i64,
    ) -> &BTreeSet<MultiTapeState> {
        self.touch(source_state, offset)
    }

    fn has_tape_mutual_exclusion(
        source_state: &MultiTapeState, target_state: &MultiTapeState, offset: i64,
    ) -> bool {
        source_state.tape_no == target_state.tape_no
            && source_state.tape_cell_state != target_state.tape_cell_state
            && offset == 0
    }

    /// Check that there are no overlaps with tape mutual exclusion
    /// i.e. if two states are on the same tape and same position
    /// but have different cell states, then they can't overlap
    fn validate_mutual_overlaps_for(&mut self, source_state: MultiTapeState) {
        let direct_overlap_states = self.touch(source_state, 0);
        debug_assert!(
            direct_overlap_states.iter().all(|overlap_state| {
                !Self::has_tape_mutual_exclusion(&source_state, overlap_state, 0)
            }),
            "Invalid overlap for {} at offset 0 due to tape mutual exclusion",
            source_state
        );
    }

    /// Every overlap should be symmetric:
    /// so if A overlaps with B at offset k,
    /// then B should also overlap with A at offset -k
    /// (only checked in debug builds)
    fn validate_symmetric_overlaps_for(&self, source_state: MultiTapeState) {
        if !cfg!(debug_assertions) {
            return;
        }
        let Some(source_overlaps_map) = self.overlaps.get(&source_state) else {
            return;
        };
        for (offset, target_states) in source_overlaps_map.iter() {
            for target_state in target_states.iter() {
                let is_symmetric = self
                    .get_overlaps_for_offset(target_state, -offset)
                    .map(|states| states.contains(&source_state))
                    .unwrap_or(false);
                assert!(
                    is_symmetric,
                    "Invalid asymmetric overlap: {} overlaps with {} at offset {}",
                    source_state, target_state, offset
                );
            }
        }
    }

    /// If state A overlaps with state B at offset k,
    /// then state B also overlaps with state A at offset -k
    ///
    /// `offset` is the offset of `target_state` FROM `source_state`
    pub fn insert_direct_overlap(
        &mut self,
        source_state: MultiTapeState,
        target_state: MultiTapeState,
        offset: i64,
    ) -> bool {
        if Self::has_tape_mutual_exclusion(&source_state, &target_state, offset) {
            // states have same-tape mutual exclusion, so can't overlap
            return false;
        }

        self.touch(source_state, offset);
        self.touch(target_state, -offset);
        if !self.touch(source_state, offset).insert(target_state) {
            // overlap already exists
            return false;
        }
        self.touch(target_state, -offset).insert(source_state);
        self.validate_mutual_overlaps_for(source_state);
        self.validate_symmetric_overlaps_for(source_state);
        true
    }

    /// `offset` is the offset of `target_state` FROM `source_state`
    pub fn remove_overlaps_for(
        &mut self,
        source_state: MultiTapeState,
        target_state: MultiTapeState,
        offset: i64,
    ) -> bool {
        self.touch_state(source_state);
        self.touch_state(target_state);
        self.touch(source_state, offset);
        self.touch(target_state, -offset);

        if !self.touch(source_state, offset).remove(&target_state) {
            // overlap doesn't exist
            return false;
        }
        self.touch(target_state, -offset).remove(&source_state);
        self.validate_mutual_overlaps_for(source_state);
        self.validate_symmetric_overlaps_for(source_state);
        true
    }

    pub fn delete_state(&mut self, state: &MultiTapeState) {
        self.overlaps.remove(state);
        for source_overlaps in self.overlaps.values_mut() {
            for offset_overlaps in source_overlaps.values_mut() {
                offset_overlaps.remove(state);
            }
        }
    }

    pub fn propagate_overlap(
        &mut self,
        source_state: MultiTapeState,
        target_state: MultiTapeState,
        offset: i64,
        min_offset: i64,
        max_offset: i64,
    ) -> bool {
        let source_updated = self.insert_overlaps_for(
            source_state, target_state, offset, Some(min_offset), Some(max_offset),
        );
        self.validate_mutual_overlaps_for(source_state);
        self.validate_symmetric_overlaps_for(source_state);

        let target_updated = self.insert_overlaps_for(
            target_state, source_state, -offset, Some(min_offset), Some(max_offset),
        );
        self.validate_mutual_overlaps_for(source_state);
        self.validate_symmetric_overlaps_for(source_state);
        source_updated || target_updated
    }

    /// `offset` is the offset of `target_state` FROM `source_state`
    pub fn insert_overlaps_for(
        &mut self,
        source_state: MultiTapeState,
        target_state: MultiTapeState,
        offset: i64,
        min_offset: Option<i64>,
        max_offset: Option<i64>,
    ) -> bool {
        let source_cell_state = source_state.tape_cell_state;
        self.touch_state(source_state);
        let target_overlap_offsets: Vec<i64> =
            self.touch_state(target_state).keys().copied().collect();
        let mut overlaps_inserted = false;

        for target_state_offset in target_overlap_offsets {
            /*
            If state A overlaps with state B at offset k,
            and state B overlaps with state C at offset m,
            then state A also overlaps with state C at offset k + m,

            So what we are doing here is to add all relevant states C
            to the overlaps of state A at relevant (shifted) offsets
            accordingly
            */
            let source_state_offset = offset + target_state_offset;
            if let Some(min_offset) = min_offset {
                if source_state_offset < min_offset {
                    continue;
                }
            }
            if let Some(max_offset) = max_offset {
                if source_state_offset > max_offset {
                    continue;
                }
            }

            self.touch(source_state, source_state_offset);
            let prev_target_overlap_states: Vec<MultiTapeState> = self
                .touch(target_state, target_state_offset)
                .iter()
                .copied()
                .collect();

            for target_overlap_state in prev_target_overlap_states {
                if self.touch(source_state, source_state_offset).contains(&target_overlap_state) {
                    continue;
                }

                let has_tape_mutual_exclusion = source_state_offset == 0
                    && target_overlap_state.tape_cell_state != source_cell_state
                    && target_overlap_state.tape_no == source_state.tape_no;
                if has_tape_mutual_exclusion {
                    // if the states have same-tape mutual exclusion,
                    // then they can't overlap
                    continue;
                }

                overlaps_inserted |= self.insert_direct_overlap(
                    source_state, target_overlap_state, source_state_offset,
                );
                debug_assert!(self
                    .touch(source_state, source_state_offset)
                    .contains(&target_overlap_state));
                /*
                The target_overlap_state also would now overlap with
                source_state at an offset of -offset
                */
                overlaps_inserted |= self.insert_direct_overlap(
                    target_overlap_state, source_state, -offset,
                );
                self.validate_mutual_overlaps_for(source_state);
                self.validate_symmetric_overlaps_for(source_state);
            }
        }

        if !Self::has_tape_mutual_exclusion(&source_state, &target_state, offset) {
            /*
            if two multi tape states are on the same tape and
            have different tape cell states then they must
            necessarily never be able to overlap with one another directly
            (i.e. with an overlap offset=0)
            */
            overlaps_inserted |= self.insert_direct_overlap(
                source_state, target_state, offset,
            );
            self.validate_mutual_overlaps_for(source_state);
        }

        self.validate_mutual_overlaps_for(source_state);
        self.validate_symmetric_overlaps_for(source_state);
        overlaps_inserted
    }

    /// `offset` is the offset of `target_state` FROM `source_state`
    pub fn can_overlap_exist(
        &self,
        source_state: &MultiTapeState,
        target_state: &MultiTapeState,
        offset: i64,
        default_value: bool,
    ) -> bool {
        let Some(source_overlaps) = self.overlaps.get(source_state) else {
            return default_value;
        };
        let Some(offset_overlaps) = source_overlaps.get(&offset) else {
            return default_value;
        };
        offset_overlaps.contains(target_state)
    }

    /// For a given offset, create a whitelist of what tape states can
    /// exist at each tape for that offset
    ///
    /// If we leave offset at zero, this conveniently returns all the possible
    /// input tape states that can exist grouped by tape no.
    pub fn create_whitelist_for_offset(&mut self, offset: i64) -> MultiTapeStatesMap {
        let mut whitelist = MultiTapeStatesMap::new();
        let source_states: Vec<MultiTapeState> = self.overlaps.keys().copied().collect();

        for source_state in source_states {
            for offset_state_overlap in self.touch(source_state, offset).iter() {
                whitelist.insert(
                    offset_state_overlap.tape_no,
                    offset_state_overlap.tape_cell_state,
                );
            }
        }
        whitelist
    }

    /// If there is overlap := (source_state, target_state, offset) in self
    /// but not in whitelist_overlaps, and whitelist_overlaps contains
    /// (source_state, states with tape_no = target_state.tape_no, offset)
    /// overlaps, then we remove said overlap.
    pub fn apply_whitelist(&mut self, whitelist_overlaps: &TapeOverlaps, verbose: bool) {
        let source_states: Vec<MultiTapeState> = self.overlaps.keys().copied().collect();

        for source_state in source_states {
            let Some(whitelisted_state_overlaps) = whitelist_overlaps.get(&source_state) else {
                /*
                This means that whitelist_overlaps makes no claims
                on what states are whitelisted to coexist with source_state
                */
                continue;
            };

            for (offset, whitelisted_offset_states) in whitelisted_state_overlaps.iter() {
                let offset = *offset;
                let offset_states: Vec<MultiTapeState> =
                    self.touch(source_state, offset).iter().copied().collect();
                let whitelisted_offset_tapes: BTreeSet<TapeNo> = whitelisted_offset_states
                    .iter()
                    .map(|state| state.tape_no)
                    .collect();

                for target_state in offset_states {
                    if whitelisted_offset_states.contains(&target_state) {
                        continue;
                    } else if !whitelisted_offset_tapes.contains(&target_state.tape_no) {
                        /*
                        Any state in the automata *has* to overlap with some
                        other state on the same tape, otherwise it would be
                        impossible for that state to exist in the automata
                        at all.

                        So if we don't specify any target_state that
                        source_state can overlap with, we take it that the
                        overlaps makes no claims about what states can
                        overlap with source_state at that offset.
                        */
                        continue;
                    }

                    if verbose {
                        println!(
                            "REMOVE_OVERLAPS ({}, {}, {})",
                            source_state, target_state, offset
                        );
                    }
                    self.remove_overlaps_for(source_state, target_state, offset);
                }
            }
        }
    }

    pub fn merge<'a>(input_tape_overlaps: impl IntoIterator<Item = &'a TapeOverlaps>) -> Self {
        let mut new_overlaps = TapeOverlaps::new();

        for tape_overlaps in input_tape_overlaps {
            for (source_state, source_overlaps) in tape_overlaps.overlaps.iter() {
                for (offset, offset_overlaps) in source_overlaps.iter() {
                    for target_state in offset_overlaps.iter() {
                        new_overlaps.insert_direct_overlap(
                            *source_state, *target_state, *offset,
                        );
                    }
                }
            }
        }
        new_overlaps
    }

    fn state_to_str(state: &MultiTapeState) -> String {
        format!("T{}:{}", state.tape_no, state.tape_cell_state)
    }

    fn list_for_offset(target_states_set: &BTreeSet<MultiTapeState>) -> String {
        let mut tape_states_map: BTreeMap<TapeNo, Vec<CellState>> = BTreeMap::new();
        for target_state in target_states_set.iter() {
            tape_states_map
                .entry(target_state.tape_no)
                .or_default()
                .push(target_state.tape_cell_state);
        }

        tape_states_map
            .iter()
            .map(|(tape_no, cell_states)| {
                let cell_states_str = cell_states
                    .iter()
                    .map(|state| state.to_string())
                    .collect::<Vec<String>>()
                    .join("|");
                format!("T{}:{}", tape_no, cell_states_str)
            })
            .collect::<Vec<String>>()
            .join(" ")
    }

    pub fn visualize_for_state(
        &self, source_state: &MultiTapeState, prefix_length: usize,
    ) -> Vec<String> {
        let mut tree_prefix = Self::state_to_str(source_state);
        let prefix_char_count = tree_prefix.chars().count();
        if prefix_length > prefix_char_count {
            tree_prefix.push_str(&" ".repeat(prefix_length - prefix_char_count));
        }

        let empty_overlaps = OffsetOverlaps::new();
        let overlap_map = self.overlaps.get(source_state).unwrap_or(&empty_overlaps);
        let mut min_offset: i64 = 0;
        let mut max_offset: i64 = 0;

        if let (Some(first), Some(last)) =
            (overlap_map.keys().next(), overlap_map.keys().next_back())
        {
            min_offset = (*first).min(0);
            max_offset = (*last).max(0);
        }

        let offset_pad_length = min_offset
            .to_string()
            .len()
            .max(max_offset.to_string().len());
        let mid_pre_line = format!("{} —", tree_prefix);
        let mut lines: Vec<String> = Vec::new();
        let empty_set: BTreeSet<MultiTapeState> = BTreeSet::new();

        for offset in min_offset..=max_offset {
            let pre_line = if offset == 0 {
                mid_pre_line.clone()
            } else {
                " ".repeat(mid_pre_line.chars().count())
            };

            let target_states_set = overlap_map.get(&offset).unwrap_or(&empty_set);
            if offset != 0 && target_states_set.is_empty() {
                // we want to not skip if offset=0 because
                // the offset=0 is also the one showing the tape state
                continue;
            }

            let target_states_str = Self::list_for_offset(target_states_set);
            let offset_str = if offset < 0 {
                offset.to_string()
            } else {
                format!("+{}", offset)
            };
            let branch_char = if offset == min_offset {
                '┌'
            } else if offset == max_offset {
                '└'
            } else {
                '|'
            };

            let offset_pad_chars =
                " ".repeat(offset_pad_length.saturating_sub(offset_str.len()));
            lines.push(format!(
                "{}{}— {}{} —> {}",
                pre_line, branch_char, offset_pad_chars, offset_str, target_states_str
            ));
        }

        lines
    }

    pub fn visualize_for_states(&self, source_states: &BTreeSet<MultiTapeState>) -> Vec<String> {
        let max_prefix_length = source_states
            .iter()
            .map(|state| Self::state_to_str(state).chars().count())
            .max()
            .unwrap_or(0);

        let mut lines: Vec<String> = Vec::new();
        for source_state in source_states.iter() {
            lines.extend(self.visualize_for_state(source_state, max_prefix_length));
            lines.push(String::new());
        }
        lines
    }

    pub fn print_for_states(&self, source_states: Option<&BTreeSet<MultiTapeState>>) {
        let all_states;
        let source_states = match source_states {
            Some(states) => states,
            None => {
                all_states = self.get_all_states();
                &all_states
            }
        };
        println!("{}", self.visualize_for_states(source_states).join("\n"));
    }
}

/// Check if the given product is satisfiable based on the
/// overlaps that exist in the automata
pub fn is_product_satisfiable(
    product: &MultiTapeProduct,
    overlaps: &TapeOverlaps,
    default_if_term_missing: bool,
) -> bool {
    let terms = &product._terms;

    if terms.len() == 1 {
        // if overlaps contain the one term, then product is satisfiable
        return overlaps.contains(&MultiTapeState::from_term(terms[0].clone()));
    }

    for pair in terms.windows(2) {
        let (term_a, term_b) = (&pair[0], &pair[1]);
        let state_a = MultiTapeState::new(term_a.state.0, term_a.state.1);
        let state_b = MultiTapeState::new(term_b.state.0, term_b.state.1);
        let relative_offset = term_b.position - term_a.position;

        if !overlaps.can_overlap_exist(
            &state_a, &state_b, relative_offset, default_if_term_missing,
        ) {
            /*
            state_b cannot possibly be found at a position offset
            of relative_offset from state_a
            */
            return false;
        }
    }

    true
}
