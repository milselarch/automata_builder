use std::collections::HashMap;
use indexmap::IndexMap;
use pyo3::{pyclass, pymethods, Bound, PyAny, PyResult};
use pyo3::exceptions::PyIndexError;
use pyo3_stub_gen::{define_stub_info_gatherer, PyStubType, TypeInfo};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use crate::automata::multi_tape_automata::MultiTapeAutomata;
use crate::automata::multi_tape_transition::TransitionPriority;
use crate::automata::py_multi_tape_transition::PyMultiTapeTransitionsGroup;
use crate::automata::py_rule_generator_multitape::{extract_multi_tape_state, PyBiDirectionalMultiTape, PyBidirectionalTape, PyMultiTapeDataRegion, PyMultiTapeState, PyProcessStepResult, PyRenderFrame, BLANK_INT};
use crate::automata::py_terms_multitape::{PyMultiTapeExpression, PyMultiTapeProduct};
use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{AbstractMultiTapeExpression, MultiTapeProduct, MultiTapeState, TapeNo};

#[gen_stub_pyclass]
#[pyclass]
pub struct PyMultiTapeAutomata {
    automata: MultiTapeAutomata,
}
impl PyMultiTapeAutomata {
    pub fn get_automata(&self) -> &MultiTapeAutomata {
        &self.automata
    }
    pub fn get_automata_mut(&mut self) -> &mut MultiTapeAutomata {
        &mut self.automata
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMultiTapeAutomata {
    #[new]
    #[pyo3(signature=(transitions_group, require_annotations=false))]
    pub fn new(
        transitions_group: &PyMultiTapeTransitionsGroup,
        require_annotations: bool
    ) -> PyResult<Self> {
        let rs_transitions_group = transitions_group.inner.clone();
        let automata = MultiTapeAutomata::new(
            rs_transitions_group, require_annotations
        ).map_err(crate::automata::py_rule_generator_multitape::automata_err)?;
        Ok(PyMultiTapeAutomata { automata })
    }

    /// Python `__getitem__` -> snapshot of the requested tape.
    fn __getitem__(&self, tape_no: TapeNo) -> PyResult<PyBidirectionalTape> {
        match self.automata.get_tape(tape_no) {
            Some(tape) => Ok(PyBidirectionalTape::from_tape(tape.clone())),
            None => Err(PyIndexError::new_err(format!("No tape {}", tape_no))),
        }
    }

    pub fn get_tape(&self, tape_no: TapeNo) -> Option<PyBidirectionalTape> {
        self.automata
            .get_tape(tape_no)
            .map(|tape| PyBidirectionalTape::from_tape(tape.clone()))
    }

    pub fn get_tape_nos(&self) -> Vec<TapeNo> {
        self.automata.get_tape_nos()
    }

    pub fn get_multi_tape(&self) -> PyBiDirectionalMultiTape {
        PyBiDirectionalMultiTape::from_multi_tape(self.automata.get_multi_tape().clone())
    }

    #[getter]
    pub fn leftmost_extent(&self) -> i64 {
        self.automata.leftmost_extent()
    }
    #[getter]
    pub fn rightmost_extent(&self) -> i64 {
        self.automata.rightmost_extent()
    }
    pub fn get_rule_range(&self) -> (i64, i64) {
        self.automata.get_rule_range()
    }
    /// Inclusive `(min_pos, max_pos)` range of allocated cells across all tapes.
    pub fn get_range(&self) -> (i64, i64) {
        self.automata.get_multi_tape().get_range()
    }

    /// `product -> {tape_no: output tape cell state}` as a list of pairs
    /// (products are not hashable-friendly dict keys across the FFI boundary
    /// in every python version, so a list of pairs is returned instead).
    pub fn get_prod_to_state_map(
        &self,
    ) -> Vec<(PyMultiTapeProduct, std::collections::BTreeMap<TapeNo, CellState>)> {
        self.automata
            .get_prod_to_state_map()
            .iter()
            .map(|(product, writes)| {
                (PyMultiTapeProduct::from_product(product.copy()), writes.clone())
            })
            .collect()
    }

    pub fn get_state_eq_map(&self) -> Vec<(PyMultiTapeState, PyMultiTapeExpression)> {
        self.automata
            .get_state_eq_map()
            .into_iter()
            .map(|(state, expr)| {
                (PyMultiTapeState::from_state(state), PyMultiTapeExpression::new(expr))
            })
            .collect()
    }

    pub fn get_prod_priority_map(&self) -> HashMap<
        PyMultiTapeProduct, TransitionPriority
    > {
        let mut py_prod_priority_map: HashMap<
            PyMultiTapeProduct, TransitionPriority
        > = HashMap::new();

        let prod_priority_map = self.automata.get_prod_priority_map();
        for (product, priority) in prod_priority_map.iter() {
            let py_product = PyMultiTapeProduct::from(product.clone());
            py_prod_priority_map.insert(py_product, priority.clone());
        }
        py_prod_priority_map
    }

    pub fn get_num_products(&self) -> usize {
        self.automata.get_prod_to_state_map().len()
    }

    #[pyo3(signature = (tape_nos))]
    pub fn init_tapes(&mut self, tape_nos: Vec<TapeNo>) -> PyResult<()> {
        self.automata.init_tapes(&tape_nos).map_err(
            crate::automata::py_rule_generator_multitape::automata_err
        )
    }

    /// Populate the automata cells from `position` to `end_position`
    /// (inclusive) using `data` as a repeating pattern.
    #[pyo3(signature = (position, end_position, data))]
    pub fn write_region(
        &mut self,
        position: i64,
        end_position: i64,
        data: Vec<Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let mut states: Vec<MultiTapeState> = Vec::with_capacity(data.len());
        for item in data.iter() {
            states.push(extract_multi_tape_state(item)?);
        }
        self.automata
            .write_region(position, end_position, &states)
            .map_err(crate::automata::py_rule_generator_multitape::automata_err)
    }

    pub fn write_cell(
        &mut self, tape_no: TapeNo, position: i64, value: CellState,
    ) -> PyResult<()> {
        let state = MultiTapeState::new(tape_no, value);
        self.automata
            .write_region(position, position, &[state])
            .map_err(crate::automata::py_rule_generator_multitape::automata_err)
    }

    pub fn read_cell(&self, tape_no: TapeNo, position: i64) -> CellState {
        self.automata
            .get_tape(tape_no)
            .map(|tape| tape.read(position))
            .unwrap_or(crate::automata::rule_generator::VOID_STATE)
    }

    pub fn get_minimal_data_region(&mut self) -> PyMultiTapeDataRegion {
        let minimal_region = self.automata.get_multi_tape().get_minimal_data_region();
        PyMultiTapeDataRegion::from_region(minimal_region)
    }

    /// Minimal contiguous region of the tape containing all non-void states.
    pub fn get_minimal_data_region_for(&self, tape_no: TapeNo) -> Vec<CellState> {
        match self.automata.get_tape(tape_no) {
            Some(tape) => tape.clone().get_minimal_data_region(),
            None => vec![],
        }
    }

    #[pyo3(signature = (start_position, length, cell_width = BLANK_INT))]
    pub fn render_tapes(
        &self, start_position: i64, length: usize, cell_width: i64,
    ) -> PyResult<PyRenderFrame> {
        let cell_width = crate::automata::py_rule_generator_multitape::to_cell_width(cell_width)?;
        let frame = self
            .automata
            .render_tapes(start_position, length, cell_width)
            .map_err(crate::automata::py_rule_generator_multitape::automata_err)?;
        Ok(PyRenderFrame::from_frame(frame))
    }

    pub fn product_satisfies(
        &self, product: PyMultiTapeProduct, position: i64,
    ) -> bool {
        self.automata.product_satisfies(&product.product, position)
    }

    /// Compute the next step without mutating the automata.
    #[pyo3(signature = (log_active_writes = true))]
    pub fn process_step(&self, log_active_writes: bool) -> PyResult<PyProcessStepResult> {
        let result = self.automata.process_step(log_active_writes).map_err(crate::automata::py_rule_generator_multitape::automata_err)?;
        Ok(PyProcessStepResult::from_result(result))
    }

    /// Advance the automata a single timestep.
    #[pyo3(signature = (verbose = false))]
    pub fn step(&mut self, verbose: bool) -> PyResult<PyProcessStepResult> {
        let result = self.automata.step(verbose).map_err(crate::automata::py_rule_generator_multitape::automata_err)?;
        Ok(PyProcessStepResult::from_result(result))
    }

    fn __repr__(&self) -> String {
        let (leftmost, rightmost) = self.automata.get_rule_range();
        format!(
            "MultiTapeAutomata(tape_nos={:?}, rule_range=({}, {}), num_products={})",
            self.automata.get_tape_nos(),
            leftmost,
            rightmost,
            self.automata.get_prod_to_state_map().len()
        )
    }
}

define_stub_info_gatherer!(stub_info);
