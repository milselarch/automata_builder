use rayon::iter::ParallelIterator;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::{BitOr, Mul};
use indexmap::IndexSet;
use rayon::iter::IntoParallelRefIterator;
use crate::automata::terms::{clip_after_space, CellState, ExprDebugInfo, ExprPosition};

pub type TapeNo = u16;
pub type MultiTapeCellState = (TapeNo, CellState);

pub trait AbstractMultiTapeExpression: Mul + BitOr + Eq + Sized {
    fn copy(&self) -> Self;
    fn _sub(&self, substitutions: &HashMap<i64, MultiTapeCellState>, default: MultiTapeCellState) -> bool;
    fn offset(&self, offset: i64) -> Self;
    fn _expand(
        &self, expansion_mapping: &HashMap<MultiTapeCellState, MultiTapeExpression>,
        include_debug_info: bool, fold: bool
    ) -> MultiTapeExpression;
    fn _expand_steps(
        &self, expansion_mapping: &HashMap<MultiTapeCellState, MultiTapeExpression>,
        steps: u64, include_debug_info: bool, fold: bool
    ) -> MultiTapeExpression;
    fn to_expression(&self) -> MultiTapeExpression;
    fn _to_string(&self, name: &str) -> String;
    fn _get_num_terms(&self) -> usize;
    fn _assign_indexes_as_base(&mut self);
}

#[derive(Clone, Debug, Eq)]
pub struct MultiTapeTerm {
    // position within the cellular automata tape
    pub (crate) position: i64,
    pub state: MultiTapeCellState,
    // TODO: implement optimization
    pub (crate) _optimized: bool,
    // position of the term within an expression
    pub (crate) _debug_info: ExprDebugInfo,
}

impl PartialEq<MultiTapeTerm> for MultiTapeTerm {
    fn eq(&self, other: &Self) -> bool {
        self.position == other.position && self.state == other.state
    }
}
impl PartialOrd<MultiTapeTerm> for MultiTapeTerm {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for MultiTapeTerm {
    fn cmp(&self, other: &Self) -> Ordering {
        self.position.cmp(&other.position)
            .then_with(|| self.state.cmp(&other.state))
    }
}

impl Hash for MultiTapeTerm {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.position.hash(state);
        self.state.hash(state);
    }
}
impl MultiTapeTerm {
    pub fn new(position: i64, state: MultiTapeCellState, optimized: bool) -> MultiTapeTerm {
        MultiTapeTerm {
            position, state, _optimized: optimized,
            _debug_info: ExprDebugInfo::spawn_empty(),
        }
    }

    pub fn has_debug_position_info(&self) -> bool {
        self._debug_info.position_info.is_some()
    }

    pub fn _assign_expr_position(
        &mut self, product_idx: u64, term_idx: u64,
    ) {
        self._debug_info.position_info = Some(ExprPosition {
            product_idx, term_idx
        });
    }
    pub fn _assign_expr_position_if_empty(
        &mut self, product_idx: u64, term_idx: u64
    ) {
        if self._debug_info.position_info.is_none() {
            self._assign_expr_position(
                product_idx, term_idx
            );
        }
    }
    pub fn insert_expr_position(
        &mut self, position: ExprPosition,
        require_empty: bool
    ) {
        if require_empty {
            assert!(
                self._debug_info.position_info.is_none(),
                "Term already has position info: {:?}",
                self._debug_info.position_info
            );
        }
        self._debug_info.position_info = Some(position);
    }

    pub fn _to_product(&self) -> MultiTapeProduct {
        MultiTapeProduct::new(vec![self.copy()])
    }
    pub fn _get_name() -> String {
        const TERM_EXAMPLE: MultiTapeTerm = MultiTapeTerm {
            position: 0, state: (0, 0), _optimized: false,
            _debug_info: ExprDebugInfo {
                expansion_index: 0,
                position_info: None,
                parent_position_info: None
            }
        };
        clip_after_space(format!("{:?}", TERM_EXAMPLE))
    }
}

impl Mul for MultiTapeTerm {
    type Output = MultiTapeProduct;

    fn mul(self, rhs: Self) -> Self::Output {
        MultiTapeProduct::new(vec![self, rhs])
    }
}
impl Mul<MultiTapeProduct> for MultiTapeTerm {
    type Output = MultiTapeProduct;

    fn mul(self, rhs: MultiTapeProduct) -> MultiTapeProduct {
        let mut new_terms: Vec<MultiTapeTerm> = Vec::new();
        new_terms.push(self.clone());
        for term in rhs._terms.iter() {
            new_terms.push(term.copy());
        }
        // TODO: concat annotations?
        MultiTapeProductFactory::new(new_terms)
            .with_optimized(self._optimized && rhs._optimized)
            .to_product()
    }
}
impl Mul<MultiTapeExpression> for MultiTapeTerm {
    type Output = MultiTapeExpression;

    fn mul(self, rhs: MultiTapeExpression) -> MultiTapeExpression {
        let mut new_products = Vec::new();
        for product in rhs.products.iter() {
            new_products.push(self.clone() * product.copy());
        }
        MultiTapeExpression::new(new_products)
    }
}
impl BitOr for MultiTapeTerm {
    type Output = MultiTapeExpression;

    fn bitor(self, rhs: Self) -> Self::Output {
        MultiTapeExpression::new(vec![
            MultiTapeProduct::new(vec![self]),
            MultiTapeProduct::new(vec![rhs])
        ])
    }
}
impl BitOr<MultiTapeProduct> for MultiTapeTerm {
    type Output = MultiTapeExpression;

    fn bitor(self, rhs: MultiTapeProduct) -> MultiTapeExpression {
        MultiTapeExpression::new(vec![
            MultiTapeProduct::new(vec![self]), rhs
        ])
    }
}
impl BitOr<MultiTapeExpression> for MultiTapeTerm {
    type Output = MultiTapeExpression;

    fn bitor(self, rhs: MultiTapeExpression) -> MultiTapeExpression {
        let mut new_products: Vec<MultiTapeProduct> = vec![
            MultiTapeProduct::new(vec![self])
        ];
        new_products.extend(rhs.products);
        MultiTapeExpression::new(new_products)
    }
}

impl AbstractMultiTapeExpression for MultiTapeTerm {
    fn copy(&self) -> Self {
        self.clone()
    }

    fn _sub(
        &self, substitutions: &HashMap<i64, MultiTapeCellState>,
        default: MultiTapeCellState
    ) -> bool {
        substitutions.get(&self.position).unwrap_or(&default) == &self.state
    }

    fn offset(&self, offset: i64) -> Self {
        MultiTapeTerm::new(self.position + offset, self.state, self._optimized)
    }

    fn _expand(
        &self, expansion_mapping: &HashMap<MultiTapeCellState, MultiTapeExpression>,
        include_debug_info: bool, fold: bool
    ) -> MultiTapeExpression {
        let mut expanded_expr =
            expansion_mapping[&self.state].offset(self.position);
        if include_debug_info {
            expanded_expr._assign_parent_debug_info(&self._debug_info);
            expanded_expr._assign_base_expansion_indexes();
        }
        if fold {
            expanded_expr = expanded_expr.to_normalized(true);
        }
        expanded_expr
    }

    fn _expand_steps(
        &self, expansion_mapping: &HashMap<MultiTapeCellState, MultiTapeExpression>,
        steps: u64, include_debug_info: bool, fold: bool
    ) -> MultiTapeExpression {
        let mut expr = self.to_expression();
        expr._assign_base_indexes();
        expr._expand_steps(
            expansion_mapping, steps, include_debug_info, fold
        )
    }

    fn to_expression(&self) -> MultiTapeExpression {
        MultiTapeExpression::new(vec![
            MultiTapeProduct::new(vec![self.copy()])
        ])
    }
    fn _to_string(&self, name: &str) -> String {
        let (tape_no, cell_state) = self.state;
        format!("{}({},{},{})", name, self.position, tape_no, cell_state)
    }
    fn _get_num_terms(&self) -> usize {
        1
    }

    fn _assign_indexes_as_base(&mut self) {
        self._assign_expr_position(
            0, 0
        )
    }
}

#[derive(Clone, Debug)]
pub struct MultiTapeProductFactory {
    pub (crate) _terms: Vec<MultiTapeTerm>,
    pub (crate) _optimized: bool,
    pub (crate) _annotation: String
}
impl MultiTapeProductFactory {
    pub fn spawn_empty() -> MultiTapeProductFactory {
        MultiTapeProductFactory {
            _terms: vec![],
            _optimized: false,
            _annotation: String::new()
        }
    }
    pub fn new(terms: Vec<MultiTapeTerm>) -> MultiTapeProductFactory {
        Self::spawn_empty().with_terms(terms)
    }
    pub fn with_terms(
        self, terms: Vec<MultiTapeTerm>
    ) -> MultiTapeProductFactory {
        let mut clone = self.clone();
        clone._terms = terms;
        clone
    }
    pub fn with_optimized(
        &mut self, optimized: bool
    ) -> MultiTapeProductFactory {
        let mut clone = self.clone();
        clone._optimized = optimized;
        clone
    }
    pub fn with_annotation(
        &mut self, annotation: String
    ) -> MultiTapeProductFactory {
        let mut clone = self.clone();
        clone._annotation = annotation;
        clone
    }
    pub fn to_product(self) -> MultiTapeProduct {
        MultiTapeProduct {
            _terms: self._terms,
            _optimized: self._optimized,
            _annotation: self._annotation.clone()
        }
    }
}

#[derive(Clone, Debug)]
pub struct MultiTapeProduct {
    pub (crate) _terms: Vec<MultiTapeTerm>,
    pub (crate) _optimized: bool,
    pub (crate) _annotation: String
}
impl MultiTapeProduct {
    pub fn new(terms: Vec<MultiTapeTerm>) -> Self {
        MultiTapeProductFactory::new(terms).to_product()
    }
    pub fn to_flat_terms(&self) -> Vec<MultiTapeTerm> {
        self._terms.clone()
    }
    pub fn get_annotation(&self) -> &String {
        &self._annotation
    }
    pub(crate) fn _get_term(&self, index: usize) -> Option<&MultiTapeTerm> {
        self._terms.get(index)
    }
    pub fn merge_products(
        products: Vec<MultiTapeProduct>
    ) -> MultiTapeProduct {
        let mut merged_terms: Vec<MultiTapeTerm> = Vec::new();
        for product in products.iter() {
            for term in product._terms.iter() {
                merged_terms.push(term.copy());
            }
        }
        MultiTapeProductFactory::new(merged_terms)
            .with_optimized(products.iter().all(|p| p._optimized))
            .to_product()
    }
    pub(crate) fn pad_terms(&self, length: usize) -> Option<MultiTapeProduct> {
        let mut new_terms = self._terms.clone();
        let current_length = self._terms.len();

        if length >= current_length {
            let pad_length = length - self._terms.len();
            let last_term = self._terms.last().unwrap();
            for _ in 0..pad_length { new_terms.push(last_term.clone()); }
            Some(MultiTapeProduct::new(new_terms))
        } else {
            None
        }
    }
    pub(crate) fn _assign_base_indexes(&mut self, product_idx: u64) {
        for (index, term) in self._terms.iter_mut().enumerate() {
            let expr_position = ExprPosition { product_idx, term_idx: index as u64 };
            term.insert_expr_position(expr_position, false);
        }
    }
    pub fn reduce(&self) -> MultiTapeProduct {
        // reduce the product into the minimal number of terms by removing duplicates
        let mut terms_set: IndexSet<MultiTapeTerm> = IndexSet::new();
        for term in self._terms.iter() {
            terms_set.insert(term.copy());
        }
        let mut reduced_terms: Vec<MultiTapeTerm> = terms_set.into_iter().collect();
        reduced_terms.sort();
        MultiTapeProduct::new(reduced_terms)
    }
    pub fn push_term(&mut self, term: MultiTapeTerm) {
        self._terms.push(term);
    }
    pub fn to_hashset(&self) -> HashSet<&MultiTapeTerm> {
        HashSet::from_iter(self._terms.iter())
    }
    pub fn to_normalized_vec(&self, sort: bool) -> Vec<MultiTapeTerm> {
        /*
        I use the term normalized here to mean that
        the returned vector has no duplicate terms
        */
        let mut unique_terms_set: HashSet<&MultiTapeTerm> = HashSet::new();
        let mut unique_terms: Vec<MultiTapeTerm> = Vec::new();

        for term in self._terms.iter() {
            if unique_terms_set.contains(term) { continue }
            unique_terms_set.insert(term);
            unique_terms.push(term.copy());
        }
        if sort { unique_terms.sort() }
        unique_terms
    }
    pub fn to_normalized(&self) -> Self {
        // TODO: should sort be passed in
        let norm_terms = self.to_normalized_vec(true);
        MultiTapeProduct::new(norm_terms)
    }
}

impl PartialEq<Self> for MultiTapeProduct {
    fn eq(&self, other: &MultiTapeProduct) -> bool {
        self._terms == other._terms
    }
}
impl Hash for MultiTapeProduct {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for term in &self._terms {
            term.hash(state);
        }
    }
}
impl Eq for MultiTapeProduct {}
impl PartialEq<MultiTapeProduct> for &MultiTapeProduct {
    fn eq(&self, other: &MultiTapeProduct) -> bool {
        self._terms == other._terms
    }
}

impl Mul for MultiTapeProduct {
    type Output = MultiTapeProduct;

    fn mul(self, rhs: Self) -> Self::Output {
        let mut new_terms: Vec<MultiTapeTerm> = Vec::new();
        for term in self._terms.iter() {
            new_terms.push(term.clone());
        }
        for term in rhs._terms.iter() {
            new_terms.push(term.clone());
        }
        MultiTapeProductFactory::new(new_terms)
            .with_optimized(self._optimized && rhs._optimized)
            .to_product()
    }
}
impl Mul<MultiTapeTerm> for &MultiTapeProduct {
    type Output = MultiTapeProduct;

    fn mul(self, rhs: MultiTapeTerm) -> MultiTapeProduct {
        let mut new_terms: Vec<MultiTapeTerm> = self._terms.clone();
        let rhs_optimized = rhs._optimized;
        new_terms.push(rhs);
        MultiTapeProductFactory::new(new_terms)
            .with_optimized(self._optimized && rhs_optimized)
            .to_product()
    }
}
impl Mul<MultiTapeTerm> for MultiTapeProduct {
    type Output = MultiTapeProduct;

    fn mul(self, rhs: MultiTapeTerm) -> MultiTapeProduct {
        let mut new_terms = self._terms.clone();
        new_terms.push(rhs.copy());
        MultiTapeProductFactory::new(new_terms)
            .with_optimized(self._optimized && rhs._optimized)
            .to_product()
    }
}
impl Mul<MultiTapeExpression> for MultiTapeProduct {
    type Output = MultiTapeExpression;

    fn mul(self, rhs: MultiTapeExpression) -> Self::Output {
        let mut new_products = Vec::new();
        for product in rhs.products.iter() {
            new_products.push(self.copy() * product.copy());
        }
        MultiTapeExpression::new(new_products)
    }
}
impl BitOr for MultiTapeProduct {
    type Output = MultiTapeExpression;

    fn bitor(self, rhs: Self) -> Self::Output {
        MultiTapeExpression::new(vec![
            self, rhs
        ])
    }
}
impl BitOr<MultiTapeTerm> for MultiTapeProduct {
    type Output = MultiTapeExpression;

    fn bitor(self, rhs: MultiTapeTerm) -> Self::Output {
        MultiTapeExpression::new(vec![
            self, MultiTapeProduct::new(vec![rhs])
        ])
    }
}
impl BitOr<MultiTapeExpression> for MultiTapeProduct {
    type Output = MultiTapeExpression;

    fn bitor(self, rhs: MultiTapeExpression) -> Self::Output {
        let mut new_products = vec![self];
        new_products.extend(rhs.products);
        MultiTapeExpression::new(new_products)
    }
}
impl PartialOrd<Self> for MultiTapeProduct {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for MultiTapeProduct {
    fn cmp(&self, other: &Self) -> Ordering {
        self._terms.len().cmp(&other._terms.len())
            .then_with(|| self._terms.cmp(&other._terms))
    }
}
impl AbstractMultiTapeExpression for MultiTapeProduct {
    fn copy(&self) -> Self {
        MultiTapeProductFactory::new(self._terms.clone())
            .with_optimized(self._optimized)
            .with_annotation(self._annotation.clone())
            .to_product()
    }
    fn _sub(
        &self, substitutions: &HashMap<i64, MultiTapeCellState>,
        default: MultiTapeCellState
    ) -> bool {
        for term in self._terms.iter() {
            if !term._sub(substitutions, default) {
                return false;
            }
        }
        true
    }
    fn offset(&self, offset: i64) -> Self {
        let mut new_terms: Vec<MultiTapeTerm> = Vec::new();
        for term in self._terms.iter() {
            new_terms.push(term.offset(offset));
        }
        MultiTapeProduct::new(new_terms)
    }
    fn _expand(
        &self, expansion_mapping: &HashMap<MultiTapeCellState, MultiTapeExpression>,
        include_debug_info: bool, fold: bool
    ) -> MultiTapeExpression {
        if self._terms.len() == 0 {
            return MultiTapeExpression::new(vec![]);
        }

        let first_term_opt = self._terms.get(0);
        let mut result = match first_term_opt {
            Some(term) => term._expand(
                expansion_mapping, include_debug_info, fold
            ),
            None => return MultiTapeExpression::new(vec![])
        };
        for term in self._terms.iter().skip(1) {
            result = result * term._expand(
                expansion_mapping, include_debug_info, fold
            );
        }
        result._assign_expr_positions();
        result
    }
    fn _expand_steps(
        &self, expansion_mapping: &HashMap<MultiTapeCellState, MultiTapeExpression>,
        steps: u64, include_debug_info: bool, fold: bool
    ) -> MultiTapeExpression {
        let mut copy = self.copy();
        copy._assign_indexes_as_base();
        let mut result = copy._expand(
            expansion_mapping, include_debug_info, fold
        );
        for _ in 1..steps {
            result = result._expand(
                expansion_mapping, include_debug_info, fold
            );
        }
        result
    }
    fn to_expression(&self) -> MultiTapeExpression {
        MultiTapeExpression::new(vec![self.copy()])
    }
    fn _to_string(&self, name: &str) -> String {
        self._terms
            .iter()
            .map(|term| term._to_string(name))
            .collect::<Vec<String>>()
            .join("*")
    }
    fn _get_num_terms(&self) -> usize {
        self._terms.len()
    }

    fn _assign_indexes_as_base(&mut self) {
        self._assign_base_indexes(0);
    }
}

#[derive(Clone, Debug, Default)]
pub struct MultiTapeExpression {
    pub (crate) products: Vec<MultiTapeProduct>,
    pub (crate) _optimized: bool
}
impl MultiTapeExpression {
    pub fn new(products: Vec<MultiTapeProduct>) -> Self {
        MultiTapeExpression {
            products, _optimized: false
        }
    }
    pub fn _get_num_products(&self) -> usize {
        self.products.len()
    }
    pub fn _get_products(&self) -> &Vec<MultiTapeProduct> { &self.products }
    pub fn _get_product(&self, index: usize) -> Option<&MultiTapeProduct> {
        self.products.get(index)
    }
    pub fn _assign_base_indexes(&mut self) {
        for (index, product) in self.products.iter_mut().enumerate() {
            product._assign_base_indexes(index as u64);
        }
    }
    pub fn _to_flat_terms(&self) -> Vec<MultiTapeTerm> {
        let mut flat_terms: Vec<MultiTapeTerm> = Vec::new();
        for product in self.products.iter() {
            for term in product._terms.iter() {
                flat_terms.push(term.copy());
            }
        }
        flat_terms
    }
    pub fn _assign_parent_debug_info(
        &mut self, parent_debug_info: &ExprDebugInfo
    ) {
        for product in self.products.iter_mut() {
            product._terms.iter_mut().for_each(|term| {
                term._debug_info.parent_position_info =
                    parent_debug_info.position_info.clone()
            });
        }
    }
    fn _assign_base_expansion_indexes(&mut self) {
        // TODO: we really need to do a unittest for this
        //  wasted days cause we incremented before assignment lmao
        let mut expansion_index: usize = 0;

        for product in self.products.iter_mut() {
            product._terms.iter_mut().for_each(|term| {
                term._debug_info.expansion_index = expansion_index as u16;
                expansion_index += 1;
            });
        }
    }
    pub fn _assign_expr_positions(&mut self) {
        for (product_index, product) in self.products.iter_mut().enumerate() {
            product._assign_base_indexes(product_index as u64);
        }
    }
    pub fn pad_products(&self, new_num_products: usize) -> Option<MultiTapeExpression> {
        let mut new_products = self.products.clone();
        let last_product = self.products.last().unwrap();

        while new_products.len() < new_num_products {
            new_products.push(last_product.clone());
        }
        Some(MultiTapeExpression::new(new_products))
    }
    pub fn reduce(&self) -> MultiTapeExpression {
        let mut products_set: IndexSet<MultiTapeProduct> = IndexSet::new();
        for product in self.products.iter() {
            products_set.insert(product.reduce());
        }
        let mut reduced_products: Vec<MultiTapeProduct> = products_set.into_iter().collect();
        reduced_products.sort();
        MultiTapeExpression::new(reduced_products)
    }
    pub fn push_product(&mut self, product: MultiTapeProduct) {
        self.products.push(product);
    }
    pub fn to_normalized_vec(&self, sort: bool) -> Vec<MultiTapeProduct> {
        /*
        I use the term normalized here to mean that
        the returned vector has no duplicate products
        */
        let mut unique_products_set: HashSet<MultiTapeProduct> = HashSet::new();
        let mut unique_products: Vec<MultiTapeProduct> = Vec::new();

        for un_normalized_product in self.products.iter() {
            let product = un_normalized_product.to_normalized();
            if unique_products_set.contains(&product) { continue }
            unique_products_set.insert(product.copy());
            unique_products.push(product.copy());
        }
        if sort { unique_products.sort() }
        unique_products
    }
    pub fn to_normalized(&self, sort: bool) -> Self {
        let norm_products = self.to_normalized_vec(sort);
        MultiTapeExpression::new(norm_products)
    }
}
impl PartialEq<MultiTapeExpression> for &MultiTapeExpression {
    fn eq(&self, other: &MultiTapeExpression) -> bool {
        self.products == other.products
    }
}
impl Hash for MultiTapeExpression {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for product in &self.products {
            product.hash(state);
        }
    }
}
impl Eq for MultiTapeExpression {}
impl PartialEq<MultiTapeExpression> for MultiTapeExpression {
    fn eq(&self, other: &MultiTapeExpression) -> bool {
        self.products == other.products
    }
}

impl Mul for MultiTapeExpression {
    type Output = MultiTapeExpression;

    fn mul(self, rhs: Self) -> Self::Output {
        let mut multiplied_products = Vec::new();
        for product in self.products.iter() {
            for rhs_product in rhs.products.iter() {
                let new_product = product.copy() * rhs_product.copy();
                multiplied_products.push(new_product);
            }
        }
        MultiTapeExpression::new(multiplied_products)
    }
}
impl Mul<MultiTapeTerm> for MultiTapeExpression {
    type Output = MultiTapeExpression;

    fn mul(self, rhs: MultiTapeTerm) -> Self::Output {
        // println!("POTATO_PRINT");
        let mut new_products = Vec::new();
        for product in self.products.iter() {
            // println!("PROD_MUL");
            new_products.push(product * rhs.copy());
        }
        MultiTapeExpression::new(new_products)
    }
}
impl Mul<MultiTapeProduct> for MultiTapeExpression {
    type Output = MultiTapeExpression;

    fn mul(self, rhs: MultiTapeProduct) -> Self::Output {
        let mut new_products = Vec::new();
        for product in self.products.iter() {
            new_products.push(product.copy() * rhs.copy());
        }
        MultiTapeExpression::new(new_products)
    }
}
impl BitOr for MultiTapeExpression {
    type Output = MultiTapeExpression;

    fn bitor(self, rhs: Self) -> Self::Output {
        let mut new_products = Vec::new();
        for product in self.products.iter() {
            new_products.push(product.copy());
        }
        for product in rhs.products.iter() {
            new_products.push(product.copy());
        }
        MultiTapeExpression::new(new_products)
    }
}
impl BitOr<MultiTapeTerm> for MultiTapeExpression {
    type Output = MultiTapeExpression;

    fn bitor(self, rhs: MultiTapeTerm) -> Self::Output {
        let mut new_products: Vec<MultiTapeProduct> = Vec::new();
        for product in self.products.iter() {
            new_products.push(product.copy());
        }
        new_products.push(rhs._to_product());
        MultiTapeExpression::new(new_products)
    }
}
impl BitOr<MultiTapeProduct> for MultiTapeExpression {
    type Output = MultiTapeExpression;

    fn bitor(self, rhs: MultiTapeProduct) -> Self::Output {
        let mut new_products: Vec<MultiTapeProduct> = Vec::new();
        for product in self.products.iter() {
            new_products.push(product.copy());
        }
        new_products.push(rhs);
        MultiTapeExpression::new(new_products)
    }
}

impl AbstractMultiTapeExpression for MultiTapeExpression {
    fn copy(&self) -> Self {
        let mut products = Vec::new();
        for product in self.products.iter() {
            products.push(product.copy());
        }
        MultiTapeExpression { products, _optimized: self._optimized }
    }
    fn _sub(
        &self, substitutions: &HashMap<i64, MultiTapeCellState>,
        default: MultiTapeCellState
    ) -> bool {
        for product in self.products.iter() {
            if product._sub(substitutions, default) {
                return true;
            }
        }
        false
    }
    fn offset(&self, offset: i64) -> Self {
        let mut products = Vec::new();
        for product in self.products.iter() {
            products.push(product.offset(offset));
        }
        MultiTapeExpression::new(products)
    }
    fn _expand(
        &self, expansion_mapping: &HashMap<MultiTapeCellState, MultiTapeExpression>,
        include_debug_info: bool, fold: bool
    ) -> MultiTapeExpression {
        /*
        // non-parallelized implementation
        let mut expanded_expression = MultiTapeExpression::new(vec![]);
        for product in self.products.iter() {
            let expanded_subexpression = product._expand(expansion_mapping);
            expanded_expression = expanded_expression | expanded_subexpression;
        }
        */
        let mut result = self.products.par_iter()
            .map(|product|
                product._expand(expansion_mapping, include_debug_info, fold)
            ).reduce(
            // sum up all the individual expanded products
            || MultiTapeExpression::new(vec![]),
            |a, b| { a | b }
        );
        if fold { result = result.to_normalized(true); }
        if include_debug_info { result._assign_expr_positions(); }
        result
    }
    fn _expand_steps(
        &self, expansion_mapping: &HashMap<MultiTapeCellState, MultiTapeExpression>,
        steps: u64, include_debug_info: bool, fold: bool
    ) -> MultiTapeExpression {
        let mut result = self.copy();
        result._assign_base_indexes();
        for _ in 0..steps {
            result = result._expand(
                expansion_mapping, include_debug_info, fold
            );
        }
        result
    }
    fn to_expression(&self) -> MultiTapeExpression {
        self.copy()
    }
    fn _to_string(&self, name: &str) -> String {
        self.products
            .iter()
            .map(|product| product._to_string(name))
            .collect::<Vec<String>>()
            .join(" | ")
    }
    fn _get_num_terms(&self) -> usize {
        self.products.iter().map(|product| product._get_num_terms()).sum()
    }

    fn _assign_indexes_as_base(&mut self) {
        self._assign_base_indexes()
    }
}

pub fn validate_debug_info_exists(expr: &MultiTapeExpression) {
    for (product_index, product) in expr.products.iter().enumerate() {
        for (term_index, term) in product._terms.iter().enumerate() {
            let position_info = &term._debug_info.position_info;
            assert!(
                position_info.is_some(),
                "Term {:?} at product_idx {} term_idx {} does not have position info",
                term, product_index, term_index
            )
        }
    }
}
