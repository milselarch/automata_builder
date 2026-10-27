//! Rust port of the python `MultiTapeBuilder.compile_tapes` pipeline
//! (see `automata_builder/rule_generator_multitape.py`), which compiles
//! a multi-tape cellular automata into an equivalent single-tape one.
//!
//! The overall pipeline is:
//! 1. build an FSM of the possible tape state overlaps over time
//!    ([`MultiTapeBuilder::build_overlaps_fsm`])
//! 2. extend the pre-existing (user declared) products so that no two
//!    products of the same priority can be simultaneously satisfiable
//!    ([`MultiTapeBuilder::build_extended_products_for`])
//! 3. generate "neutral" products (output == input) for every other
//!    combination of tape states that could exist
//!    ([`MultiTapeBuilder::build_product_same_writes_map`])
//! 4. remap every combination of tape states along the same position to
//!    a single (global) tape cell state
//!    ([`MultiTapeBuilder::build_global_state_path_remap`])
//! 5. generate the single-tape transitions for every product
//!    ([`MultiTapeBuilder::build_transitions_for_product`])
//!
//! Unlike the python implementation (whose results depend on `set`
//! iteration order), every collection here is iterated in a deterministic
//! (sorted / insertion) order, so compilation results are reproducible.

use std::backtrace::Backtrace;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

use indexmap::{IndexMap, IndexSet};
use rayon::prelude::*;

use crate::automata::multi_tape_automata::MultiTapeAutomata;
use crate::automata::multi_tape_product_trie::{
    offset_group_path_to_product, terms_satisfiable_with, MultiTapeProductTrie,
    OffsetGroupedTerms, TrieError,
};
use crate::automata::multi_tape_transition::TransitionPriority;
use crate::automata::product_writes_map::{ProductWritesError, ProductWritesMap, TapeWrites};
use crate::automata::rule_generator::{AutomataTransitionsGroup, HALT_STATE, VOID_STATE};
use crate::automata::tape_overlaps::{
    is_product_satisfiable, MultiTapeStatesMap, TapeOverlaps,
};
use crate::automata::tape_overlaps_fsm::{
    TapeOverlapsFSM, TapeOverlapsFSMError, TapeOverlapsFSMState,
};
use crate::automata::terms::{CellState, Term};
use crate::automata::terms_multitape::{
    AbstractMultiTapeExpression, MultiTapeProduct, MultiTapeProductFactory, MultiTapeState,
    MultiTapeTerm, TapeNo,
};

/// Remapped (global) tape cell states start after VOID (0) and HALT (1)
pub const REMAP_COUNTER_START: CellState = 2;

pub type StatePath = Vec<MultiTapeState>;

fn product_repr(product: &MultiTapeProduct) -> String {
    product._to_string("D")
}

fn terms_repr(terms: &[Term]) -> String {
    let terms_str = terms
        .iter()
        .map(|term| format!("A({},{})", term.position, term.state))
        .collect::<Vec<String>>()
        .join(", ");
    format!("({})", terms_str)
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CompileTapesError {
    ProductWrites(ProductWritesError),
    Trie(TrieError),
    OverlapsFSM(TapeOverlapsFSMError),
    /// a remap lookup for a tape state path / remapped state failed
    MissingRemap(String),
    /// a remapped tape cell state was the HALT state
    HaltState(CellState),
    /// conflicting writes between simultaneously satisfiable products
    ConflictingWrites(String),
    /// invalid / conflicting single-tape transitions
    Transition(String),
    /// a pre-existing product has no transition priority
    MissingPriority(String),
    /// any other inconsistency
    Invalid(String),
}

impl fmt::Display for CompileTapesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileTapesError::ProductWrites(err) => write!(f, "{}", err),
            CompileTapesError::Trie(err) => write!(f, "{}", err),
            CompileTapesError::OverlapsFSM(err) => write!(f, "{}", err),
            CompileTapesError::MissingRemap(msg) => write!(f, "Missing remap: {}", msg),
            CompileTapesError::HaltState(state) => {
                write!(f, "Unexpected halt state {}", state)
            }
            CompileTapesError::ConflictingWrites(msg) => write!(f, "{}", msg),
            CompileTapesError::Transition(msg) => write!(f, "{}", msg),
            CompileTapesError::MissingPriority(product) => {
                write!(f, "Missing transition priority for product {}", product)
            }
            CompileTapesError::Invalid(msg) => write!(f, "{}", msg),
        }
    }
}
impl std::error::Error for CompileTapesError {}

impl From<ProductWritesError> for CompileTapesError {
    fn from(err: ProductWritesError) -> Self {
        CompileTapesError::ProductWrites(err)
    }
}
impl From<TrieError> for CompileTapesError {
    fn from(err: TrieError) -> Self {
        CompileTapesError::Trie(err)
    }
}
impl From<TapeOverlapsFSMError> for CompileTapesError {
    fn from(err: TapeOverlapsFSMError) -> Self {
        CompileTapesError::OverlapsFSM(err)
    }
}

pub type CompileResult<T> = Result<T, CompileTapesError>;

/// Port of the python `MultiTapeStatePathRemap`.
///
/// Contains a remap of possible tape cell state combinations
/// along the exact same position across all tapes
/// to global tape cell state values
#[derive(Debug, Clone)]
pub struct MultiTapeStatePathRemap {
    remap_counter_start: CellState,
    void_state_paths: BTreeSet<StatePath>,
    halt_state_paths: BTreeSet<StatePath>,
    tape_state_path_remap: IndexMap<StatePath, CellState>,
    rev_state_path_remap: HashMap<CellState, StatePath>,
}

impl MultiTapeStatePathRemap {
    pub fn new(remap_counter_start: CellState) -> Self {
        assert!(remap_counter_start >= REMAP_COUNTER_START);
        Self {
            remap_counter_start,
            void_state_paths: BTreeSet::new(),
            halt_state_paths: BTreeSet::new(),
            tape_state_path_remap: IndexMap::new(),
            rev_state_path_remap: HashMap::new(),
        }
    }

    pub fn remap_counter_start(&self) -> CellState {
        self.remap_counter_start
    }

    pub fn num_normal_remaps(&self) -> usize {
        self.tape_state_path_remap.len()
    }

    pub fn num_void_remaps(&self) -> usize {
        self.void_state_paths.len()
    }

    pub fn num_halt_remaps(&self) -> usize {
        self.halt_state_paths.len()
    }

    pub fn remap_counter_end(&self) -> CellState {
        self.remap_counter_start + self.num_normal_remaps() as CellState
    }

    pub fn next_free_counter(&self) -> CellState {
        self.remap_counter_end() + 1
    }

    pub fn tape_state_path_remap(&self) -> &IndexMap<StatePath, CellState> {
        &self.tape_state_path_remap
    }

    pub fn void_state_paths(&self) -> &BTreeSet<StatePath> {
        &self.void_state_paths
    }

    pub fn halt_state_paths(&self) -> &BTreeSet<StatePath> {
        &self.halt_state_paths
    }

    /// The remapped state is VOID if all the multi-tape cell states
    /// passed in are void also
    pub fn is_void_path(path: &[MultiTapeState]) -> bool {
        path.iter().all(|state| state.tape_cell_state == VOID_STATE)
    }

    /// The remapped state is HALT if any of the multi-tape cell states
    /// passed in are HALT also
    pub fn is_halt_path(path: &[MultiTapeState]) -> bool {
        path.iter().any(|state| state.tape_cell_state == HALT_STATE)
    }

    /// Returns `None` if the path has already been (normally) remapped
    pub fn insert_overlap_path(&mut self, path: StatePath) -> Option<CellState> {
        if self.tape_state_path_remap.contains_key(&path) {
            return None;
        }
        if Self::is_halt_path(&path) {
            self.halt_state_paths.insert(path);
            Some(HALT_STATE)
        } else if Self::is_void_path(&path) {
            self.void_state_paths.insert(path);
            Some(VOID_STATE)
        } else {
            let new_tape_state = self.remap_counter_end();
            self.tape_state_path_remap.insert(path.clone(), new_tape_state);
            self.rev_state_path_remap.insert(new_tape_state, path);
            Some(new_tape_state)
        }
    }

    /// Remaps a combination of multi tape cell states to
    /// a single tape cell state
    pub fn remap(&self, state_path: &[MultiTapeState]) -> CompileResult<CellState> {
        if Self::is_halt_path(state_path) {
            return Ok(HALT_STATE);
        }
        if Self::is_void_path(state_path) {
            return Ok(VOID_STATE);
        }

        let state_path_str = state_path
            .iter()
            .map(|state| state.to_string())
            .collect::<Vec<String>>()
            .join(", ");

        self.tape_state_path_remap
            .get(state_path)
            .copied()
            .ok_or_else(|| CompileTapesError::MissingRemap(format!(
                "no remap for state path [{}] - backtrace: \n{}",
                state_path_str, Backtrace::capture()
            )))
    }

    /// Returns `Ok(None)` for the HALT state
    pub fn rev_lookup(&self, tape_cell_state: CellState) -> CompileResult<Option<&StatePath>> {
        if tape_cell_state == HALT_STATE {
            return Ok(None);
        }
        if tape_cell_state == VOID_STATE {
            if self.void_state_paths.len() != 1 {
                return Err(CompileTapesError::MissingRemap(format!(
                    "expected exactly one void state path, found {}",
                    self.void_state_paths.len()
                )));
            }
            return Ok(self.void_state_paths.iter().next());
        }
        self.rev_state_path_remap
            .get(&tape_cell_state)
            .map(Some)
            .ok_or_else(|| CompileTapesError::MissingRemap(format!(
                "no state path for remapped state {}", tape_cell_state
            )))
    }

    pub fn get_all_state_paths(&self) -> BTreeSet<StatePath> {
        let mut all_paths: BTreeSet<StatePath> =
            self.tape_state_path_remap.keys().cloned().collect();
        all_paths.extend(self.void_state_paths.iter().cloned());
        all_paths.extend(self.halt_state_paths.iter().cloned());
        all_paths
    }

    /// All remapped tape cell state values that have been allocated
    pub fn get_all_remap_states(&self) -> BTreeSet<CellState> {
        let mut remap_states: BTreeSet<CellState> =
            self.tape_state_path_remap.values().copied().collect();
        if !self.void_state_paths.is_empty() {
            remap_states.insert(VOID_STATE);
        }
        if !self.halt_state_paths.is_empty() {
            remap_states.insert(HALT_STATE);
        }
        remap_states
    }

    /// Resolve a term in the composed automata to a multi-tape product
    /// with the corresponding tape states for each tape in the original
    /// multi-tape automata (`Err(HaltState)` for the HALT state).
    pub fn remap_term_to_multi_tape(&self, input_term: &Term) -> CompileResult<MultiTapeProduct> {
        let position = input_term.position;
        let state_path = self
            .rev_lookup(input_term.state)?
            .ok_or(CompileTapesError::HaltState(input_term.state))?;
        let terms: Vec<MultiTapeTerm> = state_path
            .iter()
            .map(|state| MultiTapeTerm::new(position, state.to_multi_tape_cell_state(), false))
            .collect();
        Ok(MultiTapeProduct::new(terms))
    }

    pub fn remap_single_tape_terms(
        &self, terms: &[Term], annotation: &str,
    ) -> CompileResult<MultiTapeProduct> {
        let mut sub_products: Vec<MultiTapeProduct> = Vec::with_capacity(terms.len());
        for term in terms.iter() {
            sub_products.push(self.remap_term_to_multi_tape(term)?);
        }
        let mut combined_product = MultiTapeProduct::merge_products(sub_products);
        combined_product.set_annotation(annotation.to_string());
        Ok(combined_product)
    }

    /// Remap a multi-tape product whose terms all lie on the same position
    /// to the corresponding (single tape) remapped term
    pub fn remap_from_product_to_term(&self, product: &MultiTapeProduct) -> CompileResult<Term> {
        let positions: BTreeSet<i64> = product._terms.iter().map(|term| term.position).collect();
        if positions.len() != 1 {
            return Err(CompileTapesError::Invalid(format!(
                "Cannot remap from product with multiple positions: {}",
                product_repr(product)
            )));
        }
        let position = *positions.iter().next().unwrap();
        let state_path: StatePath = product
            ._terms
            .iter()
            .map(|term| MultiTapeState::new(term.state.0, term.state.1))
            .collect();
        Ok(Term::new(position, self.remap(&state_path)?, false))
    }
}

/// Port of the python `CompileTapesResult`
#[derive(Debug, Clone)]
pub struct CompileTapesResult {
    pub transitions_group: AutomataTransitionsGroup,
    pub state_remap: MultiTapeStatePathRemap,
    pub num_unrepresentable_products: usize,
}

impl CompileTapesResult {
    pub fn count_unique_states(&self) -> usize {
        self.state_remap.get_all_remap_states().len()
    }
}

/// States that disappear after a time step, the product writes map
/// with all permanently unsatisfiable products removed, and
/// a whitelist of overlaps for newly spawned states
#[derive(Debug, Clone)]
pub struct TransitionOptimizations {
    pub disappeared_states: BTreeSet<MultiTapeState>,
    pub new_prod_to_state_map: ProductWritesMap,
    pub whitelist_overlaps: TapeOverlaps,
}

/// map state -> products that write said state as their output
pub type StatesWritten = BTreeMap<MultiTapeState, BTreeSet<MultiTapeProduct>>;

/// Shared (read-only) data for generating the single tape transitions
struct TransitionsContext<'a> {
    all_tape_nos: Vec<TapeNo>,
    product_writes_map: &'a ProductWritesMap,
    all_tape_states_per_tape: &'a MultiTapeStatesMap,
    global_overlaps: &'a TapeOverlaps,
    global_state_path_remap: &'a MultiTapeStatePathRemap,
    preexisting_products: &'a MultiTapeProductTrie,
    representability: RepresentabilityIndex<'a>,
}

/// Precomputed `product -> states of FSM states containing the product`
/// lookups used to check if a multi-tape product is representable
struct RepresentabilityIndex<'a> {
    entries: Vec<(&'a [MultiTapeTerm], Vec<&'a BTreeSet<MultiTapeState>>)>,
}

impl<'a> RepresentabilityIndex<'a> {
    fn new(tape_overlaps_fsm: &'a TapeOverlapsFSM) -> Self {
        let entries = tape_overlaps_fsm
            .get_prod_to_fsm_states()
            .iter()
            .map(|(product, fsm_state_indexes)| {
                let fsm_states = fsm_state_indexes
                    .iter()
                    .map(|index| tape_overlaps_fsm.get_state(*index).unwrap().get_all_states())
                    .collect();
                (product._terms.as_slice(), fsm_states)
            })
            .collect();
        Self { entries }
    }

    /// whether there exists an FSM state containing a product that
    /// is satisfiable with `multi_tape_product`, where every tape state
    /// in `multi_tape_product` can exist in the FSM state
    fn is_representable(&self, multi_tape_product: &MultiTapeProduct) -> bool {
        let product_states: BTreeSet<MultiTapeState> = multi_tape_product
            ._terms
            .iter()
            .map(|term| MultiTapeState::new(term.state.0, term.state.1))
            .collect();

        for (product_terms, fsm_states) in self.entries.iter() {
            if !terms_satisfiable_with(product_terms, &multi_tape_product._terms) {
                continue;
            }
            for fsm_state_states in fsm_states.iter() {
                if product_states.is_subset(fsm_state_states) {
                    return true;
                }
            }
        }
        false
    }
}

/// Iterate over the cartesian product of `sequences`
/// (first sequence is the most significant)
fn cartesian_product<T: Clone>(sequences: &[Vec<T>]) -> Vec<Vec<T>> {
    let mut combos: Vec<Vec<T>> = vec![Vec::with_capacity(sequences.len())];
    for sequence in sequences.iter() {
        let mut next_combos: Vec<Vec<T>> = Vec::with_capacity(combos.len() * sequence.len());
        for combo in combos.iter() {
            for item in sequence.iter() {
                let mut extended = combo.clone();
                extended.push(item.clone());
                next_combos.push(extended);
            }
        }
        combos = next_combos;
    }
    combos
}

/// Port of the python `MultiTapeBuilder`
#[derive(Debug, Clone)]
pub struct MultiTapeBuilder {
    prod_to_state_map: ProductWritesMap,
    prod_priority_map: IndexMap<MultiTapeProduct, TransitionPriority>,
    tape_nos: Vec<TapeNo>,
    leftmost_extent: i64,
    rightmost_extent: i64,
    /// tape state -> (relative) position -> overlapping tape states
    initial_overlaps: TapeOverlaps,
}

impl MultiTapeBuilder {
    pub fn new(automata: &MultiTapeAutomata) -> Self {
        Self::from_parts(
            automata.get_prod_to_state_map(),
            automata.get_prod_priority_map().clone(),
            automata.get_tape_nos(),
            automata.leftmost_extent(),
            automata.rightmost_extent(),
        )
    }

    pub fn from_parts(
        prod_to_state_map: ProductWritesMap,
        prod_priority_map: IndexMap<MultiTapeProduct, TransitionPriority>,
        tape_nos: Vec<TapeNo>,
        leftmost_extent: i64,
        rightmost_extent: i64,
    ) -> Self {
        let mut builder = Self {
            prod_to_state_map,
            prod_priority_map,
            tape_nos,
            leftmost_extent,
            rightmost_extent,
            initial_overlaps: TapeOverlaps::new(),
        };
        // declare that void states can overlap with one another
        let void_overlap_states: BTreeSet<MultiTapeState> = builder
            .tape_nos
            .iter()
            .map(|tape_no| MultiTapeState::new(*tape_no, VOID_STATE))
            .collect();
        builder.declare_initial_group_overlaps(&void_overlap_states);
        builder
    }

    pub fn leftmost_extent(&self) -> i64 {
        self.leftmost_extent
    }

    pub fn rightmost_extent(&self) -> i64 {
        self.rightmost_extent
    }

    pub fn get_tape_nos(&self) -> Vec<TapeNo> {
        self.tape_nos.clone()
    }

    pub fn get_prod_to_state_map(&self) -> &ProductWritesMap {
        &self.prod_to_state_map
    }

    pub fn get_initial_overlaps(&self) -> &TapeOverlaps {
        &self.initial_overlaps
    }

    /// Declare that every state in overlap_states
    /// can overlap with any other state at any relative offset
    /// in the initial automata tape
    ///
    /// (i.e. if A is present at some position p on the tape, then B can
    /// also plausibly be present at position p + k at some point in the
    /// history of the tape)
    pub fn declare_initial_group_overlaps(&mut self, overlap_states: &BTreeSet<MultiTapeState>) {
        for offset in self.leftmost_extent..=self.rightmost_extent {
            for state in overlap_states.iter() {
                for other_state in overlap_states.iter() {
                    /*
                    every tape state could overlap with any other tape state
                    at any offset within the range of possible offsets
                    covered across all the automata's rules
                    */
                    self.initial_overlaps.insert_direct_overlap(*state, *other_state, offset);
                }
                for tape_no in self.tape_nos.iter() {
                    if *tape_no == state.tape_no && offset == 0 {
                        /*
                        a tape state can't overlap directly
                        on the same position with void on the same tape
                        */
                        continue;
                    }
                    // every tape state can overlap with void at any offset
                    let tape_void = MultiTapeState::new(*tape_no, VOID_STATE);
                    self.initial_overlaps.insert_direct_overlap(*state, tape_void, offset);
                }
            }
        }
    }

    /// If for some newly spawned state_written that did not exist in the
    /// previous tape overlaps, and if for some offset e, every contributing
    /// product to the state_written has a translated variant that also
    /// writes to the same tape as state_written at said offset e,
    /// then we know that state_written can only overlap with the states
    /// produced by the translated variant products
    pub fn build_whitelist_overlaps(
        overlaps_fsm_state: &TapeOverlapsFSMState,
        states_written: &StatesWritten,
        verbose: bool,
    ) -> CompileResult<TapeOverlaps> {
        let prod_to_state_map = overlaps_fsm_state.product_writes_map();
        // all states that could exist at the start of current time step
        let all_prev_overlap_states = overlaps_fsm_state.get_all_states();
        let mut whitelist_overlaps = TapeOverlaps::new();

        for (state_written, writing_products) in states_written.iter() {
            if all_prev_overlap_states.contains(state_written) {
                continue;
            }
            let state_written_tape_no = state_written.tape_no;
            if verbose {
                println!("NEW SPAWNED STATE {}", state_written);
            }

            /*
            maps write_offset -> products whose outputs are offset by
            write_offset relative from the products that wrote state_written
            */
            let mut covering_products: BTreeMap<i64, BTreeSet<MultiTapeProduct>> =
                BTreeMap::new();

            for contributing_product in writing_products.iter() {
                let translated_prods =
                    prod_to_state_map.get_translated_variants(contributing_product, None)?;
                for (translated_prod, terms_offset) in translated_prods.into_iter() {
                    let writes_tape = prod_to_state_map
                        .get(&translated_prod)
                        .map(|writes| writes.contains_key(&state_written_tape_no))
                        .unwrap_or(false);
                    if !writes_tape {
                        continue;
                    }
                    /*
                    if for a translated_product the input term positions
                    that make it up are displaced by term_offset
                    relative to the input terms of the contributing_product,
                    then its output will be displaced by
                    (write_offset := -terms_offset) from the output position
                    of the contributing_product that spawned state_written
                    */
                    let write_offset = -terms_offset;
                    covering_products.entry(write_offset).or_default().insert(translated_prod);
                }
            }

            for (write_offset, translated_prods) in covering_products.iter() {
                if *write_offset == 0 {
                    continue;
                }
                if translated_prods.len() != writing_products.len() {
                    // translated products don't cover
                    // all products contributing to the state_written
                    continue;
                }
                for translated_prod in translated_prods.iter() {
                    let written_tape_cell_state = *prod_to_state_map
                        .get(translated_prod)
                        .and_then(|writes| writes.get(&state_written_tape_no))
                        .ok_or_else(|| CompileTapesError::Invalid(format!(
                            "missing write for {}", product_repr(translated_prod)
                        )))?;
                    let target_state =
                        MultiTapeState::new(state_written_tape_no, written_tape_cell_state);
                    if verbose {
                        println!(
                            "WHITE_INS ({}, {}, {})", state_written, target_state, write_offset
                        );
                    }
                    whitelist_overlaps.insert_direct_overlap(
                        *state_written, target_state, *write_offset,
                    );
                }
            }
        }
        Ok(whitelist_overlaps)
    }

    pub fn create_optimizations(
        start_overlaps_fsm_state: &TapeOverlapsFSMState,
        states_written: &StatesWritten,
        verbose: bool,
    ) -> CompileResult<TransitionOptimizations> {
        let prev_overlaps = start_overlaps_fsm_state.tape_overlaps();
        let prev_prod_to_state_map = start_overlaps_fsm_state.product_writes_map();
        // all states that could exist at the start of current time step
        let all_prev_overlap_states = start_overlaps_fsm_state.get_all_states();
        // states that cease to exist in tapes after current time step
        let mut disappeared_states: BTreeSet<MultiTapeState> = BTreeSet::new();

        for prev_overlap_state in all_prev_overlap_states.iter() {
            let current_state_attrs = prev_prod_to_state_map.get_state_attributes(
                *prev_overlap_state, Some(all_prev_overlap_states), prev_overlaps,
            );
            // whether state has no occurrences after the current time step
            let no_state_occurrences_post_transition = current_state_attrs.instant_delete
                && !states_written.contains_key(prev_overlap_state);
            if no_state_occurrences_post_transition {
                if verbose {
                    println!("DISAPPEARED STATE {}", prev_overlap_state);
                }
                disappeared_states.insert(*prev_overlap_state);
            }
        }

        let whitelist_overlaps = Self::build_whitelist_overlaps(
            start_overlaps_fsm_state, states_written, verbose,
        )?;

        // remove products that will never be made satisfiable after
        // the current time step
        let mut prod_to_state_map = prev_prod_to_state_map.to_unfrozen();
        let state_attrs_map = prod_to_state_map.build_all_state_attrs_map(None, prev_overlaps);
        let deleted_products = prod_to_state_map.purge_unsatisfiable_products(&state_attrs_map)?;
        if verbose {
            for product in deleted_products.iter() {
                println!("DELETED_PRODUCT {} {}", product_repr(product), product.get_annotation());
            }
        }

        Ok(TransitionOptimizations {
            disappeared_states,
            new_prod_to_state_map: prod_to_state_map,
            whitelist_overlaps,
        })
    }

    /// Determine the states that would be spawned in the current
    /// timestep given the products and tape overlaps
    /// in the overlaps_fsm_state of the previous timestep
    pub fn determine_state_written(
        overlaps_fsm_state: &TapeOverlapsFSMState, verbose: bool,
    ) -> StatesWritten {
        let prev_overlaps = overlaps_fsm_state.tape_overlaps();
        let prev_prod_to_state_map = overlaps_fsm_state.product_writes_map();
        let mut states_written: StatesWritten = BTreeMap::new();

        for product in overlaps_fsm_state.relevant_input_products().iter() {
            if !is_product_satisfiable(product, prev_overlaps, false) {
                if verbose {
                    println!("NO_SAT <<< {} {}", product_repr(product), product.get_annotation());
                }
                continue;
            }
            if verbose {
                println!("IS_SAT >>> {} {}", product_repr(product), product.get_annotation());
            }
            let Some(product_writes) = prev_prod_to_state_map.get(product) else {
                continue;
            };
            for (write_tape_no, output_tape_cell_state) in product_writes.iter() {
                let output_state = MultiTapeState::new(*write_tape_no, *output_tape_cell_state);
                states_written.entry(output_state).or_default().insert(product.clone());
            }
        }
        states_written
    }

    pub fn transition_overlaps(
        &self, start_overlaps_fsm_state: &TapeOverlapsFSMState, verbose: bool,
    ) -> CompileResult<TapeOverlapsFSMState> {
        let prev_overlaps = start_overlaps_fsm_state.tape_overlaps();
        let prev_prod_to_state_map = start_overlaps_fsm_state.product_writes_map();
        let input_state_to_prod_map = prev_prod_to_state_map.build_input_state_to_prod_map();
        if verbose {
            prev_overlaps.print_for_states(None);
        }

        let mut new_relevant_input_products: BTreeSet<MultiTapeProduct> = BTreeSet::new();
        let mut overlaps = prev_overlaps.clone();
        /*
        Collection of states that were spawned in the current timestep
        - note that this does not make any claims on whether the states
        written here did not already exist in the automata / overlaps
        at the start of timestep
        */
        let states_written = Self::determine_state_written(start_overlaps_fsm_state, verbose);
        let optimizations =
            Self::create_optimizations(start_overlaps_fsm_state, &states_written, verbose)?;

        for (output_state, writing_products) in states_written.iter() {
            for product in writing_products.iter() {
                let mut overlaps_updated = false;
                for input_term in product._terms.iter() {
                    // Insert overlaps between the products' constituent
                    // input states and the output state it writes to
                    let input_state = MultiTapeState::new(input_term.state.0, input_term.state.1);
                    let term_offset_from_input = -input_term.position;
                    overlaps_updated |= overlaps.propagate_overlap(
                        input_state,
                        *output_state,
                        term_offset_from_input,
                        self.leftmost_extent,
                        self.rightmost_extent,
                    );
                }

                if !overlaps_updated {
                    continue;
                }
                if verbose {
                    println!(
                        "DO_WRITE ({}, {})", output_state.tape_no, output_state.tape_cell_state
                    );
                }
                // Get the other products that use the current products'
                // output state as one of their input states and add it
                // to list of products to check for satisfiability later
                if let Some(affected_products) = input_state_to_prod_map.get(output_state) {
                    for affected_product in affected_products.iter() {
                        new_relevant_input_products.insert(affected_product.clone());
                    }
                }
            }
        }

        for disappeared_state in optimizations.disappeared_states.iter() {
            overlaps.delete_state(disappeared_state);
        }
        if verbose {
            println!(
                "disappeared_states={:?}",
                optimizations.disappeared_states.iter().map(|s| s.to_string()).collect::<Vec<_>>()
            );
        }
        overlaps.apply_whitelist(&optimizations.whitelist_overlaps, verbose);

        Ok(TapeOverlapsFSMState::create(
            overlaps,
            new_relevant_input_products,
            optimizations.new_prod_to_state_map,
        ))
    }

    /// Builds a mapping of which tape states can overlap with
    /// which other tape states at what relative offsets
    pub fn build_overlaps_fsm(&self, verbose: bool) -> CompileResult<TapeOverlapsFSM> {
        // map input products to output tape writes
        let prod_to_state_map = self.prod_to_state_map.to_unfrozen();
        let relevant_input_products: BTreeSet<MultiTapeProduct> =
            prod_to_state_map.products().cloned().collect();
        let initial_fsm_state = TapeOverlapsFSMState::create(
            self.initial_overlaps.clone(), relevant_input_products, prod_to_state_map,
        );

        let mut overlaps_fsm = TapeOverlapsFSM::new(initial_fsm_state);
        let mut prev_fsm_state_index = TapeOverlapsFSM::INITIAL_STATE_INDEX;
        let mut round_no: usize = 0;

        loop {
            if verbose {
                println!("NEXT_ROUND: {}\n", round_no);
            }
            round_no += 1;
            let prev_fsm_state = overlaps_fsm.get_state(prev_fsm_state_index).unwrap();
            let next_fsm_state = self.transition_overlaps(prev_fsm_state, verbose)?;
            let (next_index, overlaps_fsm_updated) =
                overlaps_fsm.insert(prev_fsm_state_index, next_fsm_state)?;
            if verbose {
                println!("overlaps_fsm_updated={}", overlaps_fsm_updated);
            }
            prev_fsm_state_index = next_index;
            if !overlaps_fsm_updated {
                break;
            }
        }

        if verbose {
            println!("overlaps FSM has {} states", overlaps_fsm.len());
        }
        Ok(overlaps_fsm)
    }

    /// For every tape (in ascending order), the (sorted) states that
    /// can exist on said tape. Returns the cartesian product of those.
    fn build_tape_state_combos(overlaps: &TapeOverlaps) -> Vec<Vec<MultiTapeState>> {
        let states_by_tape: Vec<Vec<MultiTapeState>> = overlaps
            .group_states_by_tape()
            .into_values()
            .map(|states| states.into_iter().collect())
            .collect();
        cartesian_product(&states_by_tape)
    }

    fn combo_to_group(combo: &[MultiTapeState], offset: i64) -> OffsetGroupedTerms {
        let terms: Vec<MultiTapeTerm> = combo
            .iter()
            .map(|state| MultiTapeTerm::new(offset, state.to_multi_tape_cell_state(), false))
            .collect();
        OffsetGroupedTerms::new(terms, offset)
    }

    fn _build_product_same_writes(
        combos: &[Vec<MultiTapeState>],
        current_product_path: &mut Vec<OffsetGroupedTerms>,
        product_exclusions: &MultiTapeProductTrie,
        products: &mut Vec<MultiTapeProduct>,
    ) -> CompileResult<()> {
        if product_exclusions.has_end_product() {
            /*
            current product path is covered by a pre-existing product,
            so we don't need to build it
            */
            return Ok(());
        }
        if !product_exclusions.has_nested_products() {
            /*
            current_product_path is not covered by a pre-existing product
            in any subcase, so we can build and insert it
            */
            products.push(offset_group_path_to_product(current_product_path));
            return Ok(());
        }

        let next_offsets: Vec<Option<i64>> = if current_product_path.is_empty() {
            vec![Some(0)]
        } else {
            product_exclusions.get_next_offsets()
        };

        for next_offset in next_offsets.into_iter().flatten() {
            for combo in combos.iter() {
                let offset_group = Self::combo_to_group(combo, next_offset);
                let next_exclusions = product_exclusions.advance_exclusions(&offset_group)?;
                current_product_path.push(offset_group);
                Self::_build_product_same_writes(
                    combos, current_product_path, &next_exclusions, products,
                )?;
                current_product_path.pop();
            }
        }
        Ok(())
    }

    /// Generate a mapping of all possible product combinations
    /// to an output state that is the same as the previous input state,
    /// excluding products covered by `product_exclusions`.
    ///
    /// The products generated here will transition every combination
    /// of term states along the write position offset to itself,
    /// (so no change from input to output)
    pub fn build_product_same_writes_map(
        overlaps: &TapeOverlaps, product_exclusions: &MultiTapeProductTrie,
    ) -> CompileResult<ProductWritesMap> {
        let combos = Self::build_tape_state_combos(overlaps);
        let mut product_writes_map = ProductWritesMap::new();

        let root_products: Vec<MultiTapeProduct> = if product_exclusions.has_end_product() {
            vec![]
        } else if !product_exclusions.has_nested_products() {
            // empty product path
            vec![]
        } else {
            // parallelize over the combos at the root (output) offset
            let sub_products: Vec<CompileResult<Vec<MultiTapeProduct>>> = combos
                .par_iter()
                .map(|combo| {
                    let offset_group = Self::combo_to_group(combo, 0);
                    let next_exclusions = product_exclusions.advance_exclusions(&offset_group)?;
                    let mut current_product_path = vec![offset_group];
                    let mut products: Vec<MultiTapeProduct> = vec![];
                    Self::_build_product_same_writes(
                        &combos, &mut current_product_path, &next_exclusions, &mut products,
                    )?;
                    Ok(products)
                })
                .collect();

            let mut all_products: Vec<MultiTapeProduct> = vec![];
            for products in sub_products.into_iter() {
                all_products.extend(products?);
            }
            all_products
        };

        for product in root_products.iter() {
            product_writes_map.insert_neutral_product(product)?;
        }
        Ok(product_writes_map)
    }

    /// Neutral products (output == input) for every combination of
    /// tape states (across all tapes) that can exist at the same position
    pub fn build_self_writes_map(global_overlaps: &TapeOverlaps) -> CompileResult<ProductWritesMap> {
        let mut self_writes_map = ProductWritesMap::new();
        for combo in Self::build_tape_state_combos(global_overlaps).iter() {
            let group = Self::combo_to_group(combo, 0);
            self_writes_map.insert_neutral_product(&group.to_product())?;
        }
        Ok(self_writes_map)
    }

    /// For each offset, the possible groups of terms (one term per tape)
    /// that could exist at said offset while satisfying
    /// `requirements_by_offset`
    fn build_offset_group_choices(
        overlaps: &TapeOverlaps,
        offsets: &[i64],
        requirements_by_offset: &BTreeMap<i64, MultiTapeStatesMap>,
    ) -> Vec<Vec<OffsetGroupedTerms>> {
        let tape_nos = overlaps.get_all_tape_nos();
        offsets
            .iter()
            .map(|offset| {
                let mut offset_tape_states_whitelist = requirements_by_offset
                    .get(offset)
                    .cloned()
                    .unwrap_or_default();
                for tape_no in tape_nos.iter() {
                    if offset_tape_states_whitelist.contains_tape(*tape_no) {
                        continue;
                    }
                    let multi_tape_states = overlaps.get_states_for_tape(*tape_no);
                    offset_tape_states_whitelist.insert_multi_tape_states(multi_tape_states.iter());
                }
                let states_per_tape = offset_tape_states_whitelist.to_flat_states();
                cartesian_product(&states_per_tape)
                    .iter()
                    .map(|combo| Self::combo_to_group(combo, *offset))
                    .collect()
            })
            .collect()
    }

    /// Port of the python `build_full_group_paths_satisfying`
    pub fn build_full_group_paths_satisfying(
        overlaps: &TapeOverlaps,
        offsets: &[i64],
        requirements_by_offset: &BTreeMap<i64, MultiTapeStatesMap>,
    ) -> Vec<Vec<OffsetGroupedTerms>> {
        let group_choices =
            Self::build_offset_group_choices(overlaps, offsets, requirements_by_offset);
        cartesian_product(&group_choices)
    }

    /// The purpose of constructing extended products for a source_product
    /// is to avoid the situation where two or more (more ambiguously defined)
    /// products simultaneously satisfy some arrangement of states that
    /// appear in the automata tapes and lead to conflicting writes.
    pub fn build_extended_products_for(
        source_product: &MultiTapeProduct,
        overlaps: &TapeOverlaps,
        product_exclusions: &MultiTapeProductTrie,
        prod_priority_map: &IndexMap<MultiTapeProduct, TransitionPriority>,
        product_writes_map: &ProductWritesMap,
    ) -> CompileResult<IndexSet<MultiTapeProduct>> {
        let get_priority = |product: &MultiTapeProduct| -> CompileResult<TransitionPriority> {
            prod_priority_map
                .get(product)
                .copied()
                .ok_or_else(|| CompileTapesError::MissingPriority(product_repr(product)))
        };
        let source_write_priority = get_priority(source_product)?;
        let annotation = source_product.get_annotation();

        let product_path = MultiTapeProductTrie::group_path_from_product(source_product);
        let mut product_exclusions: MultiTapeProductTrie = product_exclusions.clone();
        for offset_group in product_path.iter() {
            product_exclusions = product_exclusions.advance_exclusions(offset_group)?;
        }

        let mut requirements_by_offset: BTreeMap<i64, MultiTapeStatesMap> = BTreeMap::new();
        let mut offsets_set: BTreeSet<i64> = BTreeSet::new();
        for end_product in product_exclusions.end_products.iter() {
            for flat_term in end_product._terms.iter() {
                let offset = flat_term.position;
                offsets_set.insert(offset);
                if end_product == source_product {
                    let (tape_no, cell_state) = flat_term.state;
                    requirements_by_offset.entry(offset).or_default().insert(tape_no, cell_state);
                }
            }
        }

        // all products whose input terms could be satisfied
        // when the source_product's input terms are satisfied too
        let mut writing_products: Vec<MultiTapeProduct> = vec![source_product.clone()];
        writing_products.extend(product_exclusions.search_all_nested_products());

        let mut all_writes: TapeWrites = BTreeMap::new();
        let mut higher_priority_products: IndexSet<MultiTapeProduct> = IndexSet::new();

        for product in writing_products.iter() {
            let writes = product_writes_map.get_state_writes_for(product)?;
            let write_priority = get_priority(product)?;
            for written_state in writes.iter() {
                let tape_no = written_state.tape_no;
                let tape_cell_state = written_state.tape_cell_state;
                if write_priority > source_write_priority {
                    higher_priority_products.insert(product.clone());
                    continue;
                }
                if write_priority < source_write_priority {
                    continue;
                }
                let current_write_state = *all_writes.get(&tape_no).unwrap_or(&tape_cell_state);
                if current_write_state != tape_cell_state {
                    return Err(CompileTapesError::ConflictingWrites(format!(
                        "source_product={} [{}] and product={} [{}] are simultaneously \
                         satisfiable but have conflicting writes on tape {}: {} vs {}",
                        product_repr(source_product), source_product.get_annotation(),
                        product_repr(product), product.get_annotation(),
                        tape_no, current_write_state, tape_cell_state
                    )));
                }
                all_writes.insert(tape_no, tape_cell_state);
            }
        }

        offsets_set.extend(product_exclusions.search_all_offsets());
        let offsets: Vec<i64> = offsets_set.into_iter().collect();
        let group_choices =
            Self::build_offset_group_choices(overlaps, &offsets, &requirements_by_offset);

        let mut extended_products: IndexSet<MultiTapeProduct> = IndexSet::new();
        if group_choices.iter().all(|choices| !choices.is_empty()) {
            // iterate over the cartesian product of group choices
            // (first offset is the most significant)
            let mut choice_indexes: Vec<usize> = vec![0; group_choices.len()];
            'combos: loop {
                let flat_terms: Vec<MultiTapeTerm> = choice_indexes
                    .iter()
                    .enumerate()
                    .flat_map(|(k, index)| group_choices[k][*index].terms.iter().cloned())
                    .collect();

                let is_overridden = higher_priority_products.iter().any(|overriding_product| {
                    terms_satisfiable_with(&flat_terms, &overriding_product._terms)
                });
                if !is_overridden {
                    let extended_product = MultiTapeProduct::new(flat_terms);
                    let new_annotation =
                        format!("{}_({})", annotation, product_repr(&extended_product));
                    extended_products.insert(
                        MultiTapeProductFactory::new(extended_product._terms)
                            .with_annotation(new_annotation)
                            .to_product(),
                    );
                }

                // advance the odometer
                let mut position = choice_indexes.len();
                loop {
                    if position == 0 {
                        break 'combos;
                    }
                    position -= 1;
                    choice_indexes[position] += 1;
                    if choice_indexes[position] < group_choices[position].len() {
                        break;
                    }
                    choice_indexes[position] = 0;
                }
            }
        }

        if extended_products.is_empty() {
            return Err(CompileTapesError::Invalid(format!(
                "no extended products could be built for {} [{}]",
                product_repr(source_product), annotation
            )));
        }
        Ok(extended_products)
    }

    /// We want to remap all combinations of individual tape states
    /// that can overlap over each other directly along the same position
    /// at offset=0 across all tapes to global tape state numbers.
    ///
    /// Returns every such combination (state path), in depth first order
    pub fn build_remap_state_paths(
        tape_nos: &[TapeNo],
        multi_tape_states_map: &MultiTapeStatesMap,
        tape_overlaps: &TapeOverlaps,
    ) -> Vec<StatePath> {
        let mut state_paths: Vec<StatePath> = Vec::new();
        let mut current_path: StatePath = Vec::with_capacity(tape_nos.len());
        Self::_build_remap_state_paths(
            tape_nos, 0, multi_tape_states_map, tape_overlaps,
            &mut current_path, &mut state_paths,
        );
        state_paths
    }

    fn _build_remap_state_paths(
        tape_nos: &[TapeNo],
        tape_no_index: usize,
        multi_tape_states_map: &MultiTapeStatesMap,
        tape_overlaps: &TapeOverlaps,
        overlap_state_path: &mut StatePath,
        state_paths: &mut Vec<StatePath>,
    ) {
        if tape_no_index >= tape_nos.len() {
            state_paths.push(overlap_state_path.clone());
            return;
        }

        let tape_no = tape_nos[tape_no_index];
        // states we are building combinations for in current tape
        let Some(next_tape_cell_states) = multi_tape_states_map.get_ref(tape_no) else {
            return;
        };
        let empty_set = BTreeSet::new();
        let all_tape_states;
        // what other states can overlap directly on top of
        // the last state in the overlap_state_path
        let next_state_overlaps: &BTreeSet<MultiTapeState> = match overlap_state_path.last() {
            // Use all available states for the current tape
            // as the overlap path is empty / just started
            None => {
                all_tape_states = tape_overlaps.get_states_for_tape(tape_no);
                &all_tape_states
            }
            Some(prev_tape_state) => tape_overlaps
                .get_overlaps_for_offset(prev_tape_state, 0)
                .unwrap_or(&empty_set),
        };

        for next_tape_cell_state in next_tape_cell_states.iter() {
            let next_tape_state = MultiTapeState::new(tape_no, *next_tape_cell_state);
            if !next_state_overlaps.contains(&next_tape_state) {
                continue;
            }
            overlap_state_path.push(next_tape_state);
            Self::_build_remap_state_paths(
                tape_nos, tape_no_index + 1, multi_tape_states_map, tape_overlaps,
                overlap_state_path, state_paths,
            );
            overlap_state_path.pop();
        }
    }

    pub fn build_remap_states(
        tape_nos: &[TapeNo],
        multi_tape_states_map: &MultiTapeStatesMap,
        tape_overlaps: &TapeOverlaps,
        remap_counter_start: CellState,
    ) -> MultiTapeStatePathRemap {
        let mut remap = MultiTapeStatePathRemap::new(remap_counter_start);
        for state_path in
            Self::build_remap_state_paths(tape_nos, multi_tape_states_map, tape_overlaps)
        {
            remap.insert_overlap_path(state_path);
        }
        remap
    }

    /// remap individual tape states to a global combined tape state
    pub fn build_global_state_path_remap(
        product_writes_map: &ProductWritesMap, overlaps: &TapeOverlaps,
    ) -> MultiTapeStatePathRemap {
        // contains which tape cell states can exist in each tape
        let mut multi_tape_states_map = MultiTapeStatesMap::new();
        for (product, product_writes) in product_writes_map.iter() {
            // insert product output terms into multi_tape_states_map
            for (tape_no, tape_cell_state) in product_writes.iter() {
                multi_tape_states_map.insert(*tape_no, *tape_cell_state);
            }
            // insert product input terms into multi_tape_states_map
            for product_term in product._terms.iter() {
                let (tape_no, tape_cell_state) = product_term.state;
                multi_tape_states_map.insert(tape_no, tape_cell_state);
            }
        }
        let tape_nos = multi_tape_states_map.get_tape_nos();
        Self::build_remap_states(
            &tape_nos, &multi_tape_states_map, overlaps, REMAP_COUNTER_START,
        )
    }

    /// Reassigns the tape cell states in input_state_path
    /// to the corresponding output tape cell states in product_outputs
    pub fn reassign_state_path(
        input_state_path: &[MultiTapeState], product_outputs: &TapeWrites,
    ) -> StatePath {
        input_state_path
            .iter()
            .map(|state| match product_outputs.get(&state.tape_no) {
                Some(new_tape_cell_state) => MultiTapeState::new(state.tape_no, *new_tape_cell_state),
                None => *state,
            })
            .collect()
    }

    /// For every position that is covered by the current product,
    /// we want to know which states could be present in the product
    /// terms at that position across all individual tapes,
    /// and then determine all fully formed term combinations that
    /// could satisfy the multi_tape_product
    fn build_transitions_for_product(
        ctx: &TransitionsContext,
        source_multi_tape_product: &MultiTapeProduct,
    ) -> CompileResult<(AutomataTransitionsGroup, Vec<MultiTapeProduct>)> {
        let source_annotation = source_multi_tape_product.get_annotation();
        let mut transitions_group = AutomataTransitionsGroup::spawn_new(None);
        // tape writes that the multi_tape_product produces as output
        let product_writes = ctx
            .product_writes_map
            .get(source_multi_tape_product)
            .ok_or_else(|| ProductWritesError::MissingProduct {
                product: product_repr(source_multi_tape_product),
            })?;

        /*
        map position_offset -> tape_no -> choice of possible tape states
        that can exist at the aforementioned (offset, tape_no) such that
        the current product input terms are still satisfied
        */
        let mut offset_tape_states_map: BTreeMap<i64, MultiTapeStatesMap> = BTreeMap::new();
        for product_term in source_multi_tape_product._terms.iter() {
            let (tape_no, cell_state) = product_term.state;
            offset_tape_states_map
                .entry(product_term.position)
                .or_default()
                .insert(tape_no, cell_state);
        }
        for offset_states_whitelist in offset_tape_states_map.values_mut() {
            for (tape_no, tape_states) in ctx.all_tape_states_per_tape.iter() {
                if offset_states_whitelist.contains_tape(*tape_no) {
                    continue;
                }
                /*
                If there aren't any constraints on the tape cell states
                that can exist on a particular tape_no imposed by the
                product terms at the current term_offset, then the set
                of states that can exist on that tape_no at the
                current term_offset is just the set of all tape cell
                states that can exist on that tape in general
                */
                offset_states_whitelist.set(*tape_no, tape_states.clone());
            }
        }

        if !offset_tape_states_map.contains_key(&0) {
            return Err(CompileTapesError::Invalid(format!(
                "product {} has no terms at the output position",
                product_repr(source_multi_tape_product)
            )));
        }

        /*
        Each list item contains the set of possible remapped terms
        that the corresponding term offset could contain
        */
        let mut product_pos_combos: Vec<Vec<Term>> = Vec::new();
        for (product_term_position, offset_states_whitelist) in offset_tape_states_map.iter() {
            let position_combos: BTreeSet<StatePath> = Self::build_remap_state_paths(
                &ctx.all_tape_nos, offset_states_whitelist, ctx.global_overlaps,
            )
            .into_iter()
            .collect();

            let mut position_remapped_terms: Vec<Term> = Vec::new();
            for state_path in position_combos.iter() {
                let remapped_cell_state = ctx.global_state_path_remap.remap(state_path)?;
                let remapped_term = Term::new(*product_term_position, remapped_cell_state, false);
                if position_remapped_terms.contains(&remapped_term) {
                    continue;
                }
                position_remapped_terms.push(remapped_term);
            }
            product_pos_combos.push(position_remapped_terms);
        }

        /*
        Get every specific combination of term states that could satisfy
        the current product's input terms
        */
        let specific_combos = cartesian_product(&product_pos_combos);
        let mut unrepresentable_products: Vec<MultiTapeProduct> = Vec::new();

        for remapped_product_input_terms in specific_combos.into_iter() {
            let mut current_product_writes: TapeWrites = product_writes.clone();
            let multi_tape_prod = ctx
                .global_state_path_remap
                .remap_single_tape_terms(&remapped_product_input_terms, source_annotation)?;

            if !ctx.representability.is_representable(&multi_tape_prod) {
                unrepresentable_products.push(multi_tape_prod);
                continue;
            }

            let matching_products = ctx.preexisting_products.load_matching_products(&multi_tape_prod);
            for matching_product in matching_products.iter() {
                let Some(matching_product_writes) = ctx.product_writes_map.get(matching_product)
                else {
                    continue;
                };
                for (write_tape_no, write_tape_cell_state) in matching_product_writes.iter() {
                    let existing_write_cell_state = *current_product_writes
                        .get(write_tape_no)
                        .unwrap_or(write_tape_cell_state);
                    if existing_write_cell_state != *write_tape_cell_state {
                        return Err(CompileTapesError::ConflictingWrites(format!(
                            "Conflicting writes for tape {}: {} vs {} in product {} vs {}",
                            write_tape_no, existing_write_cell_state, write_tape_cell_state,
                            product_repr(&multi_tape_prod), product_repr(matching_product)
                        )));
                    }
                    current_product_writes.insert(*write_tape_no, *write_tape_cell_state);
                }
            }

            let input_terms_at_output_pos: Vec<&Term> = remapped_product_input_terms
                .iter()
                .filter(|term| term.position == 0)
                .collect();
            if input_terms_at_output_pos.len() != 1 {
                return Err(CompileTapesError::Invalid(format!(
                    "There should only be one term at output position within {}",
                    terms_repr(&remapped_product_input_terms)
                )));
            }
            let input_tape_cell_state_at_output_pos = input_terms_at_output_pos[0].state;

            let remapped_output_state = match ctx
                .global_state_path_remap
                .rev_lookup(input_tape_cell_state_at_output_pos)?
            {
                None => HALT_STATE,
                Some(input_state_path) => {
                    let output_state_path =
                        Self::reassign_state_path(input_state_path, &current_product_writes);
                    ctx.global_state_path_remap.remap(&output_state_path)?
                }
            };

            let mut annotation = multi_tape_prod.get_annotation().clone();
            if annotation.is_empty() {
                annotation = product_repr(&multi_tape_prod);
            }
            transitions_group
                .add_transition(remapped_product_input_terms, remapped_output_state, true, annotation)
                .map_err(CompileTapesError::Transition)?;
        }

        Ok((transitions_group, unrepresentable_products))
    }

    /// Combine a multi-tape automata into a single tape automata
    pub fn compile_tapes(&self, verbose: bool) -> CompileResult<CompileTapesResult> {
        let tape_overlaps_fsm = self.build_overlaps_fsm(verbose)?;
        let mut global_overlaps = tape_overlaps_fsm.merge();
        // get all tape states that can exist in each tape
        let all_tape_states_per_tape = global_overlaps.create_whitelist_for_offset(0);

        let prod_priority_map = &self.prod_priority_map;
        let preexisting_writes_map = &self.prod_to_state_map;
        let mut preexisting_products_trie = MultiTapeProductTrie::spawn_root();
        for product in preexisting_writes_map.products() {
            preexisting_products_trie.insert_product(product);
        }

        let preexisting_products: Vec<&MultiTapeProduct> =
            preexisting_writes_map.products().collect();
        let all_product_extensions: Vec<CompileResult<IndexSet<MultiTapeProduct>>> =
            preexisting_products
                .par_iter()
                .map(|product| {
                    Self::build_extended_products_for(
                        product,
                        &global_overlaps,
                        &preexisting_products_trie,
                        prod_priority_map,
                        preexisting_writes_map,
                    )
                })
                .collect();

        let mut extended_preexisting_products = ProductWritesMap::new();
        let mut extended_preexisting_products_trie = MultiTapeProductTrie::spawn_root();
        for (product, product_extensions) in
            preexisting_products.iter().zip(all_product_extensions.into_iter())
        {
            let product_extensions = product_extensions?;
            if verbose {
                println!(
                    "\nBuilding extensions for: {} {} -> {}",
                    product_repr(product), product.get_annotation(),
                    preexisting_writes_map.get_stub_writes_str_for(product)
                );
                for (k, extended_product) in product_extensions.iter().enumerate() {
                    println!("[{}] -> {}", k, product_repr(extended_product));
                    println!("{}", extended_product.get_annotation());
                }
            }

            let writes = preexisting_writes_map.get_multi_tape_writes_for(product);
            for extended_product in product_extensions.iter() {
                for write_multi_tape_state in writes.iter() {
                    extended_preexisting_products
                        .insert(extended_product.clone(), *write_multi_tape_state)?;
                    extended_preexisting_products_trie.insert_product(extended_product);
                }
            }
        }

        /*
        Generate rules for all possible term combinations
        that could exist given the state overlaps passed in,
        excluding pre-existing products as they already have explicit
        output write rule(s).
        */
        let self_writes_map = Self::build_self_writes_map(&global_overlaps)?;
        let covering_product_writes_map = Self::build_product_same_writes_map(
            &global_overlaps, &extended_preexisting_products_trie,
        )?;

        let mut product_writes_map = ProductWritesMap::new();
        product_writes_map.merge(&extended_preexisting_products)?;
        product_writes_map.merge(&covering_product_writes_map)?;
        product_writes_map.freeze();

        // remap individual tape states to a global combined tape state
        let global_state_path_remap =
            Self::build_global_state_path_remap(&self_writes_map, &global_overlaps);

        let mut all_tape_nos = self.tape_nos.clone();
        all_tape_nos.sort();
        let ctx = TransitionsContext {
            all_tape_nos,
            product_writes_map: &product_writes_map,
            all_tape_states_per_tape: &all_tape_states_per_tape,
            global_overlaps: &global_overlaps,
            global_state_path_remap: &global_state_path_remap,
            preexisting_products: &extended_preexisting_products_trie,
            representability: RepresentabilityIndex::new(&tape_overlaps_fsm),
        };

        let products: Vec<&MultiTapeProduct> = product_writes_map.products().collect();
        let build_results: Vec<CompileResult<(AutomataTransitionsGroup, Vec<MultiTapeProduct>)>> =
            products
                .par_iter()
                .map(|product| Self::build_transitions_for_product(&ctx, product))
                .collect();

        // input-output pairs for the final combined automata
        let mut global_transitions_group = AutomataTransitionsGroup::spawn_new(None);
        let mut all_unrepresentable_products: HashSet<MultiTapeProduct> = HashSet::new();
        for build_result in build_results.into_iter() {
            let (product_transitions_group, unrepresentable_products) = build_result?;
            all_unrepresentable_products.extend(unrepresentable_products);
            global_transitions_group
                .merge(product_transitions_group)
                .map_err(CompileTapesError::Transition)?;
        }

        let num_unrepresentable_products = all_unrepresentable_products.len();
        if verbose {
            println!("len(all_unrepresentable_products)={}", num_unrepresentable_products);
        }

        Ok(CompileTapesResult {
            transitions_group: global_transitions_group,
            state_remap: global_state_path_remap,
            num_unrepresentable_products,
        })
    }
}
