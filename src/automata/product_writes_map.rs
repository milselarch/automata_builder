use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use indexmap::IndexMap;
use indexmap::IndexSet;
use crate::automata::rule_generator::VOID_STATE;
use crate::automata::tape_overlaps::{TapeOverlaps};
use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{
    AbstractMultiTapeExpression, MultiTapeProduct,
    MultiTapeState, MultiTapeTerm, TapeNo
};

/// Writes performed by a single product: `tape_no -> output tape cell state`.
/// `BTreeMap` so iteration is deterministic (ascending `TapeNo`).
pub type TapeWrites = BTreeMap<TapeNo, CellState>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ProductWritesError {
    /// The same product wants to write two different states to the same tape.
    ConflictingOutput {
        product: String,
        tape_no: TapeNo,
        existing: CellState,
        incoming: CellState,
        annotations: Vec<String>,
    },
    /// Mutating a frozen map.
    Frozen,
    /// Lookup of a product that is not present in the map.
    MissingProduct { product: String },
}

impl fmt::Display for ProductWritesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProductWritesError::ConflictingOutput {
                product, tape_no, existing,
                incoming, annotations
            } => write!(
                f,
                "Conflicting output states for product={} on tape {}: {} vs {} \
                - Annotations: {:?}",
                product, tape_no, existing, incoming, annotations
            ),
            ProductWritesError::Frozen => {
                write!(f, "Cannot modify ProductWritesMap when frozen")
            }
            ProductWritesError::MissingProduct { product } => {
                write!(f, "Product {} is not in the ProductWritesMap", product)
            }
        }
    }
}
impl std::error::Error for ProductWritesError {}

/// - `writable`: there are products that can produce this state
/// - `deletable`: there are products that can transition away from it
/// - `instant_delete`: every instance is immediately deleted, i.e. there is
///   a rule `state -> other_state` with no path back to `state`
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MultiTapeStateAttributes {
    pub writable: bool,
    pub deletable: bool,
    pub instant_delete: bool,
}

pub type StateAttributesMap = BTreeMap<MultiTapeState, MultiTapeStateAttributes>;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProductToStateMap {
    prod_to_state_map: IndexMap<MultiTapeProduct, TapeWrites>,
    // self-reference for products, so that we can look up
    // product annotations for pre-existing products
    products: HashMap<MultiTapeProduct, MultiTapeProduct>,
}

impl ProductToStateMap {
    pub fn new() -> Self {
        Self {
            prod_to_state_map: IndexMap::new(),
            products: HashMap::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.prod_to_state_map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.prod_to_state_map.is_empty()
    }
    pub fn iter(&self) -> indexmap::map::Iter<'_, MultiTapeProduct, TapeWrites> {
        self.prod_to_state_map.iter()
    }
    pub fn get(&self, product: &MultiTapeProduct) -> Option<&TapeWrites> {
        self.prod_to_state_map.get(product)
    }
    pub fn contains_key(&self, product: &MultiTapeProduct) -> bool {
        self.prod_to_state_map.contains_key(product)
    }
    pub fn shift_remove(&mut self, product: &MultiTapeProduct) -> Option<TapeWrites> {
        self.prod_to_state_map.shift_remove(product)
    }
    pub fn keys(&self) -> indexmap::map::Keys<'_, MultiTapeProduct, TapeWrites> {
        self.prod_to_state_map.keys()
    }
    pub fn insert(
        &mut self,
        product: MultiTapeProduct,
        write_tape_no: TapeNo,
        write_tape_cell_state: CellState,
    ) -> Result<(), ProductWritesError> {
        let product_repr = product._to_string("D");
        let writes_map = self.prod_to_state_map.entry(product.clone()).or_default();
        let existing = writes_map
            .get(&write_tape_no)
            .copied()
            .unwrap_or(write_tape_cell_state);

        if existing != write_tape_cell_state {
            let existing_product = &self.products[&product];
            let annotations = vec![
                existing_product.get_annotation().clone(),
                product.get_annotation().clone()
            ];
            return Err(ProductWritesError::ConflictingOutput {
                product: product_repr,
                tape_no: write_tape_no,
                existing,
                incoming: write_tape_cell_state,
                annotations
            });
        }

        writes_map.insert(write_tape_no, write_tape_cell_state);
        self.products.insert(product.clone(), product);
        Ok(())
    }
}


/// map `product -> tape_no -> output tape cell state`
///
/// `IndexMap` preserves insertion order, mirroring the iteration order of the
/// Python `dict`-backed implementation.
#[derive(Debug, Clone, Default)]
pub struct ProductWritesMap {
    prod_to_state_map: ProductToStateMap,
    frozen: bool,
}

/// Equality only compares the `product -> writes` mappings
/// (insertion order and frozen-ness are ignored), mirroring the python
/// `encode` based equality.
impl PartialEq for ProductWritesMap {
    fn eq(&self, other: &Self) -> bool {
        self.prod_to_state_map == other.prod_to_state_map
    }
}
impl Eq for ProductWritesMap {}

impl ProductWritesMap {
    pub fn new() -> Self {
        Self { prod_to_state_map: ProductToStateMap::new(), frozen: false }
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn to_unfrozen(&self) -> Self {
        Self { prod_to_state_map: self.prod_to_state_map.clone(), frozen: false }
    }

    pub fn len(&self) -> usize {
        self.prod_to_state_map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.prod_to_state_map.is_empty()
    }

    pub fn contains_product(&self, product: &MultiTapeProduct) -> bool {
        self.prod_to_state_map.contains_key(product)
    }

    /// Python `__getitem__` (returns a borrow instead of a shallow copy).
    pub fn get(&self, product: &MultiTapeProduct) -> Option<&TapeWrites> {
        self.prod_to_state_map.get(product)
    }

    /// Python `__iter__` over products.
    pub fn products(&self) -> impl Iterator<Item = &MultiTapeProduct> {
        self.prod_to_state_map.keys()
    }

    /// Python `items()`.
    pub fn iter(&self) -> indexmap::map::Iter<'_, MultiTapeProduct, TapeWrites> {
        self.prod_to_state_map.iter()
    }

    /// Python `__delitem__`.
    pub fn remove(
        &mut self, product: &MultiTapeProduct,
    ) -> Result<Option<TapeWrites>, ProductWritesError> {
        if self.frozen {
            return Err(ProductWritesError::Frozen);
        }
        Ok(self.prod_to_state_map.shift_remove(product))
    }

    pub fn insert(
        &mut self, product: MultiTapeProduct, tape_output: MultiTapeState,
    ) -> Result<(), ProductWritesError> {
        self._insert(product, tape_output.tape_no, tape_output.tape_cell_state)
    }

    fn _insert(
        &mut self,
        product: MultiTapeProduct,
        write_tape_no: TapeNo,
        write_tape_cell_state: CellState,
    ) -> Result<(), ProductWritesError> {
        if self.frozen {
            return Err(ProductWritesError::Frozen);
        }
        self.prod_to_state_map.insert(
            product,
            write_tape_no,
            write_tape_cell_state,
        )
    }

    /// Insert a product whose outputs rewrite the input terms with offset 0
    /// so that they keep the same state.
    pub fn insert_neutral_product(
        &mut self, product: &MultiTapeProduct,
    ) -> Result<(), ProductWritesError> {
        for term in product.to_flat_terms() {
            if term.position != 0 {
                continue;
            }
            let (tape_no, tape_cell_state) = term.state;
            self._insert(product.copy(), tape_no, tape_cell_state)?;
        }
        Ok(())
    }

    pub fn merge(&mut self, other: &ProductWritesMap) -> Result<(), ProductWritesError> {
        for (product, writes) in other.iter() {
            for (tape_no, tape_cell_state) in writes.iter() {
                self._insert(product.copy(), *tape_no, *tape_cell_state)?;
            }
        }
        Ok(())
    }

    pub fn get_state_writes_for(
        &self, product: &MultiTapeProduct,
    ) -> Result<Vec<MultiTapeState>, ProductWritesError> {
        let writes_map = self.get(product).ok_or_else(|| {
            ProductWritesError::MissingProduct { product: product._to_string("D") }
        })?;

        Ok(writes_map
            .iter()
            .map(|(tape_no, cell_state)| MultiTapeState::new(*tape_no, *cell_state))
            .collect())
    }

    /// Python `get_multi_tape_writes_for` (empty if product is missing).
    pub fn get_multi_tape_writes_for(
        &self, product: &MultiTapeProduct,
    ) -> BTreeSet<MultiTapeState> {
        self.get(product)
            .map(|writes| {
                writes
                    .iter()
                    .map(|(tape_no, cell_state)| MultiTapeState::new(*tape_no, *cell_state))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Python `get_stub_writes_str_for`, e.g. `T0:3T1:2`
    pub fn get_stub_writes_str_for(&self, product: &MultiTapeProduct) -> String {
        self.get(product)
            .map(|writes| {
                writes
                    .iter()
                    .map(|(tape_no, cell_state)| format!("T{}:{}", tape_no, cell_state))
                    .collect::<String>()
            })
            .unwrap_or_default()
    }

    /// All (input) states referenced by the products in this map.
    pub fn get_states_set(&self) -> IndexSet<MultiTapeState> {
        let mut states_set: IndexSet<MultiTapeState> = IndexSet::new();

        for product in self.products() {
            for term in product.to_flat_terms() {
                let (tape_no, tape_cell_state) = term.state;
                states_set.insert(MultiTapeState::new(tape_no, tape_cell_state));
            }
        }
        states_set
    }

    /// maps state -> products that produce it in their output writes.
    pub fn build_state_to_products_map(
        &self,
    ) -> BTreeMap<MultiTapeState, Vec<MultiTapeProduct>> {
        let mut state_to_products: BTreeMap<MultiTapeState, Vec<MultiTapeProduct>> =
            BTreeMap::new();

        for (product, writes) in self.iter() {
            for (tape_no, tape_cell_state) in writes.iter() {
                let state = MultiTapeState::new(*tape_no, *tape_cell_state);
                state_to_products.entry(state).or_default().push(product.copy());
            }
        }
        state_to_products
    }

    /// maps state -> products that contain it in their input terms.
    pub fn build_input_state_to_prod_map(
        &self,
    ) -> BTreeMap<MultiTapeState, Vec<MultiTapeProduct>> {
        let mut input_state_to_prod: BTreeMap<MultiTapeState, Vec<MultiTapeProduct>> =
            BTreeMap::new();

        for product in self.products() {
            for term in product.to_flat_terms() {
                let (tape_no, tape_cell_state) = term.state;
                let state = MultiTapeState::new(tape_no, tape_cell_state);
                input_state_to_prod.entry(state).or_default().push(product.copy());
            }
        }
        input_state_to_prod
    }
}

impl ProductWritesMap {
    /// Python `from_pairs`.
    pub fn from_pairs(
        pairs: Vec<(MultiTapeProduct, TapeWrites)>,
    ) -> Result<Self, ProductWritesError> {
        let mut map = ProductWritesMap::new();
        for (product, writes) in pairs {
            for (tape_no, tape_cell_state) in writes.iter() {
                map._insert(product.copy(), *tape_no, *tape_cell_state)?;
            }
        }
        Ok(map)
    }

    /// All products with the same shape as `target_product` but whose input
    /// term offsets are shifted by a single constant offset.
    ///
    /// e.g. `D(0,1,1)*D(1,2,1)` and `D(1,1,1)*D(2,2,1)` are translational
    /// variants (the second is shifted by `+1`).
    ///
    /// Returns `(variant, offset_diff)` pairs.
    pub fn get_translated_variants(
        &self,
        target_product: &MultiTapeProduct,
        offset_whitelist: Option<&BTreeSet<i64>>,
    ) -> Result<IndexSet<(MultiTapeProduct, i64)>, ProductWritesError> {
        if !self.contains_product(target_product) {
            return Err(ProductWritesError::MissingProduct {
                product: target_product._to_string("D"),
            });
        }

        let mut source_terms = target_product.to_flat_terms();
        source_terms.sort();

        let mut variants: IndexSet<(MultiTapeProduct, i64)> = IndexSet::new();

        for candidate in self.products() {
            let mut candidate_terms = candidate.to_flat_terms();
            if candidate_terms.len() != source_terms.len() {
                continue;
            }
            candidate_terms.sort();

            let mut offset_diffs: BTreeSet<i64> = BTreeSet::new();
            for (source_term, candidate_term) in
                source_terms.iter().zip(candidate_terms.iter())
            {
                offset_diffs.insert(candidate_term.position - source_term.position);
            }

            // a single shared delta across every term == a pure translation
            if offset_diffs.len() != 1 {
                continue;
            }
            let offset_diff = *offset_diffs.iter().next().unwrap();

            if let Some(whitelist) = offset_whitelist {
                if !whitelist.is_empty() && !whitelist.contains(&offset_diff) {
                    continue;
                }
            }

            variants.insert((candidate.copy(), offset_diff));
        }

        Ok(variants)
    }

    /// Remove every product that contains `state` among its input terms.
    pub fn extinct_input_state(
        &mut self, state: MultiTapeState,
    ) -> Result<usize, ProductWritesError> {
        if self.frozen {
            return Err(ProductWritesError::Frozen);
        }

        let doomed: Vec<MultiTapeProduct> = self
            .products()
            .filter(|product| {
                product.to_flat_terms().iter().any(|term| {
                    let (tape_no, tape_cell_state) = term.state;
                    MultiTapeState::new(tape_no, tape_cell_state) == state
                })
            })
            .map(|product| product.copy())
            .collect();

        let num_removed = doomed.len();
        for product in doomed {
            self.prod_to_state_map.shift_remove(&product);
        }
        Ok(num_removed)
    }

    /// Remove all input products that become permanently unsatisfiable,
    /// along with their translational variants.
    pub fn purge_unsatisfiable_products(
        &mut self, state_attributes_map: &StateAttributesMap,
    ) -> Result<IndexSet<MultiTapeProduct>, ProductWritesError> {
        if self.frozen {
            return Err(ProductWritesError::Frozen);
        }

        let all_products: Vec<MultiTapeProduct> =
            self.products().map(|product| product.copy()).collect();
        let mut products_to_delete: IndexSet<MultiTapeProduct> = IndexSet::new();

        for product in all_products.iter() {
            let becomes_unsatisfiable =
                self.does_product_become_unsatisfiable(product, state_attributes_map)?;
            if !becomes_unsatisfiable {
                continue;
            }

            products_to_delete.insert(product.copy());
            for (variant, _) in self.get_translated_variants(product, None)? {
                products_to_delete.insert(variant);
            }
        }

        for product in products_to_delete.iter() {
            self.prod_to_state_map.shift_remove(product);
        }

        Ok(products_to_delete)
    }

    /// Whether `product` is guaranteed to never be satisfiable again.
    ///
    /// This holds when:
    /// 1. every input term outside the write position is unwritable, and
    /// 2. some input at offset 0 is unwritable, and
    /// 3. that input is replaced by an undeletable state, and
    /// 4. the replacement differs from the input state.
    ///
    /// (1) and (2) mean no new instances can form elsewhere; (3) and (4) mean
    /// every current match transitions away permanently.
    pub fn does_product_become_unsatisfiable(
        &self,
        product: &MultiTapeProduct,
        state_attributes_map: &StateAttributesMap,
    ) -> Result<bool, ProductWritesError> {
        let product_writes = self.get(product).ok_or_else(|| {
            ProductWritesError::MissingProduct { product: product._to_string("D") }
        })?;

        let mut has_transition_to_unsatisfiability = false;
        let mut has_input_terms_along_output_offset = false;

        for input_term in product.to_flat_terms() {
            let (input_tape_no, input_tape_cell_state) = input_term.state;
            let input_state = MultiTapeState::new(input_tape_no, input_tape_cell_state);

            let input_attrs = match state_attributes_map.get(&input_state) {
                Some(attrs) => *attrs,
                None => continue,
            };

            if input_term.position != 0 {
                if input_attrs.writable {
                    /*
                    An input term off the output position can be spawned from
                    elsewhere, so the product may become satisfiable again.
                    */
                    return Ok(false);
                }
                continue;
            }

            has_input_terms_along_output_offset = true;

            // output does not touch this term's tape -> nothing to check
            let output_tape_cell_state = match product_writes.get(&input_tape_no) {
                Some(state) => *state,
                None => continue,
            };
            let output_state = MultiTapeState::new(input_tape_no, output_tape_cell_state);

            let output_attrs = match state_attributes_map.get(&output_state) {
                Some(attrs) => *attrs,
                None => continue,
            };

            has_transition_to_unsatisfiability |= !input_attrs.writable
                && !output_attrs.deletable
                && input_state != output_state;
        }

        if !has_input_terms_along_output_offset {
            /*
            Every input term is offset away from the output and none of them
            can be spawned moving forward.
            */
            return Ok(true);
        }

        Ok(has_transition_to_unsatisfiability)
    }

    /// Python `build_input_products`.
    pub fn build_input_products(&self) -> IndexSet<MultiTapeProduct> {
        self.products().map(|product| product.copy()).collect()
    }

    /// Python `build_all_state_attrs_map`.
    pub fn build_all_state_attrs_map(
        &self,
        extant_states: Option<&BTreeSet<MultiTapeState>>,
        tape_overlaps: &TapeOverlaps,
    ) -> StateAttributesMap {
        let mut state_attributes_map: StateAttributesMap = BTreeMap::new();

        for state in self.get_states_set() {
            let attributes =
                self.get_state_attributes(state, extant_states, tape_overlaps);
            state_attributes_map.insert(state, attributes);
        }
        state_attributes_map
    }

    /// `extant_states == None` yields attributes valid across all future
    /// timesteps; otherwise they are valid only for the timestep in which
    /// exactly those states exist.
    pub fn get_state_attributes(
        &self,
        source_state: MultiTapeState,
        extant_states: Option<&BTreeSet<MultiTapeState>>,
        tape_overlaps: &TapeOverlaps,
    ) -> MultiTapeStateAttributes {
        let source_tape_cell_state = source_state.tape_cell_state;
        let transitioned_states =
            self.get_same_tape_states_transitioned_to(source_state, tape_overlaps);

        let mut writable = false;
        let mut deletable = false;

        for (product, writes_map) in self.iter() {
            if let Some(extant) = extant_states {
                // a product needing a non-existent input state is unsatisfiable
                let is_unsatisfiable = product.to_flat_terms().iter().any(|term| {
                    let (tape_no, tape_cell_state) = term.state;
                    !extant.contains(&MultiTapeState::new(tape_no, tape_cell_state))
                });
                if is_unsatisfiable {
                    continue;
                }
            }

            let mut source_state_written = false;
            for (tape_no, tape_cell_state) in writes_map.iter() {
                if MultiTapeState::new(*tape_no, *tape_cell_state) == source_state {
                    source_state_written = true;
                }
            }

            // product writes to source_state's tape, but a different state
            let mut writes_away_from_source_state = false;
            // product maps source_state back onto itself
            let mut is_idempotent_transition = false;

            for input_term in product.to_flat_terms() {
                if input_term.position != 0 {
                    continue;
                }
                let (input_tape_no, input_tape_cell_state) = input_term.state;
                if input_tape_no != source_state.tape_no {
                    continue;
                }
                if input_tape_cell_state != source_state.tape_cell_state {
                    continue;
                }

                let output_tape_cell_state = writes_map
                    .get(&source_state.tape_no)
                    .copied()
                    .unwrap_or(source_state.tape_cell_state);

                if source_state.tape_cell_state != output_tape_cell_state {
                    writes_away_from_source_state = true;
                } else {
                    is_idempotent_transition = true;
                }
            }

            debug_assert!(
                product._get_num_terms() != 1
                    || product.to_flat_terms()[0].state.1 != VOID_STATE,
                "VOID STATE CANNOT AUTO TRANSITION AWAY - {}",
                product._to_string("D")
            );

            writable |= source_state_written && !is_idempotent_transition;
            deletable |= writes_away_from_source_state;
        }

        let instant_delete = !transitioned_states.contains(&source_tape_cell_state)
            && source_tape_cell_state != VOID_STATE
            && !writable;

        MultiTapeStateAttributes { writable, deletable, instant_delete }
    }

    /// For each offset, the cartesian product of the states reachable on each
    /// tape at that offset, as concrete terms.
    pub fn build_flat_offset_path_combos(
        source_state: MultiTapeState, tape_overlaps: &TapeOverlaps,
    ) -> Vec<Vec<MultiTapeTerm>> {
        let offset_overlaps = match tape_overlaps.get(&source_state) {
            Some(overlaps) => overlaps,
            None => return vec![],
        };

        let mut combos: Vec<Vec<MultiTapeTerm>> = Vec::new();

        for (offset, target_states) in offset_overlaps.iter() {
            let mut states_by_tape: BTreeMap<TapeNo, BTreeSet<CellState>> =
                BTreeMap::new();
            for target_state in target_states.iter() {
                states_by_tape
                    .entry(target_state.tape_no)
                    .or_default()
                    .insert(target_state.tape_cell_state);
            }

            // iterative cartesian product across tapes
            let mut offset_combos: Vec<Vec<MultiTapeTerm>> = vec![vec![]];
            for (tape_no, cell_states) in states_by_tape.iter() {
                let mut next_combos: Vec<Vec<MultiTapeTerm>> =
                    Vec::with_capacity(offset_combos.len() * cell_states.len());

                for combo in offset_combos.iter() {
                    for cell_state in cell_states.iter() {
                        let mut extended = combo.clone();
                        extended.push(MultiTapeTerm::new(
                            *offset,
                            (*tape_no, *cell_state),
                            false,
                        ));
                        next_combos.push(extended);
                    }
                }
                offset_combos = next_combos;
            }

            combos.extend(offset_combos);
        }

        combos
    }

    /// Every state `source_state` could hold at the same position after one
    /// timestep.
    pub fn get_same_tape_states_transitioned_to(
        &self, source_state: MultiTapeState, tape_overlaps: &TapeOverlaps,
    ) -> BTreeSet<CellState> {
        let source_tape_no = source_state.tape_no;
        let source_tape_cell_state = source_state.tape_cell_state;

        if !tape_overlaps.contains(&source_state) {
            return BTreeSet::from([source_tape_cell_state]);
        }

        let source_term =
            MultiTapeTerm::new(0, (source_tape_no, source_tape_cell_state), false);
        let mut transitioned_states: BTreeSet<CellState> = BTreeSet::new();

        for (product, writes_map) in self.iter() {
            let input_terms = product.to_flat_terms();

            let product_is_satisfiable = input_terms.iter().all(|term| {
                let (tape_no, tape_cell_state) = term.state;
                tape_overlaps.contains(&MultiTapeState::new(tape_no, tape_cell_state))
            });
            if !product_is_satisfiable {
                continue;
            }
            if !input_terms.contains(&source_term) {
                continue;
            }

            if let Some(write_state) = writes_map.get(&source_tape_no) {
                transitioned_states.insert(*write_state);
            }
        }

        /*
        If no input product applies to some combination of states that can
        actually co-occur, then source_state may persist unchanged.
        */
        let mut has_path_transition_to_same_state = false;
        let offset_path_combos =
            Self::build_flat_offset_path_combos(source_state, tape_overlaps);

        'combos: for combo in offset_path_combos.iter() {
            for (product, writes_map) in self.iter() {
                let input_terms = product.to_flat_terms();
                if !input_terms.contains(&source_term) {
                    continue;
                }

                let input_tapes: BTreeSet<TapeNo> =
                    input_terms.iter().map(|term| term.state.0).collect();

                let written_state = writes_map
                    .get(&source_tape_no)
                    .copied()
                    .unwrap_or(source_tape_cell_state);
                if written_state != source_tape_cell_state {
                    continue;
                }

                let mut product_covers_combo = true;
                for term in combo.iter() {
                    /*
                    A tape the product never reads cannot invalidate the match.
                    */
                    if !input_tapes.contains(&term.state.0) {
                        continue;
                    }
                    if !input_terms.contains(term) {
                        product_covers_combo = false;
                        break;
                    }
                }

                if product_covers_combo {
                    has_path_transition_to_same_state = true;
                    break 'combos;
                }
            }
        }

        if has_path_transition_to_same_state || transitioned_states.is_empty() {
            transitioned_states.insert(source_tape_cell_state);
        }

        transitioned_states
    }
}

impl<'a> IntoIterator for &'a ProductWritesMap {
    type Item = (&'a MultiTapeProduct, &'a TapeWrites);
    type IntoIter = indexmap::map::Iter<'a, MultiTapeProduct, TapeWrites>;

    fn into_iter(self) -> Self::IntoIter {
        self.prod_to_state_map.iter()
    }
}
