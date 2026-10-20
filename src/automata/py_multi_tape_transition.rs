use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use indexmap::{IndexMap, IndexSet};
use pyo3::{pyclass, pymethods, PyResult};
use pyo3::exceptions::PyValueError;
use pyo3_stub_gen::define_stub_info_gatherer;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use crate::automata::multi_tape_transition::{
    MultiTapeTransition, MultiTapeTransitionsGroup, TransitionPriority
};
use crate::automata::py_terms_multitape::{
    PyMultiTapeExpression, PyMultiTapeProduct, D
};
use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{MultiTapeState, MultiTapeTerm, TapeNo};

// PyO3 wrapper classes
#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct PyMultiTapeState {
    inner: MultiTapeState,
}
impl PyMultiTapeState {
    pub fn from(state: MultiTapeState) -> Self {
        PyMultiTapeState { inner: state }
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl PyMultiTapeState {
    #[new]
    pub fn new(tape_no: TapeNo, tape_cell_state: CellState) -> Self {
        Self {
            inner: MultiTapeState::new(tape_no, tape_cell_state),
        }
    }
    #[getter]
    pub fn tape_no(&self) -> TapeNo {
        self.inner.tape_no
    }
    #[getter]
    pub fn tape_cell_state(&self) -> CellState {
        self.inner.tape_cell_state
    }
    pub fn __repr__(&self) -> String {
        format!(
            "MultiTapeState(tape_no={}, tape_cell_state={})",
            self.inner.tape_no, self.inner.tape_cell_state
        )
    }
    pub fn __hash__(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }

    pub fn __eq__(&self, other: &PyMultiTapeState) -> bool {
        self.inner == other.inner
    }
}

#[gen_stub_pyclass]
#[pyclass]
pub struct PyMultiTapeTransition {
    inner: MultiTapeTransition,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyMultiTapeTransition {
    #[new]
    #[pyo3(signature = (
        input_terms, output_state, annotation="", priority=0
    ))]
    pub fn new(
        input_terms: Vec<D>,
        output_state: PyMultiTapeState,
        annotation: &str,
        priority: TransitionPriority,
    ) -> Self {
        let rs_terms: Vec<MultiTapeTerm> = input_terms.iter().map(|py_term| {
            py_term.get_term().clone()
        }).collect::<Vec<MultiTapeTerm>>();

        Self {
            inner: MultiTapeTransition::new(
                rs_terms,
                output_state.inner.clone(),
                annotation.parse().unwrap(),
                priority,
            ),
        }
    }

    #[getter]
    pub fn input_terms(&self) -> Vec<D> {
        let py_terms: Vec<D> = self.inner.input_terms.iter().map(|term| {
            D::from_term(term.clone())
        }).collect();
        py_terms
    }

    #[getter]
    pub fn output_state(&self) -> PyMultiTapeState {
        PyMultiTapeState {
            inner: self.inner.output_state.clone(),
        }
    }

    #[getter]
    pub fn annotation(&self) -> String {
        self.inner.annotation.clone()
    }

    #[getter]
    pub fn priority(&self) -> TransitionPriority {
        self.inner.priority
    }

    pub fn __repr__(&self) -> String {
        format!(
            "MultiTapeTransition(\
                input_terms={}, output_state={}, annotation='{}', priority={}\
            )",
            self.inner.input_terms.len(),
            format!(
                "MultiTapeState(tape_no={}, tape_cell_state={})",
                self.inner.output_state.tape_no,
                self.inner.output_state.tape_cell_state
            ),
            self.inner.annotation,
            self.inner.priority
        )
    }
}

#[gen_stub_pyclass]
#[pyclass]
pub struct PyMultiTapeTransitionsGroup {
    pub inner: MultiTapeTransitionsGroup,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyMultiTapeTransitionsGroup {
    #[new]
    #[pyo3(signature=(require_annotation=false))]
    pub fn new(require_annotation: bool) -> Self {
        Self {
            inner: MultiTapeTransitionsGroup::new(require_annotation),
        }
    }
    pub fn __len__(&self) -> usize {
        self.inner.len()
    }
    pub fn __repr__(&self) -> String {
        format!(
            "MultiTapeTransitionsGroup(transitions={}, require_annotation={})",
            self.inner.transitions.len(),
            self.inner.require_annotation
        )
    }
    #[getter]
    pub fn transitions(&self) -> Vec<PyMultiTapeTransition> {
        self.inner
            .transitions
            .iter()
            .map(|t| PyMultiTapeTransition {
                inner: t.clone(),
            })
            .collect()
    }

    #[getter]
    pub fn require_annotation(&self) -> bool {
        self.inner.require_annotation
    }

    #[pyo3(signature = (
        input_terms, output_tape_no, output_cell_state,
        validate_void=true, validate_halt=true,
        annotation=String::new(), priority=0
    ))]
    pub fn add_transition(
        &mut self,
        input_terms: Vec<D>,
        output_tape_no: TapeNo,
        output_cell_state: CellState,
        validate_void: bool,
        validate_halt: bool,
        annotation: String,
        priority: TransitionPriority,
    ) -> PyResult<()> {
        /*
        param input_terms:
        param output_tape_no:
        param output_cell_state:
        param validate_void:
        If true, check that the input terms do not all have a void state
        param validate_halt:
        If true, check that the halt state is not within input terms
        param annotation:
        :return:
        */
        let rs_terms = input_terms.iter().map(|py_term| {
            py_term.get_term().clone()
        }).collect::<Vec<MultiTapeTerm>>();

        let insert_res = self.inner.add_transition(
            rs_terms,
            output_tape_no,
            output_cell_state,
            validate_void,
            validate_halt,
            annotation,
            priority,
        );
        insert_res.map_err(|e| {
            // eprintln!("PY FAILED TO INSERT TRANSITION {}", e);
            PyValueError::new_err(e.to_string())
        })?;
        Ok(())
    }

    pub fn __or__(
        &self, other: &PyMultiTapeTransitionsGroup
    ) -> PyResult<PyMultiTapeTransitionsGroup> {
        let merged_res = self.inner.or(&other.inner);

        match merged_res {
            Ok(merged) => {
                Ok(PyMultiTapeTransitionsGroup {
                    inner: merged
                })
            }
            Err(err) => {
                Err(PyValueError::new_err(err.to_string()))
            }
        }
    }

    #[pyo3(signature=(require_annotations=false))]
    pub fn generate_equations(
        &self, require_annotations: bool
    ) -> PyResult<IndexMap<
        PyMultiTapeState, PyMultiTapeExpression
    >> {
        let equations_map_res = self.inner.generate_equations(require_annotations);
        let equations_map = match equations_map_res {
            Ok(equations_map) => { equations_map }
            Err(err) => {
                return Err(PyValueError::new_err(err.to_string()));
            }
        };

        let mut py_equations_map: IndexMap<
            PyMultiTapeState, PyMultiTapeExpression
        > = IndexMap::new();

        for (multi_tape_state, expression) in equations_map.into_iter() {
            let py_multi_tape_state = PyMultiTapeState::from(multi_tape_state);
            py_equations_map.insert(
                py_multi_tape_state, PyMultiTapeExpression::new(expression)
            );
        }

        Ok(py_equations_map)
    }

    pub fn build_prod_priority_map(&self) -> PyResult<IndexMap<
        PyMultiTapeProduct, TransitionPriority
    >> {
        let mut py_prod_priority_map: IndexMap<
            PyMultiTapeProduct, TransitionPriority
        > = IndexMap::new();

        let prod_priority_map = self.inner.prod_priority_map.clone();
        for (product, priority) in prod_priority_map.into_iter() {
            let py_product = PyMultiTapeProduct::from(product);
            py_prod_priority_map.insert(py_product, priority);
        }
        Ok(py_prod_priority_map)
    }

    pub fn get_all_states(&self) -> PyResult<HashSet<PyMultiTapeState>> {
        // TODO: go and implement IntoPyObject for IndexSet
        let all_states = self.inner.get_all_states();
        let mut py_all_states: HashSet<PyMultiTapeState> = HashSet::new();

        for multi_tape_state in all_states.into_iter() {
            let py_multi_tape_state = PyMultiTapeState::from(multi_tape_state);
            py_all_states.insert(py_multi_tape_state);
        }
        Ok(py_all_states)
    }
}

define_stub_info_gatherer!(stub_info);
