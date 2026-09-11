use indexmap::IndexMap;

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::{pyclass, pymethods, PyResult};
use pyo3_stub_gen::define_stub_info_gatherer;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::automata::py_rule_generator_multitape::{
    PyBidirectionalTape, PyRenderFrame, BLANK_INT,
};
use crate::automata::py_terms::{PyExpression, PyProduct, A};
use crate::automata::single_tape_automata::{
    ProcessStepResult, SingleTapeAutomata, SingleTapeAutomataError,
    WriteRecord,
};
use crate::automata::terms::{AbstractExpression, CellState, Expression};

fn automata_err(err: SingleTapeAutomataError) -> PyErr {
    PyValueError::new_err(err.to_string())
}

/// `cell_width == BLANK_INT` -> `None` (auto-derive from the largest state)
fn to_cell_width(cell_width: i64) -> PyResult<Option<usize>> {
    if cell_width == BLANK_INT {
        return Ok(None);
    }
    if cell_width < 0 {
        return Err(PyValueError::new_err(format!(
            "cell_width must be non-negative or BLANK_INT ({}), got {}",
            BLANK_INT, cell_width
        )));
    }
    Ok(Some(cell_width as usize))
}

/// Accepts a `PyExpression`, `PyProduct` or `A` term.
fn extract_expression(obj: &Bound<'_, PyAny>) -> PyResult<Expression> {
    if let Ok(expr) = obj.extract::<PyExpression>() {
        return Ok(expr.expression);
    }
    if let Ok(product) = obj.extract::<PyProduct>() {
        return Ok(product.product.to_expression());
    }
    if let Ok(term) = obj.extract::<A>() {
        return Ok(term.get_term().to_expression());
    }
    Err(PyTypeError::new_err(
        "Expected a PyExpression, PyProduct or A term",
    ))
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug)]
pub struct PySingleTapeWriteRecord {
    record: WriteRecord,
}
impl PySingleTapeWriteRecord {
    pub fn from_record(record: WriteRecord) -> Self {
        PySingleTapeWriteRecord { record }
    }
    pub fn get_record(&self) -> &WriteRecord {
        &self.record
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PySingleTapeWriteRecord {
    #[getter]
    pub fn origin_product(&self) -> PyProduct {
        PyProduct::from_product(self.record.origin_product.copy())
    }
    #[getter]
    pub fn position(&self) -> i64 {
        self.record.position
    }
    #[getter]
    pub fn cell_state(&self) -> CellState {
        self.record.cell_state
    }
    pub fn log(&self) {
        self.record.log()
    }
    fn __repr__(&self) -> String {
        self.record.to_string()
    }
    fn __str__(&self) -> String {
        self.record.to_string()
    }
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug)]
pub struct PySingleTapeProcessStepResult {
    result: ProcessStepResult,
}
impl PySingleTapeProcessStepResult {
    pub fn from_result(result: ProcessStepResult) -> Self {
        PySingleTapeProcessStepResult { result }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PySingleTapeProcessStepResult {
    #[getter]
    pub fn prev_tape(&self) -> PyBidirectionalTape {
        PyBidirectionalTape::from_tape(self.result.prev_tape.clone())
    }
    #[getter]
    pub fn new_tape(&self) -> PyBidirectionalTape {
        PyBidirectionalTape::from_tape(self.result.new_tape.clone())
    }
    #[getter]
    pub fn active_writes(&self) -> Vec<PySingleTapeWriteRecord> {
        self.result
            .active_writes
            .iter()
            .map(|record| PySingleTapeWriteRecord::from_record(record.clone()))
            .collect()
    }
    pub fn get_num_active_writes(&self) -> usize {
        self.result.active_writes.len()
    }
    fn __repr__(&self) -> String {
        format!(
            "SingleTapeProcessStepResult(active_writes={})",
            self.result.active_writes.len()
        )
    }
}

#[gen_stub_pyclass]
#[pyclass]
pub struct PySingleTapeAutomata {
    automata: SingleTapeAutomata,
}
impl PySingleTapeAutomata {
    pub fn get_automata(&self) -> &SingleTapeAutomata {
        &self.automata
    }
    pub fn get_automata_mut(&mut self) -> &mut SingleTapeAutomata {
        &mut self.automata
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PySingleTapeAutomata {
    /// `state_eq_map`: `dict[int, PyExpression]`
    /// (output cell state -> expression over input states)
    #[new]
    pub fn new(state_eq_map: &Bound<'_, PyDict>) -> PyResult<Self> {
        let mut rs_state_eq_map: IndexMap<CellState, Expression> =
            IndexMap::with_capacity(state_eq_map.len());

        for (key, value) in state_eq_map.iter() {
            let output_state = key.extract::<CellState>()?;
            let expression = extract_expression(&value)?;
            rs_state_eq_map.insert(output_state, expression);
        }

        let automata = SingleTapeAutomata::new(rs_state_eq_map).map_err(automata_err)?;
        Ok(PySingleTapeAutomata { automata })
    }

    /// Snapshot of the tape. NOTE: this is a *copy*, so writing to it does not
    /// mutate the automata; use `write_cell` for that.
    pub fn get_tape(&self) -> PyBidirectionalTape {
        PyBidirectionalTape::from_tape(self.automata.get_tape().clone())
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
    /// Inclusive `(min_pos, max_pos)` range of allocated cells.
    pub fn get_range(&self) -> (i64, i64) {
        self.automata.get_tape().get_range()
    }

    /// `product -> output cell state`, as a list of pairs.
    pub fn get_prod_to_state_map(&self) -> Vec<(PyProduct, CellState)> {
        self.automata
            .get_prod_to_state_map()
            .iter()
            .map(|(product, output_state)| {
                (PyProduct::from_product(product.copy()), *output_state)
            })
            .collect()
    }

    pub fn get_state_eq_map(&self) -> Vec<(CellState, PyExpression)> {
        self.automata
            .get_state_eq_map()
            .into_iter()
            .map(|(state, expr)| (state, PyExpression::new(expr)))
            .collect()
    }

    pub fn get_num_products(&self) -> usize {
        self.automata.get_prod_to_state_map().len()
    }

    /// Populate the automata cells from `position` to `end_position`
    /// (inclusive) using `data` as a repeating pattern.
    #[pyo3(signature = (position, end_position, data))]
    pub fn write_region(
        &mut self, position: i64, end_position: i64, data: Vec<CellState>,
    ) -> PyResult<()> {
        self.automata
            .write_region(position, end_position, &data)
            .map_err(automata_err)
    }

    pub fn write_cell(&mut self, position: i64, value: CellState) {
        self.automata.write(position, value)
    }

    pub fn read_cell(&self, position: i64) -> CellState {
        self.automata.read(position)
    }

    fn __getitem__(&self, position: i64) -> CellState {
        self.automata.read(position)
    }
    fn __setitem__(&mut self, position: i64, value: CellState) {
        self.automata.write(position, value)
    }

    /// Minimal contiguous region of the tape containing all non-void states.
    pub fn get_minimal_data_region(&self) -> Vec<CellState> {
        self.automata.get_tape().clone().get_minimal_data_region()
    }

    pub fn max_state(&self) -> CellState {
        self.automata.get_tape().max_state()
    }

    pub fn get_all_states(&self) -> Vec<CellState> {
        self.automata.get_tape().get_all_states().into_iter().collect()
    }

    #[pyo3(signature = (start_position, length, cell_width = BLANK_INT))]
    pub fn render_tape(
        &self, start_position: i64, length: usize, cell_width: i64,
    ) -> PyResult<PyRenderFrame> {
        let cell_width = to_cell_width(cell_width)?;
        let frame = self
            .automata
            .render_tape(start_position, length, cell_width)
            .map_err(automata_err)?;
        Ok(PyRenderFrame::from_frame(frame))
    }

    pub fn product_satisfies(&self, product: PyProduct, position: i64) -> bool {
        self.automata.product_satisfies(&product.product, position)
    }

    /// Compute the next step without mutating the automata.
    #[pyo3(signature = (log_active_writes = true))]
    pub fn process_step(
        &self, log_active_writes: bool,
    ) -> PyResult<PySingleTapeProcessStepResult> {
        let result = self
            .automata
            .process_step(log_active_writes)
            .map_err(automata_err)?;
        Ok(PySingleTapeProcessStepResult::from_result(result))
    }

    /// Advance the automata a single timestep.
    #[pyo3(signature = (verbose = false))]
    pub fn step(&mut self, verbose: bool) -> PyResult<PySingleTapeProcessStepResult> {
        let result = self.automata.step(verbose).map_err(automata_err)?;
        Ok(PySingleTapeProcessStepResult::from_result(result))
    }

    fn __repr__(&self) -> String {
        let (leftmost, rightmost) = self.automata.get_rule_range();
        let (min_pos, max_pos) = self.automata.get_tape().get_range();
        format!(
            "SingleTapeAutomata(range=({}, {}), rule_range=({}, {}), num_products={})",
            min_pos, max_pos, leftmost, rightmost,
            self.automata.get_prod_to_state_map().len()
        )
    }
}

define_stub_info_gatherer!(stub_info);
