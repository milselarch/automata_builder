use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use pyo3::{pyclass, pymethods, PyResult};
use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3_stub_gen::define_stub_info_gatherer;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use crate::automata::terms::{
    clip_after_space, validate_debug_info_exists,
    AbstractExpression, CellState, ExprDebugInfo, Expression,
    Product, ProductFactory, Term
};
/*
Reasons to redo this in rust
- expansion of terms grows hyper-exponentially with each timestep,
  so we need to be able to optimize expression expansion
- this can be reused when simulating the R110 cellular automaton step by step
  in a way that follows the source it was compiled from
*/

fn to_internal_expr_mapping(
    expansion_mapping: HashMap<CellState, PyExpression>
) -> HashMap<CellState, Expression> {
    expansion_mapping
        .into_iter()
        .map(|(key, expr)| {
            // Transform the value to the new type
            let inner_expr = expr.expression;
            (key, inner_expr)
        }).collect()
}

impl Term {
    fn to_py_term(&self) -> A {
        A::from_term(&self.clone())
    }
}
impl Product {
    fn to_py_product(&self) -> PyProduct {
        PyProduct {
            product: self.clone(),
        }
    }
}
impl Expression {
    fn to_py_expr(&self) -> PyExpression {
        PyExpression::new(self.clone())
    }
}

trait ToPyExpression {
    fn to_pyexpr(&self) -> PyExpression;
}
impl ToPyExpression for Expression {
    fn to_pyexpr(&self) -> PyExpression {
        PyExpression::new(self.clone())
    }
}
impl ToPyExpression for Product {
    fn to_pyexpr(&self) -> PyExpression {
        PyExpression::new(self.to_expression())
    }
}
impl ToPyExpression for Term {
    fn to_pyexpr(&self) -> PyExpression {
        PyExpression::new(self.to_expression())
    }
}

pub fn py_hash<T: Hash>(value: &T) -> PyResult<isize> {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    let h = hasher.finish();
    // Keep conversion explicit/safe across platforms.
    isize::try_from(h as i64).map_err(|_| PyValueError::new_err("hash overflow"))
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug)]
pub struct PyDebugInfo {
    debug_info: ExprDebugInfo,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyDebugInfo {
    pub fn get_expr_product_idx(&self) -> PyResult<Option<u64>> {
        Ok(self.debug_info.position_info.as_ref().map(
            |info| info.product_idx
        ))
    }
    pub fn get_expr_term_idx(&self) -> PyResult<Option<u64>> {
        Ok(self.debug_info.position_info.as_ref().map(
            |info| info.term_idx
        ))
    }

    pub fn get_parent_expr_product_idx(&self) -> PyResult<Option<u64>> {
        Ok(self.debug_info.parent_position_info.as_ref().map(
            |info| info.product_idx
        ))
    }
    pub fn get_parent_expr_term_idx(&self) -> PyResult<Option<u64>> {
        Ok(self.debug_info.parent_position_info.as_ref().map(
            |info| info.term_idx
        ))
    }
}


#[gen_stub_pyclass]
#[pyclass]
#[derive(struct_macro_eq::CustomEq, Clone, Debug, Hash)]
#[ignore_regex="^_"]
pub struct A {
    term: Term
}
impl A {
    pub fn copy(&self) -> A {
        A { term: self.term.clone() }
    }
    pub fn from_term(term: &Term) -> A {
        A { term: term.clone() }
    }
    pub fn _assign_expr_position(
        &mut self, product_idx: u64, term_idx: u64
    ) {
        self.term._assign_expr_position(
            product_idx, term_idx
        );
    }
    pub fn _to_product(&self) -> PyProduct {
        PyProduct::new(vec![self.copy()])
    }
    pub fn _get_name() -> String {
        const TERM_EXAMPLE: A = A {term: Term {
            position: 0, state: 0, _optimized: false,
            _debug_info: ExprDebugInfo {
                position_info: None,
                parent_position_info: None,
                expansion_index: 0
            },
        }};
        clip_after_space(format!("{:?}", TERM_EXAMPLE))
    }
    pub fn get_term(&self) -> &Term {
        &self.term
    }
}
impl PartialEq<A> for &A {
    fn eq(&self, other: &A) -> bool {
        self.term.position == other.term.position &&
            self.term.state == other.term.state
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl A {
    #[new]
    #[pyo3(signature = (position, state, optimized=false))]
    pub fn new(position: i64, state: CellState, optimized: bool) -> Self {
        A { term: Term::new(position, state, optimized) }
    }
    fn __hash__(&self) -> isize {
        let mut hasher = DefaultHasher::new();
        self.term.hash(&mut hasher);
        hasher.finish() as isize
    }
    pub fn to_py_product(&self) -> PyResult<PyProduct> {
        Ok(self._to_product())
    }
    pub fn to_py_expression(&self) -> PyResult<PyExpression> {
        Ok(PyExpression::from(
            self.term.to_expression()
        ))
    }
    pub fn get_expr_positions(&self) -> PyResult<Option<(u64, u64)>> {
        if let Some(
            position_info
        ) = &self.term._debug_info.position_info {
            Ok(Some((position_info.product_idx, position_info.term_idx)))
        } else {
            Ok(None)
        }
    }
    pub fn get_parent_expr_positions(&self) -> PyResult<Option<(u64, u64)>> {
        if let Some(
            parent_position_info
        ) = &self.term._debug_info.parent_position_info {
            Ok(Some((parent_position_info.product_idx, parent_position_info.term_idx)))
        } else {
            Ok(None)
        }
    }
    pub fn get_expansion_index(&self) -> PyResult<u16> {
        Ok(self.term._debug_info.expansion_index
        )
    }
    pub fn load_debug_info(&self) -> PyResult<PyDebugInfo> {
        Ok(PyDebugInfo {
            debug_info: self.term._debug_info.clone()
        })
    }
    fn __or__(&self, other: &Bound<PyAny>) -> PyResult<PyExpression> {
        if let Ok(other_term) = other.extract::<A>() {
            Ok(PyExpression::from(self.term.copy() | other_term.term))
        } else if let Ok(other_product) = other.extract::<PyProduct>() {
            Ok(PyExpression::from(self.term.copy() | other_product.product))
        } else if let Ok(other_expression) = other.extract::<PyExpression>() {
            Ok(PyExpression::from(self.term.copy() | other_expression.expression))
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }
    fn __mul__(&self, other: &Bound<PyAny>) -> PyResult<PyExpression> {
        if let Ok(other_term) = other.extract::<A>() {
            Ok(PyExpression::from(
                (self.term.copy() * other_term.term).to_expression()
            ))
        } else if let Ok(other_product) = other.extract::<PyProduct>() {
            Ok(PyExpression::from(
                (self.term.copy() * other_product.product).to_expression()
            ))
        } else if let Ok(other_expression) = other.extract::<PyExpression>() {
            Ok(PyExpression::from(
                self.term.copy() * other_expression.expression
            ))
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }
    fn __eq__(&self, other: &Bound<PyAny>) -> PyResult<bool> {
        if let Ok(other_term) = other.extract::<A>() {
            Ok(self == other_term)
        } else if let Ok(other_product) = other.extract::<PyProduct>() {
            if other_product._get_num_terms() > 1 { return Ok(false) }
            let term = match other_product._get_term(0) {
                Some(term) => term,
                None => return Ok(false)
            };
            Ok(self.term == *term)
        } else if let Ok(other_expression) = other.extract::<PyExpression>() {
            if other_expression._get_num_products() > 1 { return Ok(false) }
            let product = match other_expression._get_product(0) {
                Some(product) => product,
                None => return Ok(false)
            };
            let term = match product._get_term(0) {
                Some(term) => term,
                None => return Ok(false)
            };
            Ok(self.term == *term)
        } else {
            Ok(false)
        }
    }
    fn __repr__(&self) -> PyResult<String> {
        Ok(self.term._to_string(&A::_get_name()))
    }
    fn to_py_string(&self, name: String) -> PyResult<String> {
        Ok(self.term._to_string(&name))
    }
    fn get_position(&self) -> PyResult<i64> {
        Ok(self.term.position)
    }
    fn get_state(&self) -> PyResult<CellState> {
        Ok(self.term.state)
    }
    fn sub(&self, substitutions: HashMap<i64, CellState>, default: CellState) -> PyResult<bool> {
        Ok(self.term._sub(&substitutions, default))
    }
    #[pyo3(signature=(expansion_mapping, debug=true, fold=false))]
    fn expand(
        &self, expansion_mapping: HashMap<CellState, PyExpression>,
        debug: bool, fold: bool
    ) -> PyResult<PyExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        Ok(PyExpression::from(
            self.term._expand(&internal_expr_mapping, debug, fold))
        )
    }
    #[pyo3(signature=(expansion_mapping, steps, debug=true, fold=false))]
    fn expand_steps(
        &self, expansion_mapping: HashMap<CellState, PyExpression>,
        steps: u64, debug: bool, fold: bool
    ) -> PyResult<PyExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        let expansion = self.term._expand_steps(
            &internal_expr_mapping, steps, debug, fold
        );
        // validate_debug_info_exists(&expansion);
        Ok(PyExpression::from(expansion))
    }
    pub fn validate_debug_info(&self) {
        assert!(
            self.term._debug_info.position_info.is_some(),
            "Debug info should exist for term"
        );
        validate_debug_info_exists(&self.term.to_expression());
    }
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(struct_macro_eq::CustomEq, Clone, Debug, Hash)]
#[ignore_regex="^_"]
pub struct PyProduct {
    pub product: Product
}
impl PyProduct {
    pub fn new(terms: Vec<A>) -> Self {
        let mut terms_set = HashSet::new();
        let mut rust_terms = Vec::new();

        for term in terms.iter() {
            terms_set.insert(term.term.copy());
            rust_terms.push(term.term.copy());
        }

        let product = ProductFactory::new(rust_terms.clone()).to_product();
        PyProduct::from_product(product)
    }
    fn _get_term(&self, index: usize) -> Option<&Term> {
        self.product._terms.get(index)
    }
    fn _get_num_terms(&self) -> usize {
        self.product._get_num_terms()
    }
    pub(crate) fn from_product(product: Product) -> Self {
        PyProduct { product }
    }
}
impl PartialEq<PyProduct> for &PyProduct {
    fn eq(&self, other: &PyProduct) -> bool {
        self.product == other.product
    }
}

impl PartialEq<Term> for &Term {
    fn eq(&self, other: &Term) -> bool {
        self.position == other.position &&
            self.state == other.state
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyProduct {
    #[new]
    #[pyo3(signature=(terms, annotation=""))]
    pub fn init(terms: Vec<A>, annotation: &str) -> PyResult<PyProduct> {
        if terms.len() == 0 {
            return Err(PyValueError::new_err("terms cannot be empty"));
        }
        let rs_terms: Vec<Term> =
            terms.into_iter().map(|term| term.term).collect();
        let product = ProductFactory::new(rs_terms)
            .with_annotation(annotation.to_string())
            .to_product();
        Ok(PyProduct { product })
    }
    pub fn to_py_product(&self) -> PyResult<PyProduct> {
        Ok(Self::from_product(self.product.copy()))
    }
    pub fn to_py_expression(&self) -> PyResult<PyExpression> {
        Ok(PyExpression::from(
            self.product.to_expression()
        ))
    }
    pub fn get_annotation(&self) -> PyResult<String> {
        Ok(self.product._annotation.clone())
    }
    fn __hash__(&self) -> isize {
        let mut hasher = DefaultHasher::new();
        self.product.hash(&mut hasher);
        hasher.finish() as isize
    }
    fn __or__(&self, other: &Bound<PyAny>) -> PyResult<PyExpression> {
        if let Ok(other_term) = other.extract::<A>() {
            Ok((self.product.copy() | other_term.term).to_pyexpr())
        } else if let Ok(other_product) = other.extract::<PyProduct>() {
            Ok((self.product.copy() | other_product.product).to_pyexpr())
        } else if let Ok(other_expression) = other.extract::<PyExpression>() {
            Ok((self.product.copy() | other_expression.expression).to_pyexpr())
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }
    fn __mul__(&self, other: &Bound<PyAny>) -> PyResult<PyExpression> {
        if let Ok(other_term) = other.extract::<A>() {
            Ok((self.product.copy() * other_term.term).to_pyexpr())
        } else if let Ok(other_product) = other.extract::<PyProduct>() {
            Ok((self.product.copy() * other_product.product).to_pyexpr())
        } else if let Ok(other_expression) = other.extract::<PyExpression>() {
            Ok((self.product.copy() * other_expression.expression).to_pyexpr())
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }
    fn multiply_by_term(&self, term: A) -> PyResult<PyProduct> {
        Ok(Self::from_product(self.product.copy() * term.term))
    }
    fn multiply_by_product(&self, product: PyProduct) -> PyResult<PyProduct> {
        Ok(Self::from_product(self.product.copy() * product.product))
    }
    fn make_copy(&self) -> PyResult<PyProduct> {
        Ok(Self::from_product(self.product.copy()))
    }
    fn __eq__(&self, other: &Bound<PyAny>) -> PyResult<bool> {
        if let Ok(other_term) = other.extract::<A>() {
            if self.product._terms.len() > 1 { return Ok(false) }
            let term = match self.product._terms.get(0) {
                Some(term) => term,
                None => return Ok(false)
            };
            Ok(term == other_term.term)
        } else if let Ok(other_product) = other.extract::<PyProduct>() {
            Ok(self == other_product)
        } else if let Ok(other_expression) = other.extract::<PyExpression>() {
            if other_expression._get_num_products() > 1 { return Ok(false) }
            let product = match other_expression._get_product(0) {
                Some(product) => product,
                None => return Ok(false)
            };
            Ok(self.product == *product)
        } else {
            Ok(false)
        }
    }
    fn __repr__(&self) -> PyResult<String> {
        Ok(self.product._to_string(&A::_get_name()))
    }
    fn __len__(&self) -> PyResult<usize> {
        Ok(self._get_num_terms())
    }
    fn __getitem__(&self, index: usize) -> PyResult<A> {
        let term_option = self.product._terms.get(index);
        match term_option {
            Some(term) => {
                // assert!(term._debug_info.position_info.is_some());
                Ok(term.to_py_term())
            }
            None => { Err(PyIndexError::new_err("Index out of range")) }
        }
    }
    fn to_py_string(&self, name: String) -> PyResult<String> {
        Ok(self.product._to_string(&name))
    }
    fn sub(&self, substitutions: HashMap<i64, CellState>, default: CellState) -> PyResult<bool> {
        Ok(self.product._sub(&substitutions, default))
    }
    #[pyo3(signature=(expansion_mapping, debug=true, fold=false))]
    fn expand(
        &self, expansion_mapping: HashMap<CellState, PyExpression>,
        debug: bool, fold: bool
    ) -> PyResult<PyExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        Ok(self.product._expand(
            &internal_expr_mapping, debug, fold
        ).to_pyexpr())
    }
    #[pyo3(signature=(expansion_mapping, steps, debug=true, fold=false))]
    fn expand_steps(
        &self, expansion_mapping: HashMap<CellState, PyExpression>,
        steps: u64, debug: bool, fold: bool
    ) -> PyResult<PyExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        Ok(self.product._expand_steps(
            &internal_expr_mapping, steps, debug, fold
        ).to_pyexpr())
    }
    fn get_num_terms(&self) -> PyResult<usize> {
        Ok(self._get_num_terms())
    }
    pub fn validate_debug_info(&self) {
        for term in &self.product._terms {
            assert!(
                term._debug_info.position_info.is_some(),
                "Term debug info is missing for term: {:?}", term
            );
        }
    }
    pub fn to_flat_terms(&self) -> PyResult<Vec<A>> {
        let terms = self.product.to_flat_terms();
        let mut flat_py_terms = Vec::new();
        for term in terms {
            flat_py_terms.push(term.to_py_term());
        }
        Ok(flat_py_terms)
    }
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(struct_macro_eq::CustomEq, Clone, Debug, Hash)]
#[ignore_regex="^_"]
pub struct PyExpression {
    pub expression: Expression
}
impl PyExpression {
    pub fn new(expression: Expression) -> Self {
        PyExpression { expression }
    }
    pub fn _get_num_products(&self) -> usize {
        self.expression.products.len()
    }
    pub fn _get_product(&self, index: usize) -> Option<&Product> {
        self.expression.products.get(index)
    }
    pub fn from(expression: Expression) -> Self {
        PyExpression::new(expression)
    }
}
impl PartialEq<PyExpression> for &PyExpression {
    fn eq(&self, other: &PyExpression) -> bool {
        self.expression == other.expression
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl PyExpression {
    pub fn to_py_product(&self) -> PyResult<PyProduct> {
        if self._get_num_products() != 1 {
            return Err(PyValueError::new_err(
                "Expression does not have exactly one product"
            ));
        }
        Ok(self.expression.products[0].to_py_product())
    }
    pub fn to_py_expression(&self) -> PyResult<PyExpression> {
        Ok(self.expression.to_pyexpr())
    }
    fn __hash__(&self) -> isize {
        let mut hasher = DefaultHasher::new();
        self.expression.hash(&mut hasher);
        hasher.finish() as isize
    }
    fn __or__(&self, other: &Bound<PyAny>) -> PyResult<PyExpression> {
        if let Ok(other_term) = other.extract::<A>() {
            Ok((self.expression.copy() | other_term.term).to_pyexpr())
        } else if let Ok(other_product) = other.extract::<PyProduct>() {
            Ok((self.expression.copy() | other_product.product).to_pyexpr())
        } else if let Ok(other_expression) = other.extract::<PyExpression>() {
            Ok((self.expression.copy() | other_expression.expression).to_pyexpr())
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }
    fn __mul__(&self, other: &Bound<PyAny>) -> PyResult<PyExpression> {
        if let Ok(other_term) = other.extract::<A>() {
            Ok((self.expression.copy() * other_term.term).to_pyexpr())
        } else if let Ok(other_product) = other.extract::<PyProduct>() {
            Ok((self.expression.copy() * other_product.product).to_pyexpr())
        } else if let Ok(other_expression) = other.extract::<PyExpression>() {
            Ok((self.expression.copy() * other_expression.expression).to_pyexpr())
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }
    fn __eq__(&self, other: &Bound<PyAny>) -> PyResult<bool> {
        if let Ok(other_term) = other.extract::<A>() {
            if self._get_num_products() > 1 { return Ok(false) }
            let product = match self._get_product(0) {
                Some(product) => product,
                None => return Ok(false)
            };
            if product._get_num_terms() > 1 { return Ok(false) }
            let term = match product._terms.get(0) {
                Some(term) => term,
                None => return Ok(false)
            };
            Ok(term == other_term.term)
        } else if let Ok(other_product) = other.extract::<PyProduct>() {
            if self._get_num_products() > 1 { return Ok(false) }
            let product = match self._get_product(0) {
                Some(product) => product,
                None => return Ok(false)
            };
            Ok(product == other_product.product)
        } else if let Ok(other_expression) = other.extract::<PyExpression>() {
            Ok(self == other_expression)
        } else {
            Ok(false)
        }
    }
    fn __repr__(&self) -> PyResult<String> {
        Ok(self.expression._to_string(&A::_get_name()))
    }
    fn __len__(&self) -> PyResult<usize> {
        Ok(self._get_num_products())
    }
    fn __getitem__(&self, index: usize) -> PyResult<PyProduct> {
        let product_option = self.expression.products.get(index);
        match product_option {
            Some(product) => { Ok(product.to_py_product()) }
            None => { Err(PyIndexError::new_err("Index out of range")) }
        }
    }
    fn to_py_string(&self, name: String) -> PyResult<String> {
        Ok(self.expression._to_string(&name))
    }
    fn sub(&self, substitutions: HashMap<i64, CellState>, default: CellState) -> PyResult<bool> {
        Ok(self.expression._sub(&substitutions, default))
    }
    #[pyo3(signature=(expansion_mapping, debug=true, fold=false))]
    fn expand(
        &self, expansion_mapping: HashMap<CellState, PyExpression>, debug: bool,
        fold: bool
    ) -> PyResult<PyExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        let expanded = self.expression._expand(
            &internal_expr_mapping, debug, fold
        );
        Ok(expanded.to_py_expr())
    }
    #[pyo3(signature=(expansion_mapping, steps, debug=true, fold=false))]
    fn expand_steps(
        &self, expansion_mapping: HashMap<CellState, PyExpression>,
        steps: u64, debug: bool, fold: bool
    ) -> PyResult<PyExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        let expanded = self.expression._expand_steps(
            &internal_expr_mapping, steps, debug, fold
        );
        Ok(expanded.to_pyexpr())
    }
    fn get_product(&self, index: usize) -> PyResult<PyProduct> {
        Ok(self.expression.products.get(index).unwrap().to_py_product())
    }
    fn get_num_terms(&self) -> PyResult<usize> {
        Ok(self.expression._get_num_terms())
    }
    fn get_flat_terms(&self) -> PyResult<Vec<A>> {
        let mut terms = Vec::new();
        for product in &self.expression.products {
            for term in &product._terms {
                terms.push(term.to_py_term());
            }
        }
        Ok(terms)
    }
    pub fn get_flat_products(&self) -> PyResult<Vec<PyProduct>> {
        let mut products = Vec::new();
        for product in &self.expression.products {
            products.push(product.to_py_product());
        }
        Ok(products)
    }
    fn pad_products(&self, length: usize) -> PyResult<PyExpression> {
        let new_expr = self.expression.pad_products(length);
        match new_expr {
            Some(new_expr) => { Ok(new_expr.to_py_expr()) }
            None => { Err(PyValueError::new_err("Failed to pad products")) }
        }
    }
    pub fn validate_debug_info(&self) {
        validate_debug_info_exists(&self.expression);
    }
}

define_stub_info_gatherer!(stub_info);

