use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use pyo3::{pyclass, pymethods, PyResult};
use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyType};
use pyo3_stub_gen::define_stub_info_gatherer;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use crate::automata::py_terms::py_hash;
use crate::automata::terms::{clip_after_space, CellState, ExprDebugInfo};
use crate::automata::terms_multitape::{
    validate_debug_info_exists, AbstractMultiTapeExpression,
    MultiTapeCellState, MultiTapeExpression, MultiTapeProduct,
    MultiTapeProductFactory, MultiTapeTerm, TapeNo
};
/*
Reasons to redo this in rust
- expansion of terms grows hyper-exponentially with each timestep,
  so we need to be able to optimize expression expansion
- this can be reused when simulating the R110 cellular automaton step by step
  in a way that follows the source it was compiled from
*/

fn to_internal_expr_mapping(
    expansion_mapping: HashMap<MultiTapeCellState, PyMultiTapeExpression>
) -> HashMap<MultiTapeCellState, MultiTapeExpression> {
    expansion_mapping
        .into_iter()
        .map(|(key, expr)| {
            // Transform the value to the new type
            let inner_expr = expr.expression;
            (key, inner_expr)
        }).collect()
}

impl MultiTapeTerm {
    fn to_py_term(&self) -> D {
        D::from_term(self.clone())
    }
}
impl MultiTapeProduct {
    fn to_py_product(&self) -> PyMultiTapeProduct {
        PyMultiTapeProduct {
            product: self.clone(),
        }
    }
}
impl MultiTapeExpression {
    fn to_py_expr(&self) -> PyMultiTapeExpression {
        PyMultiTapeExpression::new(self.clone())
    }
}

trait ToPyMultiTapeExpression {
    fn to_pyexpr(&self) -> PyMultiTapeExpression;
}
impl ToPyMultiTapeExpression for MultiTapeExpression {
    fn to_pyexpr(&self) -> PyMultiTapeExpression {
        PyMultiTapeExpression::new(self.clone())
    }
}
impl ToPyMultiTapeExpression for MultiTapeProduct {
    fn to_pyexpr(&self) -> PyMultiTapeExpression {
        PyMultiTapeExpression::new(self.to_expression())
    }
}
impl ToPyMultiTapeExpression for MultiTapeTerm {
    fn to_pyexpr(&self) -> PyMultiTapeExpression {
        PyMultiTapeExpression::new(self.to_expression())
    }
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
pub struct D {
    term: MultiTapeTerm
}
impl D {
    pub fn copy(&self) -> D {
        D { term: self.term.clone() }
    }
    pub fn from_term(term: MultiTapeTerm) -> D {
        D { term }
    }
    pub fn _assign_expr_position(
        &mut self, product_idx: u64, term_idx: u64
    ) {
        self.term._assign_expr_position(
            product_idx, term_idx
        );
    }
    pub fn _to_product(&self) -> PyMultiTapeProduct {
        PyMultiTapeProduct::new(vec![self.copy()])
    }
    pub fn _get_name() -> String {
        const TERM_EXAMPLE: D = D {term: MultiTapeTerm {
            position: 0, state: (0, 0), _optimized: false,
            _debug_info: ExprDebugInfo {
                position_info: None,
                parent_position_info: None,
                expansion_index: 0
            },
        }};
        clip_after_space(format!("{:?}", TERM_EXAMPLE))
    }
    pub fn get_term(&self) -> &MultiTapeTerm {
        &self.term
    }
}
impl PartialEq<D> for &D {
    fn eq(&self, other: &D) -> bool {
        self.term.position == other.term.position &&
            self.term.state == other.term.state
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl D {
    #[new]
    #[pyo3(signature = (position, tape_no, state, optimized=false))]
    pub fn new(
        position: i64, tape_no: TapeNo, state: CellState,
        optimized: bool
    ) -> Self {
        let tape_and_cell_state = (tape_no, state);
        D { term: MultiTapeTerm::new(position, tape_and_cell_state, optimized) }
    }
    fn __hash__(&self) -> PyResult<isize> {
        py_hash(&self.term)
    }
    pub fn __deepcopy__(&self, _memo: &Bound<PyDict>) -> Self {
        self.clone()
    }
    pub fn to_py_product(&self) -> PyResult<PyMultiTapeProduct> {
        Ok(self._to_product())
    }
    pub fn to_py_expression(&self) -> PyResult<PyMultiTapeExpression> {
        Ok(PyMultiTapeExpression::from(
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
    fn __or__(&self, other: &Bound<PyAny>) -> PyResult<PyMultiTapeExpression> {
        if let Ok(other_term) = other.extract::<D>() {
            Ok(PyMultiTapeExpression::from(self.term.copy() | other_term.term))
        } else if let Ok(other_product) = other.extract::<PyMultiTapeProduct>() {
            Ok(PyMultiTapeExpression::from(self.term.copy() | other_product.product))
        } else if let Ok(other_expression) = other.extract::<PyMultiTapeExpression>() {
            Ok(PyMultiTapeExpression::from(self.term.copy() | other_expression.expression))
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }
    fn __mul__(&self, other: &Bound<PyAny>) -> PyResult<PyMultiTapeExpression> {
        if let Ok(other_term) = other.extract::<D>() {
            Ok(PyMultiTapeExpression::from(
                (self.term.copy() * other_term.term).to_expression()
            ))
        } else if let Ok(other_product) = other.extract::<PyMultiTapeProduct>() {
            Ok(PyMultiTapeExpression::from(
                (self.term.copy() * other_product.product).to_expression()
            ))
        } else if let Ok(other_expression) = other.extract::<PyMultiTapeExpression>() {
            Ok(PyMultiTapeExpression::from(
                self.term.copy() * other_expression.expression
            ))
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }

    fn __lt__(&self, other: &Bound<PyAny>) -> PyResult<bool> {
        if let Ok(other_product) = other.extract::<D>() {
            Ok(self.term < other_product.term)
        } else {
            Ok(false)
        }
    }

    fn __eq__(&self, other: &Bound<PyAny>) -> PyResult<bool> {
        if let Ok(other_term) = other.extract::<D>() {
            Ok(self == other_term)
        } else if let Ok(other_product) = other.extract::<PyMultiTapeProduct>() {
            if other_product._get_num_terms() > 1 { return Ok(false) }
            let term = match other_product._get_term(0) {
                Some(term) => term,
                None => return Ok(false)
            };
            Ok(self.term == *term)
        } else if let Ok(other_expression) = other.extract::<PyMultiTapeExpression>() {
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
        Ok(self.term._to_string(&D::_get_name()))
    }
    fn to_py_string(&self, name: String) -> PyResult<String> {
        Ok(self.term._to_string(&name))
    }
    fn get_position(&self) -> PyResult<i64> {
        Ok(self.term.position)
    }
    fn get_state(&self) -> PyResult<MultiTapeCellState> {
        Ok(self.term.state)
    }
    fn get_tape_no(&self) -> PyResult<TapeNo> {
        Ok(self.term.state.0)
    }
    fn get_cell_state(&self) -> PyResult<CellState> {
        Ok(self.term.state.1)
    }
    fn sub(&self, substitutions: HashMap<i64, MultiTapeCellState>, default: MultiTapeCellState) -> PyResult<bool> {
        Ok(self.term._sub(&substitutions, default))
    }
    #[pyo3(signature=(expansion_mapping, debug=true, fold=false))]
    fn expand(
        &self, expansion_mapping: HashMap<MultiTapeCellState, PyMultiTapeExpression>,
        debug: bool, fold: bool
    ) -> PyResult<PyMultiTapeExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        Ok(PyMultiTapeExpression::from(
            self.term._expand(&internal_expr_mapping, debug, fold))
        )
    }
    #[pyo3(signature=(expansion_mapping, steps, debug=true, fold=false))]
    fn expand_steps(
        &self, expansion_mapping: HashMap<MultiTapeCellState, PyMultiTapeExpression>,
        steps: u64, debug: bool, fold: bool
    ) -> PyResult<PyMultiTapeExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        let expansion = self.term._expand_steps(
            &internal_expr_mapping, steps, debug, fold
        );
        // validate_debug_info_exists(&expansion);
        Ok(PyMultiTapeExpression::from(expansion))
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
pub struct PyMultiTapeProduct {
    pub product: MultiTapeProduct
}
impl PyMultiTapeProduct {
    pub fn new(terms: Vec<D>) -> Self {
        let mut terms_set = HashSet::new();
        let mut rust_terms = Vec::new();

        for term in terms.iter() {
            terms_set.insert(term.term.copy());
            rust_terms.push(term.term.copy());
        }
        let product = MultiTapeProductFactory::new(rust_terms.clone()).to_product();
        Self::new_from_product(product)
    }
    pub fn new_from_product(product: MultiTapeProduct) -> Self {
        Self { product }
    }
    fn _get_term(&self, index: usize) -> Option<&MultiTapeTerm> {
        self.product._terms.get(index)
    }
    fn _get_num_terms(&self) -> usize {
        self.product._get_num_terms()
    }
    pub(crate) fn from_product(product: MultiTapeProduct) -> Self {
        PyMultiTapeProduct { product }
    }
}
impl PartialEq<PyMultiTapeProduct> for &PyMultiTapeProduct {
    fn eq(&self, other: &PyMultiTapeProduct) -> bool {
        self.product == other.product
    }
}

impl PartialEq<MultiTapeTerm> for &MultiTapeTerm {
    fn eq(&self, other: &MultiTapeTerm) -> bool {
        self.position == other.position &&
            self.state == other.state
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMultiTapeProduct {
    #[new]
    #[pyo3(signature = (terms, annotation = ""))]
    pub fn init(
        terms: Vec<D>, annotation: &str
    ) -> PyResult<PyMultiTapeProduct> {
        if terms.len() == 0 {
            return Err(PyValueError::new_err("terms cannot be empty"));
        }
        let rs_terms: Vec<MultiTapeTerm> =
            terms.into_iter().map(|term| term.term).collect();
        let product = MultiTapeProductFactory::new(rs_terms)
            .with_annotation(annotation.parse()?)
            .to_product();
        Ok(PyMultiTapeProduct { product })
    }
    #[classmethod]
    pub fn merge(
        _cls: &Bound<'_, PyType>, products: Vec<PyRef<PyMultiTapeProduct>>
    ) -> Self {
        let rs_products = products.into_iter().map(|product_ref| {
            return product_ref.product.clone();
        }).collect();

        PyMultiTapeProduct::new_from_product(
            MultiTapeProduct::merge_products(rs_products)
        )
    }

    // TODO: expose method to merge multi products
    pub fn get_annotation(&self) -> PyResult<String> {
        Ok(self.product._annotation.clone())
    }
    pub fn to_py_product(&self) -> PyResult<PyMultiTapeProduct> {
        Ok(Self::from_product(self.product.copy()))
    }
    pub fn to_py_expression(&self) -> PyResult<PyMultiTapeExpression> {
        Ok(PyMultiTapeExpression::from(
            self.product.to_expression()
        ))
    }
    fn __hash__(&self) -> PyResult<isize> {
        py_hash(&self.product)
    }
    pub fn __deepcopy__(&self, _memo: &Bound<PyDict>) -> Self {
        self.clone()
    }
    fn __or__(&self, other: &Bound<PyAny>) -> PyResult<PyMultiTapeExpression> {
        if let Ok(other_term) = other.extract::<D>() {
            Ok((self.product.copy() | other_term.term).to_pyexpr())
        } else if let Ok(other_product) = other.extract::<PyMultiTapeProduct>() {
            Ok((self.product.copy() | other_product.product).to_pyexpr())
        } else if let Ok(other_expression) = other.extract::<PyMultiTapeExpression>() {
            Ok((self.product.copy() | other_expression.expression).to_pyexpr())
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }
    fn __mul__(&self, other: &Bound<PyAny>) -> PyResult<PyMultiTapeExpression> {
        if let Ok(other_term) = other.extract::<D>() {
            Ok((self.product.copy() * other_term.term).to_pyexpr())
        } else if let Ok(other_product) = other.extract::<PyMultiTapeProduct>() {
            Ok((self.product.copy() * other_product.product).to_pyexpr())
        } else if let Ok(other_expression) = other.extract::<PyMultiTapeExpression>() {
            Ok((self.product.copy() * other_expression.expression).to_pyexpr())
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }

    fn __lt__(&self, other: &Bound<PyAny>) -> PyResult<bool> {
        if let Ok(other_product) = other.extract::<PyMultiTapeProduct>() {
            Ok(self.product < other_product.product)
        } else {
            Ok(false)
        }
    }

    fn multiply_by_term(&self, term: D) -> PyResult<PyMultiTapeProduct> {
        Ok(Self::from_product(self.product.copy() * term.term))
    }
    fn multiply_by_product(&self, product: PyMultiTapeProduct) -> PyResult<PyMultiTapeProduct> {
        Ok(Self::from_product(self.product.copy() * product.product))
    }
    fn make_copy(&self) -> PyResult<PyMultiTapeProduct> {
        Ok(Self::from_product(self.product.copy()))
    }
    fn __eq__(&self, other: &Bound<PyAny>) -> PyResult<bool> {
        if let Ok(other_term) = other.extract::<D>() {
            if self.product._terms.len() > 1 { return Ok(false) }
            let term = match self.product._terms.get(0) {
                Some(term) => term,
                None => return Ok(false)
            };
            Ok(term == other_term.term)
        } else if let Ok(other_product) = other.extract::<PyMultiTapeProduct>() {
            Ok(self == other_product)
        } else if let Ok(other_expression) = other.extract::<PyMultiTapeExpression>() {
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
        Ok(self.product._to_string(&D::_get_name()))
    }
    fn __len__(&self) -> PyResult<usize> {
        Ok(self._get_num_terms())
    }
    fn __getitem__(&self, index: usize) -> PyResult<D> {
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
    fn sub(&self, substitutions: HashMap<i64, MultiTapeCellState>, default: MultiTapeCellState) -> PyResult<bool> {
        Ok(self.product._sub(&substitutions, default))
    }
    #[pyo3(signature=(expansion_mapping, debug=true, fold=false))]
    fn expand(
        &self, expansion_mapping: HashMap<MultiTapeCellState, PyMultiTapeExpression>,
        debug: bool, fold: bool
    ) -> PyResult<PyMultiTapeExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        Ok(self.product._expand(
            &internal_expr_mapping, debug, fold
        ).to_pyexpr())
    }
    #[pyo3(signature=(expansion_mapping, steps, debug=true, fold=false))]
    fn expand_steps(
        &self, expansion_mapping: HashMap<MultiTapeCellState, PyMultiTapeExpression>,
        steps: u64, debug: bool, fold: bool
    ) -> PyResult<PyMultiTapeExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        Ok(self.product._expand_steps(
            &internal_expr_mapping, steps, debug, fold
        ).to_pyexpr())
    }
    fn get_num_terms(&self) -> PyResult<usize> {
        Ok(self._get_num_terms())
    }
    fn get_flat_terms(&self) -> PyResult<Vec<D>> {
        let terms = self.product._terms.iter().map(
            |term| term.to_py_term()
        ).collect();
        Ok(terms)
    }
    pub fn validate_debug_info(&self) {
        for term in &self.product._terms {
            assert!(
                term._debug_info.position_info.is_some(),
                "MultiTapeTerm debug info is missing for term: {:?}", term
            );
        }
    }
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(struct_macro_eq::CustomEq, Clone, Debug, Hash)]
#[ignore_regex="^_"]
pub struct PyMultiTapeExpression {
    pub expression: MultiTapeExpression
}
impl PyMultiTapeExpression {
    pub fn new(expression: MultiTapeExpression) -> Self {
        PyMultiTapeExpression { expression }
    }
    pub fn _get_num_products(&self) -> usize {
        self.expression.products.len()
    }
    pub fn _get_product(&self, index: usize) -> Option<&MultiTapeProduct> {
        self.expression.products.get(index)
    }
    pub fn from(expression: MultiTapeExpression) -> Self {
        PyMultiTapeExpression::new(expression)
    }
}
impl PartialEq<PyMultiTapeExpression> for &PyMultiTapeExpression {
    fn eq(&self, other: &PyMultiTapeExpression) -> bool {
        self.expression == other.expression
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl PyMultiTapeExpression {
    pub fn to_py_product(&self) -> PyResult<PyMultiTapeProduct> {
        if self._get_num_products() != 1 {
            return Err(PyValueError::new_err(
                "MultiTapeExpression does not have exactly one product"
            ));
        }
        Ok(self.expression.products[0].to_py_product())
    }
    pub fn to_py_expression(&self) -> PyResult<PyMultiTapeExpression> {
        Ok(self.expression.to_pyexpr())
    }
    fn __hash__(&self) -> PyResult<isize> {
        py_hash(&self.expression)
    }
    pub fn __deepcopy__(&self, _memo: &Bound<PyDict>) -> Self {
        self.clone()
    }
    fn __or__(&self, other: &Bound<PyAny>) -> PyResult<PyMultiTapeExpression> {
        if let Ok(other_term) = other.extract::<D>() {
            Ok((self.expression.copy() | other_term.term).to_pyexpr())
        } else if let Ok(other_product) = other.extract::<PyMultiTapeProduct>() {
            Ok((self.expression.copy() | other_product.product).to_pyexpr())
        } else if let Ok(other_expression) = other.extract::<PyMultiTapeExpression>() {
            Ok((self.expression.copy() | other_expression.expression).to_pyexpr())
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }
    fn __mul__(&self, other: &Bound<PyAny>) -> PyResult<PyMultiTapeExpression> {
        if let Ok(other_term) = other.extract::<D>() {
            Ok((self.expression.copy() * other_term.term).to_pyexpr())
        } else if let Ok(other_product) = other.extract::<PyMultiTapeProduct>() {
            Ok((self.expression.copy() * other_product.product).to_pyexpr())
        } else if let Ok(other_expression) = other.extract::<PyMultiTapeExpression>() {
            Ok((self.expression.copy() * other_expression.expression).to_pyexpr())
        } else {
            Err(PyTypeError::new_err("Unsupported operand type(s)"))
        }
    }
    fn __eq__(&self, other: &Bound<PyAny>) -> PyResult<bool> {
        if let Ok(other_term) = other.extract::<D>() {
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
        } else if let Ok(other_product) = other.extract::<PyMultiTapeProduct>() {
            if self._get_num_products() > 1 { return Ok(false) }
            let product = match self._get_product(0) {
                Some(product) => product,
                None => return Ok(false)
            };
            Ok(product == other_product.product)
        } else if let Ok(other_expression) = other.extract::<PyMultiTapeExpression>() {
            Ok(self == other_expression)
        } else {
            Ok(false)
        }
    }
    fn __repr__(&self) -> PyResult<String> {
        Ok(self.expression._to_string(&D::_get_name()))
    }
    fn __len__(&self) -> PyResult<usize> {
        Ok(self._get_num_products())
    }
    fn __getitem__(&self, index: usize) -> PyResult<PyMultiTapeProduct> {
        let product_option = self.expression.products.get(index);
        match product_option {
            Some(product) => { Ok(product.to_py_product()) }
            None => { Err(PyIndexError::new_err("Index out of range")) }
        }
    }
    fn to_py_string(&self, name: String) -> PyResult<String> {
        Ok(self.expression._to_string(&name))
    }
    fn sub(&self, substitutions: HashMap<i64, MultiTapeCellState>, default: MultiTapeCellState) -> PyResult<bool> {
        Ok(self.expression._sub(&substitutions, default))
    }
    #[pyo3(signature=(expansion_mapping, debug=true, fold=false))]
    fn expand(
        &self, expansion_mapping: HashMap<MultiTapeCellState, PyMultiTapeExpression>, debug: bool,
        fold: bool
    ) -> PyResult<PyMultiTapeExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        let expanded = self.expression._expand(
            &internal_expr_mapping, debug, fold
        );
        Ok(expanded.to_py_expr())
    }
    #[pyo3(signature=(expansion_mapping, steps, debug=true, fold=false))]
    fn expand_steps(
        &self, expansion_mapping: HashMap<MultiTapeCellState, PyMultiTapeExpression>,
        steps: u64, debug: bool, fold: bool
    ) -> PyResult<PyMultiTapeExpression> {
        let internal_expr_mapping = to_internal_expr_mapping(expansion_mapping);
        let expanded = self.expression._expand_steps(
            &internal_expr_mapping, steps, debug, fold
        );
        Ok(expanded.to_pyexpr())
    }
    fn get_product(&self, index: usize) -> PyResult<PyMultiTapeProduct> {
        Ok(self.expression.products.get(index).unwrap().to_py_product())
    }
    fn get_num_terms(&self) -> PyResult<usize> {
        Ok(self.expression._get_num_terms())
    }
    fn get_flat_terms(&self) -> PyResult<Vec<D>> {
        let mut terms = Vec::new();
        for product in &self.expression.products {
            for term in &product._terms {
                terms.push(term.to_py_term());
            }
        }
        Ok(terms)
    }
    fn get_flat_products(&self) -> PyResult<Vec<PyMultiTapeProduct>> {
        let mut products = Vec::new();
        for product in &self.expression.products {
            products.push(product.to_py_product());
        }
        Ok(products)
    }
    fn pad_products(&self, length: usize) -> PyResult<PyMultiTapeExpression> {
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
