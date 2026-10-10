use pyo3::{pyclass, pymethods, PyResult};
use pyo3::exceptions::PyValueError;
use crate::automata::multi_tape_transition::{
    MultiTapeState, MultiTapeTransition, MultiTapeTransitionsGroup
};
use crate::automata::py_terms_multitape::D;
use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{MultiTapeTerm, TapeNo};

// PyO3 wrapper classes
#[pyclass]
pub struct PyMultiTapeState {
    inner: MultiTapeState,
}

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
}

#[pyclass]
pub struct PyMultiTapeTransition {
    inner: MultiTapeTransition,
}

#[pymethods]
impl PyMultiTapeTransition {
    #[new]
    #[pyo3(signature = (
        input_terms, output_state, annotation="", priority=0
    ))]
    pub fn new(
        input_terms: Vec<D>,
        output_state: &PyMultiTapeState,
        annotation: &str,
        priority: i32,
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
    pub fn priority(&self) -> i32 {
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

#[pyclass]
pub struct PyMultiTapeTransitionsGroup {
    inner: MultiTapeTransitionsGroup,
}

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

    pub fn add_transition(
        &mut self,
        input_terms: Vec<D>,
        output_tape_no: TapeNo,
        output_cell_state: CellState,
        validate_void: bool,
        validate_halt: bool,
        annotation: String,
        priority: i32,
    ) -> PyResult<()> {
        let rs_terms = input_terms.iter().map(|py_term| {
            py_term.get_term().clone()
        }).collect::<Vec<MultiTapeTerm>>();

        self.inner
            .add_transition(
                rs_terms,
                output_tape_no,
                output_cell_state,
                validate_void,
                validate_halt,
                annotation,
                priority,
            )
            .map_err(|e|
                PyValueError::new_err(e.to_string())
            )
    }

    pub fn __or__(
        &self, other: &PyMultiTapeTransitionsGroup
    ) -> PyResult<PyMultiTapeTransitionsGroup> {
        self.inner
            .or(&other.inner)
            .map(|inner| PyMultiTapeTransitionsGroup {
                inner
            }).map_err(|e| PyValueError::new_err(e))
    }
}
