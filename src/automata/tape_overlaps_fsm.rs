use std::collections::{BTreeSet, HashMap};

use indexmap::IndexMap;

use crate::automata::multi_tape_product_trie::MultiTapeProductTrie;
use crate::automata::product_writes_map::ProductWritesMap;
use crate::automata::tape_overlaps::TapeOverlaps;
use crate::automata::terms_multitape::{MultiTapeProduct, MultiTapeState};

/// Port of the python `TapeOverlapsFSMState`
#[derive(Debug, Clone)]
pub struct TapeOverlapsFSMState {
    tape_overlaps: TapeOverlaps,
    relevant_input_products: BTreeSet<MultiTapeProduct>,
    product_writes_map: ProductWritesMap,
    /// cached `tape_overlaps.get_all_states()`
    all_states: BTreeSet<MultiTapeState>,
}

impl PartialEq for TapeOverlapsFSMState {
    fn eq(&self, other: &Self) -> bool {
        self.all_states == other.all_states
            && self.relevant_input_products == other.relevant_input_products
            && self.tape_overlaps == other.tape_overlaps
            && self.product_writes_map == other.product_writes_map
    }
}
impl Eq for TapeOverlapsFSMState {}

impl TapeOverlapsFSMState {
    pub fn create(
        tape_overlaps: TapeOverlaps,
        relevant_input_products: BTreeSet<MultiTapeProduct>,
        mut product_writes_map: ProductWritesMap,
    ) -> Self {
        product_writes_map.freeze();
        let all_states = tape_overlaps.get_all_states();
        Self { tape_overlaps, relevant_input_products, product_writes_map, all_states }
    }

    pub fn tape_overlaps(&self) -> &TapeOverlaps {
        &self.tape_overlaps
    }

    pub fn relevant_input_products(&self) -> &BTreeSet<MultiTapeProduct> {
        &self.relevant_input_products
    }

    pub fn product_writes_map(&self) -> &ProductWritesMap {
        &self.product_writes_map
    }

    pub fn get_all_states(&self) -> &BTreeSet<MultiTapeState> {
        &self.all_states
    }

    /// Python `__iter__`: iterate over the products in the product writes map
    pub fn products(&self) -> impl Iterator<Item = &MultiTapeProduct> {
        self.product_writes_map.products()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TapeOverlapsFSMError {
    MissingState { index: usize },
    ConflictingNextState { index: usize },
}

impl std::fmt::Display for TapeOverlapsFSMError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TapeOverlapsFSMError::MissingState { index } => {
                write!(f, "Overlaps FSM state #{} does not exist", index)
            }
            TapeOverlapsFSMError::ConflictingNextState { index } => {
                write!(f, "Conflicting next state for overlaps FSM state #{}", index)
            }
        }
    }
}
impl std::error::Error for TapeOverlapsFSMError {}

/// Port of the python `TapeOverlapsFSM`.
///
/// FSM states are stored in insertion order and referred to by index.
#[derive(Debug, Clone)]
pub struct TapeOverlapsFSM {
    fsm_states: Vec<TapeOverlapsFSMState>,
    next_fsm_states: HashMap<usize, usize>,
    all_products_trie: MultiTapeProductTrie,
    prod_to_fsm_states: IndexMap<MultiTapeProduct, Vec<usize>>,
}

impl TapeOverlapsFSM {
    pub fn new(initial_fsm_state: TapeOverlapsFSMState) -> Self {
        let mut fsm = Self {
            fsm_states: vec![],
            next_fsm_states: HashMap::new(),
            all_products_trie: MultiTapeProductTrie::spawn_root(),
            prod_to_fsm_states: IndexMap::new(),
        };
        fsm.fsm_states.push(initial_fsm_state);
        fsm.propagate_to_prod_to_fsm_states(0);
        fsm
    }

    pub const INITIAL_STATE_INDEX: usize = 0;

    /// Python `__len__`: number of FSM state transitions
    pub fn len(&self) -> usize {
        self.next_fsm_states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.next_fsm_states.is_empty()
    }

    pub fn num_states(&self) -> usize {
        self.fsm_states.len()
    }

    pub fn get_state(&self, index: usize) -> Option<&TapeOverlapsFSMState> {
        self.fsm_states.get(index)
    }

    pub fn states(&self) -> &[TapeOverlapsFSMState] {
        &self.fsm_states
    }

    pub fn find_state(&self, fsm_state: &TapeOverlapsFSMState) -> Option<usize> {
        self.fsm_states.iter().position(|existing| existing == fsm_state)
    }

    pub fn contains(&self, fsm_state: &TapeOverlapsFSMState) -> bool {
        self.find_state(fsm_state).is_some()
    }

    pub fn get_next_state_index(&self, index: usize) -> Option<usize> {
        self.next_fsm_states.get(&index).copied()
    }

    /// Python `__getitem__`: indexes of the FSM states whose
    /// product writes map contains `product`
    pub fn get_fsm_states_for(&self, product: &MultiTapeProduct) -> &[usize] {
        self.prod_to_fsm_states
            .get(product)
            .map(|indexes| indexes.as_slice())
            .unwrap_or(&[])
    }

    pub fn get_prod_to_fsm_states(&self) -> &IndexMap<MultiTapeProduct, Vec<usize>> {
        &self.prod_to_fsm_states
    }

    pub fn load_matching_products(
        &self, product: &MultiTapeProduct,
    ) -> indexmap::IndexSet<MultiTapeProduct> {
        self.all_products_trie.load_matching_products(product)
    }

    pub fn search_all_nested_products(&self) -> indexmap::IndexSet<MultiTapeProduct> {
        self.all_products_trie.search_all_nested_products()
    }

    fn propagate_to_prod_to_fsm_states(&mut self, index: usize) {
        let products: Vec<MultiTapeProduct> =
            self.fsm_states[index].products().cloned().collect();
        for product in products {
            let fsm_states = self.prod_to_fsm_states.entry(product.clone()).or_default();
            if !fsm_states.contains(&index) {
                fsm_states.push(index);
            }
            self.all_products_trie.insert_product(&product);
        }
    }

    /// Returns the index of the inserted next FSM state, and whether
    /// `state_index` was newly assigned a next state
    pub fn insert(
        &mut self, state_index: usize, next_state: TapeOverlapsFSMState,
    ) -> Result<(usize, bool), TapeOverlapsFSMError> {
        if state_index >= self.fsm_states.len() {
            return Err(TapeOverlapsFSMError::MissingState { index: state_index });
        }

        if let Some(existing_next_index) = self.next_fsm_states.get(&state_index) {
            if self.fsm_states[*existing_next_index] != next_state {
                return Err(TapeOverlapsFSMError::ConflictingNextState {
                    index: state_index,
                });
            }
            return Ok((*existing_next_index, false));
        }

        let next_index = match self.find_state(&next_state) {
            Some(index) => index,
            None => {
                self.fsm_states.push(next_state);
                self.fsm_states.len() - 1
            }
        };
        self.next_fsm_states.insert(state_index, next_index);
        self.propagate_to_prod_to_fsm_states(next_index);
        Ok((next_index, true))
    }

    /// Union of the tape overlaps of every FSM state
    pub fn merge(&self) -> TapeOverlaps {
        TapeOverlaps::merge(self.fsm_states.iter().map(|state| state.tape_overlaps()))
    }
}
