use std::cmp::Ordering;
use rayon::iter::ParallelIterator;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::{BitOr, Mul};
use indexmap::IndexSet;
use rayon::iter::IntoParallelRefIterator;
use serde::{Deserialize, Serialize};

pub type CellState = u32;
pub fn clip_after_space(s: String) -> String {
    s.split_whitespace().next().unwrap_or(&s).to_string()
}

pub trait AbstractExpression: Mul + BitOr + Eq + Sized {
    fn copy(&self) -> Self;
    fn _sub(&self, substitutions: &HashMap<i64, CellState>, default: CellState) -> bool;
    fn offset(&self, offset: i64) -> Self;
    fn _expand(
        &self, expansion_mapping: &HashMap<CellState, Expression>,
        include_debug_info: bool, fold: bool
    ) -> Expression;
    fn _expand_steps(
        &self, expansion_mapping: &HashMap<CellState, Expression>,
        steps: u64, include_debug_info: bool, fold: bool
    ) -> Expression;
    fn to_expression(&self) -> Expression;
    fn _to_string(&self, name: &str) -> String;
    fn _get_num_terms(&self) -> usize;
    fn _assign_indexes_as_base(&mut self);
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExprPosition {
    pub (crate) product_idx: u64,
    pub (crate) term_idx: u64
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExprDebugInfo {
    pub (crate) expansion_index: u16,
    pub (crate) position_info: Option<ExprPosition>,
    pub (crate) parent_position_info: Option<ExprPosition>
}
impl ExprDebugInfo {
    pub fn spawn_empty() -> Self {
        ExprDebugInfo {
            expansion_index: 0,
            position_info: None,
            parent_position_info: None
        }
    }
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct Term {
    // position within the cellular automata tape
    pub position: i64,
    pub state: CellState,
    // TODO: implement optimization
    pub (crate) _optimized: bool,
    // position of the term within an expression
    pub (crate) _debug_info: ExprDebugInfo,
}

impl PartialEq<Term> for Term {
    fn eq(&self, other: &Self) -> bool {
        self.position == other.position && self.state == other.state
    }
}
impl PartialOrd<Term> for Term {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Term {
    fn cmp(&self, other: &Self) -> Ordering {
        self.position.cmp(&other.position)
            .then_with(|| self.state.cmp(&other.state))
    }
}

impl Hash for Term {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.position.hash(state);
        self.state.hash(state);
    }
}
impl Term {
    pub fn new(position: i64, state: CellState, optimized: bool) -> Term {
        Term {
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

    pub fn _to_product(&self) -> Product {
        Product::new(vec![self.copy()])
    }
    pub fn _get_name() -> String {
        const TERM_EXAMPLE: Term = Term {
            position: 0, state: 0, _optimized: false,
            _debug_info: ExprDebugInfo {
                expansion_index: 0,
                position_info: None,
                parent_position_info: None
            }
        };
        clip_after_space(format!("{:?}", TERM_EXAMPLE))
    }
}

impl Mul for Term {
    type Output = Product;

    fn mul(self, rhs: Self) -> Self::Output {
        Product::new(vec![self, rhs])
    }
}
impl Mul<Product> for Term {
    type Output = Product;

    fn mul(self, rhs: Product) -> Product {
        let mut new_terms: Vec<Term> = Vec::new();
        let rhs_optimized = rhs._optimized;
        new_terms.push(self.clone());
        for term in rhs._terms.iter() {
            new_terms.push(term.copy());
        }
        ProductFactory::new(new_terms)
            .with_optimized(self._optimized && rhs_optimized)
            .to_product()
    }
}
impl Mul<Expression> for Term {
    type Output = Expression;

    fn mul(self, rhs: Expression) -> Expression {
        let mut new_products = Vec::new();
        for product in rhs.products.iter() {
            new_products.push(self.clone() * product.copy());
        }
        Expression::new(new_products)
    }
}
impl BitOr for Term{
    type Output = Expression;

    fn bitor(self, rhs: Self) -> Self::Output {
        Expression::new(vec![
            Product::new(vec![self]), Product::new(vec![rhs])
        ])
    }
}
impl BitOr<Product> for Term {
    type Output = Expression;

    fn bitor(self, rhs: Product) -> Expression {
        Expression::new(vec![
            Product::new(vec![self]), rhs
        ])
    }
}
impl BitOr<Expression> for Term {
    type Output = Expression;

    fn bitor(self, rhs: Expression) -> Expression {
        let mut new_products: Vec<Product> = vec![
            Product::new(vec![self])
        ];
        new_products.extend(rhs.products);
        Expression::new(new_products)
    }
}

impl AbstractExpression for Term {
    fn copy(&self) -> Self {
        self.clone()
    }

    fn _sub(
        &self, substitutions: &HashMap<i64, CellState>, default: CellState
    ) -> bool {
        substitutions.get(&self.position).unwrap_or(&default) == &self.state
    }

    fn offset(&self, offset: i64) -> Self {
        Term::new(self.position + offset, self.state, self._optimized)
    }

    fn _expand(
        &self, expansion_mapping: &HashMap<CellState, Expression>,
        include_debug_info: bool, fold: bool
    ) -> Expression {
        let mut expanded_expr = expansion_mapping[&self.state].offset(self.position);
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
        &self, expansion_mapping: &HashMap<CellState, Expression>,
        steps: u64, include_debug_info: bool, fold: bool
    ) -> Expression {
        let mut expr = self.to_expression();
        expr._assign_base_indexes();
        expr._expand_steps(
            expansion_mapping, steps, include_debug_info, fold
        )
    }

    fn to_expression(&self) -> Expression {
        Expression::new(vec![
            Product::new(vec![self.copy()])
        ])
    }
    fn _to_string(&self, name: &str) -> String {
        format!("{}({},{})", name, self.position, self.state)
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
pub struct  ProductFactory {
    pub (crate) _terms: Vec<Term>,
    pub (crate) _optimized: bool,
    pub (crate) _annotation: String
}
impl ProductFactory {
    pub fn spawn_empty() -> ProductFactory {
        ProductFactory {
            _terms: vec![],
            _optimized: false,
            _annotation: String::new()
        }
    }
    pub fn new(terms: Vec<Term>) -> ProductFactory {
        Self::spawn_empty().with_terms(terms)
    }
    pub fn with_terms(
        self, terms: Vec<Term>
    ) -> ProductFactory {
        let mut clone = self.clone();
        clone._terms = terms;
        clone
    }
    pub fn with_optimized(
        &mut self, optimized: bool
    ) -> ProductFactory {
        let mut clone = self.clone();
        clone._optimized = optimized;
        clone
    }
    pub fn with_annotation(
        &mut self, annotation: String
    ) -> ProductFactory {
        let mut clone = self.clone();
        clone._annotation = annotation;
        clone
    }
    pub fn to_product(self) -> Product {
        Product {
            _terms: self._terms,
            _optimized: self._optimized,
            _annotation: self._annotation.clone()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Product {
    pub (crate) _terms: Vec<Term>,
    pub (crate) _optimized: bool,
    pub (crate) _annotation: String
}
impl Product {
    pub fn new(terms: Vec<Term>) -> Self {
        ProductFactory::new(terms).to_product()
    }
    pub fn to_flat_terms(&self) -> Vec<Term> {
        self._terms.clone()
    }
    pub(crate) fn _get_term(&self, index: usize) -> Option<&Term> {
        self._terms.get(index)
    }
    pub fn pad_terms(&self, length: usize) -> Option<Product> {
        let mut new_terms = self._terms.clone();
        let current_length = self._terms.len();

        if length >= current_length {
            let pad_length = length - self._terms.len();
            let last_term = self._terms.last().unwrap();
            for _ in 0..pad_length { new_terms.push(last_term.clone()); }
            Some(Product::new(new_terms))
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
    pub fn reduce(&self) -> Product {
        // reduce the product into the minimal number of terms by removing duplicates
        let mut terms_set: IndexSet<Term> = IndexSet::new();
        for term in self._terms.iter() {
            terms_set.insert(term.copy());
        }
        let mut reduced_terms: Vec<Term> = terms_set.into_iter().collect();
        reduced_terms.sort();
        Product::new(reduced_terms)
    }
    pub fn push_term(&mut self, term: Term) {
        self._terms.push(term);
    }
    pub fn to_hashset(&self) -> HashSet<&Term> {
        HashSet::from_iter(self._terms.iter())
    }
    pub fn to_normalized_vec(&self, sort: bool) -> Vec<Term> {
        /*
        I use the term normalized here to mean that
        the returned vector has no duplicate terms
        */
        let mut unique_terms_set: HashSet<&Term> = HashSet::new();
        let mut unique_terms: Vec<Term> = Vec::new();

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
        Product::new(norm_terms)
    }
    pub(crate) fn _to_string_with_annotation(&self, name: &str) -> String {
        let product_string = self._to_string(name);
        format!("{}:{}", product_string, self._annotation)
    }
}
impl Display for Product {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        return write!(
            f, "{}", self._to_string("A")
        );
    }
}
impl PartialEq<Self> for Product {
    fn eq(&self, other: &Product) -> bool {
        self._terms == other._terms
    }
}
impl Hash for Product {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for term in &self._terms {
            term.hash(state);
        }
    }
}
impl Eq for Product {}
impl PartialEq<Product> for &Product {
    fn eq(&self, other: &Product) -> bool {
        self._terms == other._terms
    }
}

impl Mul for Product {
    type Output = Product;

    fn mul(self, rhs: Self) -> Self::Output {
        let mut new_terms: Vec<Term> = Vec::new();
        for term in self._terms.iter() {
            new_terms.push(term.clone());
        }
        for term in rhs._terms.iter() {
            new_terms.push(term.clone());
        }
        ProductFactory::new(new_terms)
            .with_optimized(self._optimized && rhs._optimized)
            .to_product()
    }
}
impl Mul<Term> for &Product {
    type Output = Product;

    fn mul(self, rhs: Term) -> Product {
        let mut new_terms: Vec<Term> = self._terms.clone();
        new_terms.push(rhs.copy());
        ProductFactory::new(new_terms)
            .with_optimized(self._optimized && rhs._optimized)
            .with_annotation(self._annotation.clone())
            .to_product()
    }
}
impl Mul<Term> for Product {
    type Output = Product;

    fn mul(self, rhs: Term) -> Product {
        let mut new_terms = self._terms.clone();
        new_terms.push(rhs.copy());
        ProductFactory::new(new_terms)
            .with_optimized(self._optimized && rhs._optimized)
            .with_annotation(self._annotation)
            .to_product()
    }
}
impl Mul<Expression> for Product {
    type Output = Expression;

    fn mul(self, rhs: Expression) -> Self::Output {
        let mut new_products = Vec::new();
        for product in rhs.products.iter() {
            new_products.push(self.copy() * product.copy());
        }
        Expression::new(new_products)
    }
}
impl BitOr for Product {
    type Output = Expression;

    fn bitor(self, rhs: Self) -> Self::Output {
        Expression::new(vec![
            self, rhs
        ])
    }
}
impl BitOr<Term> for Product {
    type Output = Expression;

    fn bitor(self, rhs: Term) -> Self::Output {
        Expression::new(vec![
            self, Product::new(vec![rhs])
        ])
    }
}
impl BitOr<Expression> for Product {
    type Output = Expression;

    fn bitor(self, rhs: Expression) -> Self::Output {
        let mut new_products = vec![self];
        new_products.extend(rhs.products);
        Expression::new(new_products)
    }
}
impl PartialOrd<Self> for Product {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Product {
    fn cmp(&self, other: &Self) -> Ordering {
        self._terms.len().cmp(&other._terms.len())
            .then_with(|| self._terms.cmp(&other._terms))
    }
}
impl AbstractExpression for Product {
    fn copy(&self) -> Self {
        Product::new(self._terms.clone())
    }
    fn _sub(&self, substitutions: &HashMap<i64, CellState>, default: CellState) -> bool {
        for term in self._terms.iter() {
            if !term._sub(substitutions, default) {
                return false;
            }
        }
        true
    }
    fn offset(&self, offset: i64) -> Self {
        let mut new_terms: Vec<Term> = Vec::new();
        for term in self._terms.iter() {
            new_terms.push(term.offset(offset));
        }
        Product::new(new_terms)
    }
    fn _expand(
        &self, expansion_mapping: &HashMap<CellState, Expression>,
        include_debug_info: bool, fold: bool
    ) -> Expression {
        if self._terms.len() == 0 {
            return Expression::new(vec![]);
        }

        let first_term_opt = self._terms.get(0);
        let mut result = match first_term_opt {
            Some(term) => term._expand(
                expansion_mapping, include_debug_info, fold
            ),
            None => return Expression::new(vec![])
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
        &self, expansion_mapping: &HashMap<CellState, Expression>,
        steps: u64, include_debug_info: bool, fold: bool
    ) -> Expression {
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
    fn to_expression(&self) -> Expression {
        Expression::new(vec![self.copy()])
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Expression {
    pub products: Vec<Product>,
    pub (crate) _optimized: bool
}
impl Expression {
    pub fn new(products: Vec<Product>) -> Self {
        Expression {
            products, _optimized: false
        }
    }
    pub fn _get_num_products(&self) -> usize {
        self.products.len()
    }
    pub fn _get_products(&self) -> &Vec<Product> { &self.products }
    pub fn _get_product(&self, index: usize) -> Option<&Product> {
        self.products.get(index)
    }
    pub fn _assign_base_indexes(&mut self) {
        for (index, product) in self.products.iter_mut().enumerate() {
            product._assign_base_indexes(index as u64);
        }
    }
    pub fn _to_flat_terms(&self) -> Vec<Term> {
        let mut flat_terms: Vec<Term> = Vec::new();
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
    pub fn pad_products(&self, new_num_products: usize) -> Option<Expression> {
        let mut new_products = self.products.clone();
        let last_product = self.products.last().unwrap();

        while new_products.len() < new_num_products {
            new_products.push(last_product.clone());
        }
        Some(Expression::new(new_products))
    }
    pub fn reduce(&self) -> Expression {
        let mut products_set: IndexSet<Product> = IndexSet::new();
        for product in self.products.iter() {
            products_set.insert(product.reduce());
        }
        let mut reduced_products: Vec<Product> = products_set.into_iter().collect();
        reduced_products.sort();
        Expression::new(reduced_products)
    }
    pub fn push_product(&mut self, product: Product) {
        self.products.push(product);
    }
    pub fn to_normalized_vec(&self, sort: bool) -> Vec<Product> {
        /*
        I use the term normalized here to mean that
        the returned vector has no duplicate products
        */
        let mut unique_products_set: HashSet<Product> = HashSet::new();
        let mut unique_products: Vec<Product> = Vec::new();

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
        Expression::new(norm_products)
    }
}
impl PartialEq<Expression> for &Expression {
    fn eq(&self, other: &Expression) -> bool {
        self.products == other.products
    }
}
impl Hash for Expression {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for product in &self.products {
            product.hash(state);
        }
    }
}
impl Eq for Expression {}
impl PartialEq<Expression> for Expression {
    fn eq(&self, other: &Expression) -> bool {
        self.products == other.products
    }
}

impl Mul for Expression {
    type Output = Expression;

    fn mul(self, rhs: Self) -> Self::Output {
        let mut multiplied_products = Vec::new();
        for product in self.products.iter() {
            for rhs_product in rhs.products.iter() {
                let new_product = product.copy() * rhs_product.copy();
                multiplied_products.push(new_product);
            }
        }
        Expression::new(multiplied_products)
    }
}
impl Mul<Term> for Expression {
    type Output = Expression;

    fn mul(self, rhs: Term) -> Self::Output {
        // println!("POTATO_PRINT");
        let mut new_products = Vec::new();
        for product in self.products.iter() {
            // println!("PROD_MUL");
            new_products.push(product * rhs.copy());
        }
        Expression::new(new_products)
    }
}
impl Mul<Product> for Expression {
    type Output = Expression;

    fn mul(self, rhs: Product) -> Self::Output {
        let mut new_products = Vec::new();
        for product in self.products.iter() {
            new_products.push(product.copy() * rhs.copy());
        }
        Expression::new(new_products)
    }
}
impl BitOr for Expression {
    type Output = Expression;

    fn bitor(self, rhs: Self) -> Self::Output {
        let mut new_products = Vec::new();
        for product in self.products.iter() {
            new_products.push(product.copy());
        }
        for product in rhs.products.iter() {
            new_products.push(product.copy());
        }
        Expression::new(new_products)
    }
}
impl BitOr<Term> for Expression {
    type Output = Expression;

    fn bitor(self, rhs: Term) -> Self::Output {
        let mut new_products: Vec<Product> = Vec::new();
        for product in self.products.iter() {
            new_products.push(product.copy());
        }
        new_products.push(rhs._to_product());
        Expression::new(new_products)
    }
}
impl BitOr<Product> for Expression {
    type Output = Expression;

    fn bitor(self, rhs: Product) -> Self::Output {
        let mut new_products: Vec<Product> = Vec::new();
        for product in self.products.iter() {
            new_products.push(product.copy());
        }
        new_products.push(rhs);
        Expression::new(new_products)
    }
}

impl AbstractExpression for Expression {
    fn copy(&self) -> Self {
        let mut products = Vec::new();
        for product in self.products.iter() {
            products.push(product.copy());
        }
        Expression { products, _optimized: self._optimized }
    }
    fn _sub(
        &self, substitutions: &HashMap<i64, CellState>, default: CellState
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
        Expression::new(products)
    }
    fn _expand(
        &self, expansion_mapping: &HashMap<CellState, Expression>,
        include_debug_info: bool, fold: bool
    ) -> Expression {
        /*
        // non-parallelized implementation
        let mut expanded_expression = Expression::new(vec![]);
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
            || Expression::new(vec![]),
            |a, b| { a | b }
        );
        if fold { result = result.to_normalized(true); }
        if include_debug_info { result._assign_expr_positions(); }
        result
    }
    fn _expand_steps(
        &self, expansion_mapping: &HashMap<CellState, Expression>,
        steps: u64, include_debug_info: bool, fold: bool
    ) -> Expression {
        let mut result = self.copy();
        result._assign_base_indexes();
        for _ in 0..steps {
            result = result._expand(
                expansion_mapping, include_debug_info, fold
            );
        }
        result
    }
    fn to_expression(&self) -> Expression {
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

pub fn validate_debug_info_exists(expr: &Expression) {
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use super::*;

    fn spawn_test_pos_empty_expr() -> Expression {
        Term::new(-1, 0, false) * Term::new(0, 0, false) * Term::new(1, 1, false) |
            Term::new(-1, 0, false) * Term::new(0, 1, false) * Term::new(1, 0, false) |
            Term::new(-1, 1, false) * Term::new(0, 1, false) * Term::new(1, 0, false) |
            Term::new(-1, 1, false) * Term::new(0, 0, false) * Term::new(1, 1, false) |
            Term::new(-1, 1, false) * Term::new(0, 1, false) * Term::new(1, 1, false)
    }
    fn spawn_test_neg_empty_expr() -> Expression {
        Term::new(-1, 0, false) * Term::new(0, 0, false) * Term::new(1, 0, false) |
            Term::new(-1, 0, false) * Term::new(0, 1, false) * Term::new(1, 1, false) |
            Term::new(-1, 1, false) * Term::new(0, 1, false) * Term::new(1, 1, false) |
            Term::new(-1, 0, false) * Term::new(0, 0, false) * Term::new(1, 0, false) |
            Term::new(-1, 0, false) * Term::new(0, 0, false) * Term::new(1, 0, false)
    }

    #[test]
    fn a_test() {
        let a = Term::new(0, 1, false);
        let b = Term::new(0, 1, true);
        assert_eq!(a, b)
    }

    #[test]
    fn expr_mul_test() {
        let a = Term::new(0, 1, false) | Term::new(1, 1, false);
        let b = Term::new(2, 1, false) | Term::new(2, 1, false);
        let c = a * b;
        println!("EXPR_MUL_TEST: {:?}", c._to_string("A"));
        assert_eq!(c._get_num_products(), 4);
    }

    #[test]
    fn expansion_test() {
        let pos_exp: Expression = spawn_test_pos_empty_expr();
        let neg_exp: Expression = spawn_test_neg_empty_expr();

        let expr_mapping: HashMap<CellState, Expression> = [
            (0, pos_exp.clone()),
            (1, neg_exp.clone())
        ].iter().cloned().collect();

        let pos_seed_exp = Term::new(0, 0, false);
        let neg_seed_exp = Term::new(0, 1, false);
        let pos_expanded_expr = pos_seed_exp._expand_steps(
            &expr_mapping, 1, false, false
        );
        let neg_expanded_expr =
            neg_seed_exp._expand_steps(
                &expr_mapping, 1, false, false
            );

        println!("\nEXPANSION_CMP");
        println!("POS_EXPR {}", pos_expanded_expr._to_string("A"));
        println!("POS_B_EXP {}", pos_exp._to_string("A"));
        println!("NEG_B_EXP {}", neg_exp._to_string("A"));
        assert_eq!(pos_expanded_expr, pos_exp);
        assert_eq!(neg_expanded_expr, neg_exp);
    }

    #[test]
    fn chain_expansion_test() {
        let pos_exp: Expression = spawn_test_pos_empty_expr();
        let neg_exp: Expression = spawn_test_neg_empty_expr();

        let expr_mapping: HashMap<CellState, Expression> = [
            (0, pos_exp.clone()),
            (1, neg_exp.clone())
        ].iter().cloned().collect();

        let pos_seed_exp =
            Term::new(0, 0, false) *
                Term::new(1, 0, false)
            ;
        // let neg_seed_exp = Term::new(0, 1, false);
        let pos_expanded_expr =
            pos_seed_exp._expand_steps(
                &expr_mapping, 1, false, false
            );
        // let neg_expanded_expr = neg_seed_exp._expand_steps(&expr_mapping, 1);

        println!("\nEXPANSION_CMP_2");
        println!("CHAIN_POS_EXPR {}", pos_expanded_expr._to_string("A"));
        assert_eq!(pos_expanded_expr._get_product(0).unwrap()._get_num_terms(), 6);
    }

    #[test]
    fn product_debug_info_test() {
        let pos_exp: Expression = spawn_test_pos_empty_expr();
        let neg_exp: Expression = spawn_test_neg_empty_expr();

        let expr_mapping: HashMap<CellState, Expression> = [
            (0, pos_exp.clone()),
            (1, neg_exp.clone())
        ].iter().cloned().collect();

        let pos_seed_prod =
            Term::new(0, 0, false) *
                Term::new(1, 0, false)
            ;

        for steps in 1..3 {
            println!("\nEXPANSION_CMP_DEBUG_{}", steps);
            let pos_expanded_expr =
                pos_seed_prod._expand_steps(
                    &expr_mapping, steps, true, false
                );
            validate_debug_info_exists(&pos_expanded_expr);
        }
    }

    #[test]
    fn term_debug_info_test() {
        let pos_exp: Expression = spawn_test_pos_empty_expr();
        let neg_exp: Expression = spawn_test_neg_empty_expr();

        let expr_mapping: HashMap<CellState, Expression> = [
            (0, pos_exp.clone()),
            (1, neg_exp.clone())
        ].iter().cloned().collect();

        let pos_seed_term =
            Term::new(0, 0, false);

        for steps in 1..3 {
            println!("\nEXPANSION_CMP_DEBUG_{}", steps);
            let pos_expanded_expr =
                pos_seed_term._expand_steps(
                    &expr_mapping, steps, true, false
                );
            validate_debug_info_exists(&pos_expanded_expr);
        }
    }

    #[test]
    fn correct_expr_pos_test() {
        let pos_exp: Expression = spawn_test_pos_empty_expr();
        let neg_exp: Expression = spawn_test_neg_empty_expr();

        let expr_mapping: HashMap<CellState, Expression> = [
            (0, pos_exp.clone()),
            (1, neg_exp.clone())
        ].iter().cloned().collect();

        let pos_seed_term =
            Term::new(0, 0, false);

        for steps in 1..3 {
            println!("\nEXPANSION_CMP_DEBUG_{}", steps);
            let pos_expanded_expr =
                pos_seed_term._expand_steps(
                    &expr_mapping, steps, true, false
                );

            for (product_index, product) in
                pos_expanded_expr.products.iter().enumerate()
            {
                for (term_index, term) in product._terms.iter().enumerate() {
                    let position_info = &term._debug_info.position_info;
                    assert!(
                        position_info.is_some(),
                        "Term {:?} at product_idx {} term_idx {} does not have position info",
                        term, product_index, term_index
                    );
                    if let Some(pos_info) = position_info {
                        assert_eq!(pos_info.product_idx, product_index as u64);
                        assert_eq!(pos_info.term_idx, term_index as u64);
                    }
                }
            }
        }
    }

    #[test]
    fn correct_product_length_test() {
        let p = Term::new(0, 0, false) * Term::new(1, 0, false);
        assert_eq!(p._terms.len(), 2);
        let p_padded = p.pad_terms(5).unwrap();
        assert_eq!(p_padded._terms.len(), 5);
    }
}
