use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::{pyclass, pymethods, PyResult};
use pyo3_stub_gen::define_stub_info_gatherer;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use crate::automata::multi_tape_automata::{
    ProcessStepResult, WriteRecord
};
use crate::automata::py_terms::py_hash;
use crate::automata::py_terms_multitape::{
    PyMultiTapeExpression, PyMultiTapeProduct, D
};
use crate::automata::renderer::RenderFrame;
use crate::automata::rule_generator::{BidirectionalTape, TapeError};
use crate::automata::rule_generator_multitape::{
    AutomataError, BiDirectionalMultiTape, MultiTapeDataRegion
};
use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{
    AbstractMultiTapeExpression, MultiTapeExpression, MultiTapeState, TapeNo
};

/// `BLANK_INT` sentinel used by the python side to mean
/// "derive the cell width automatically".
pub const BLANK_INT: i64 = -1;

pub(crate) fn automata_err(err: AutomataError) -> PyErr {
    PyValueError::new_err(err.to_string())
}
fn tape_err(err: TapeError) -> PyErr {
    PyValueError::new_err(err.to_string())
}

/// `cell_width == BLANK_INT` -> `None` (auto-derive from the largest state)
pub(crate) fn to_cell_width(cell_width: i64) -> PyResult<Option<usize>> {
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

/// Accepts:
/// - a `PyMultiTapeState`
/// - a `(tape_no, tape_cell_state)` tuple
/// - any object exposing `tape_no` / `tape_cell_state` attributes
///   (i.e. the pure-python `MultiTapeState` dataclass)
pub fn extract_multi_tape_state(obj: &Bound<'_, PyAny>) -> PyResult<MultiTapeState> {
    if let Ok(py_state) = obj.extract::<PyMultiTapeState>() {
        return Ok(py_state.state);
    }
    if let Ok((tape_no, tape_cell_state)) = obj.extract::<(TapeNo, CellState)>() {
        return Ok(MultiTapeState::new(tape_no, tape_cell_state));
    }
    if let (Ok(tape_no_attr), Ok(cell_state_attr)) =
        (obj.getattr("tape_no"), obj.getattr("tape_cell_state"))
    {
        return Ok(MultiTapeState::new(
            tape_no_attr.extract::<TapeNo>()?,
            cell_state_attr.extract::<CellState>()?,
        ));
    }
    Err(PyTypeError::new_err(
        "Expected a MultiTapeState, a (tape_no, tape_cell_state) tuple, \
         or an object with tape_no / tape_cell_state attributes",
    ))
}

/// Accepts a `PyMultiTapeExpression`, `PyMultiTapeProduct` or `D` term.
fn extract_expression(obj: &Bound<'_, PyAny>) -> PyResult<MultiTapeExpression> {
    if let Ok(expr) = obj.extract::<PyMultiTapeExpression>() {
        return Ok(expr.expression);
    }
    if let Ok(product) = obj.extract::<PyMultiTapeProduct>() {
        return Ok(product.to_py_expression()?.expression);
    }
    if let Ok(term) = obj.extract::<D>() {
        return Ok(term.to_py_expression()?.expression);
    }
    Err(PyTypeError::new_err(
        "Expected a PyMultiTapeExpression, PyMultiTapeProduct or D term",
    ))
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct PyMultiTapeState {
    pub(crate) state: MultiTapeState,
}
impl PyMultiTapeState {
    pub fn from_state(state: MultiTapeState) -> Self {
        PyMultiTapeState { state }
    }
    pub fn get_state(&self) -> MultiTapeState {
        self.state
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMultiTapeState {
    #[new]
    pub fn new(tape_no: TapeNo, tape_cell_state: CellState) -> Self {
        PyMultiTapeState { state: MultiTapeState::new(tape_no, tape_cell_state) }
    }

    #[staticmethod]
    pub fn from_term(term: D) -> Self {
        let (tape_no, tape_cell_state) = term.get_term().state;
        PyMultiTapeState { state: MultiTapeState::new(tape_no, tape_cell_state) }
    }

    #[getter]
    pub fn tape_no(&self) -> TapeNo {
        self.state.tape_no
    }
    #[getter]
    pub fn tape_cell_state(&self) -> CellState {
        self.state.tape_cell_state
    }

    pub fn as_tuple(&self) -> (TapeNo, CellState) {
        (self.state.tape_no, self.state.tape_cell_state)
    }

    pub fn __deepcopy__(&self, _memo: &Bound<PyDict>) -> Self {
        *self
    }
    fn __hash__(&self) -> PyResult<isize> {
        py_hash(&self.state)
    }
    fn __eq__(&self, other: &Bound<PyAny>) -> PyResult<bool> {
        match extract_multi_tape_state(other) {
            Ok(other_state) => Ok(self.state == other_state),
            Err(_) => Ok(false),
        }
    }
    fn __lt__(&self, other: &Bound<PyAny>) -> PyResult<bool> {
        match extract_multi_tape_state(other) {
            Ok(other_state) => Ok(self.state < other_state),
            Err(_) => Ok(false),
        }
    }
    fn __repr__(&self) -> String {
        format!(
            "MultiTapeState(tape_no={}, tape_cell_state={})",
            self.state.tape_no, self.state.tape_cell_state
        )
    }
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug, Default)]
pub struct PyRenderFrame {
    frame: RenderFrame,
}
impl PyRenderFrame {
    pub fn from_frame(frame: RenderFrame) -> Self {
        PyRenderFrame { frame }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyRenderFrame {
    pub fn render(&self) -> String {
        self.frame.render()
    }
    pub fn get_lines(&self) -> Vec<String> {
        self.frame.get_lines().to_vec()
    }
    pub fn get_width(&self) -> usize {
        self.frame.get_width()
    }
    pub fn get_height(&self) -> usize {
        self.frame.get_height()
    }
    /// `(height, width)`, matching the python `get_dimensions`.
    pub fn get_dimensions(&self) -> (usize, usize) {
        self.frame.get_dimensions()
    }
    fn __str__(&self) -> String {
        self.frame.render()
    }
    fn __repr__(&self) -> String {
        format!(
            "RenderFrame(height={}, width={})",
            self.frame.get_height(),
            self.frame.get_width()
        )
    }
    fn __len__(&self) -> usize {
        self.frame.get_height()
    }
}

/// Snapshot of a single tape.
///
/// NOTE: this is a *copy* of the tape held by the automata, so writing to it
/// does not mutate the automata. Use `PyMultiTapeAutomata.write_cell` for that.
#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug, Default)]
pub struct PyBidirectionalTape {
    tape: BidirectionalTape,
}
impl PyBidirectionalTape {
    pub fn from_tape(tape: BidirectionalTape) -> Self {
        PyBidirectionalTape { tape }
    }
    pub fn get_tape(&self) -> &BidirectionalTape {
        &self.tape
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBidirectionalTape {
    #[new]
    #[pyo3(signature = (data = vec![]))]
    pub fn new(data: Vec<CellState>) -> Self {
        PyBidirectionalTape { tape: BidirectionalTape::new(data) }
    }

    pub fn read(&self, position: i64) -> CellState {
        self.tape.read(position)
    }
    pub fn write(&mut self, position: i64, value: CellState) {
        self.tape.write(position, value)
    }
    fn __getitem__(&self, position: i64) -> CellState {
        self.tape.read(position)
    }
    fn __setitem__(&mut self, position: i64, value: CellState) {
        self.tape.write(position, value)
    }
    /// Inclusive `(min_pos, max_pos)` range of allocated cells.
    pub fn get_range(&self) -> (i64, i64) {
        self.tape.get_range()
    }
    pub fn get_all_states(&self) -> Vec<CellState> {
        self.tape.get_all_states().into_iter().collect()
    }
    pub fn max_state(&self) -> CellState {
        self.tape.max_state()
    }
    pub fn prune(&mut self) -> (usize, usize) {
        let result = self.tape.prune();
        (result.forward_popped, result.reverse_popped)
    }
    /// Minimal contiguous region of tape data containing all non-void states.
    pub fn get_minimal_data_region(&mut self) -> Vec<CellState> {
        self.tape.get_minimal_data_region()
    }
    pub fn write_region(
        &mut self, position: i64, end_position: i64, values: Vec<CellState>,
    ) -> PyResult<()> {
        self.tape
            .write_region(position, end_position, &values)
            .map_err(tape_err)
    }
    #[pyo3(signature = (start_position, length, cell_width = BLANK_INT))]
    pub fn render_line(
        &self, start_position: i64, length: usize, cell_width: i64,
    ) -> PyResult<PyRenderFrame> {
        let cell_width = to_cell_width(cell_width)?;
        let frame = self
            .tape
            .render_line(start_position, length, cell_width)
            .map_err(tape_err)?;
        Ok(PyRenderFrame::from_frame(frame.clone()))
    }
    pub fn __deepcopy__(&self, _memo: &Bound<PyDict>) -> Self {
        self.clone()
    }
    fn __repr__(&self) -> String {
        let (min_pos, max_pos) = self.tape.get_range();
        format!("BidirectionalTape(min_pos={}, max_pos={})", min_pos, max_pos)
    }
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug, Default)]
pub struct PyBiDirectionalMultiTape {
    multi_tape: BiDirectionalMultiTape,
}
impl PyBiDirectionalMultiTape {
    pub fn from_multi_tape(multi_tape: BiDirectionalMultiTape) -> Self {
        PyBiDirectionalMultiTape { multi_tape }
    }
    pub fn get_multi_tape(&self) -> &BiDirectionalMultiTape {
        &self.multi_tape
    }
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug, Default)]
pub struct PyMultiTapeDataRegion {
    region: MultiTapeDataRegion,
}
impl PyMultiTapeDataRegion {
    pub fn from_region(region: MultiTapeDataRegion) -> Self {
        PyMultiTapeDataRegion { region }
    }
    pub fn get_region(&self) -> &MultiTapeDataRegion {
        &self.region
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMultiTapeDataRegion {
    #[new]
    pub fn new(start_position: i64, length: usize) -> Self {
        PyMultiTapeDataRegion {
            region: MultiTapeDataRegion::new(start_position, length),
        }
    }
    #[getter]
    pub fn start_position(&self) -> i64 {
        self.region.start_position
    }
    #[getter]
    pub fn length(&self) -> usize {
        self.region.length
    }
    pub fn get_tape_nos(&self) -> Vec<TapeNo> {
        self.region.tape_regions.keys().copied().collect()
    }
    pub fn get_tape_region(&self, tape_no: TapeNo) -> Option<Vec<CellState>> {
        self.region.tape_regions.get(&tape_no).cloned()
    }
    pub fn add_tape_region(&mut self, tape_no: TapeNo, region: Vec<CellState>) -> PyResult<()> {
        if region.len() != self.region.length {
            return Err(PyValueError::new_err(format!(
                "region length mismatch: expected {}, got {}",
                self.region.length,
                region.len()
            )));
        }
        self.region.add_tape_region(tape_no, region);
        Ok(())
    }
    pub fn get_product_slice_at(&self, index: usize) -> PyResult<PyMultiTapeProduct> {
        if index >= self.region.length {
            return Err(PyIndexError::new_err(format!(
                "index out of bounds: len is {} but index is {}",
                self.region.length,
                index
            )));
        }
        let product = self.region.get_product_slice_at(index);
        Ok(PyMultiTapeProduct::from_product(product))
    }
    pub fn __deepcopy__(&self, _memo: &Bound<PyDict>) -> Self {
        self.clone()
    }
    fn __repr__(&self) -> String {
        format!(
            "MultiTapeDataRegion(start_position={}, length={}, tape_nos={:?})",
            self.region.start_position,
            self.region.length,
            self.region.tape_regions.keys().collect::<Vec<_>>()
        )
    }
    fn __len__(&self) -> usize {
        self.region.length
    }
    fn __getitem__(&self, index: usize) -> PyResult<PyMultiTapeProduct> {
        self.get_product_slice_at(index)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBiDirectionalMultiTape {
    #[new]
    pub fn new() -> Self {
        PyBiDirectionalMultiTape { multi_tape: BiDirectionalMultiTape::default() }
    }

    pub fn get_tape_nos(&self) -> Vec<TapeNo> {
        self.multi_tape.get_tape_nos()
    }
    fn __getitem__(&self, tape_no: TapeNo) -> PyResult<PyBidirectionalTape> {
        match self.multi_tape.get_tape(tape_no) {
            Some(tape) => Ok(PyBidirectionalTape::from_tape(tape.clone())),
            None => Err(PyIndexError::new_err(format!("No tape {}", tape_no))),
        }
    }
    pub fn get_tape(&self, tape_no: TapeNo) -> Option<PyBidirectionalTape> {
        self.multi_tape
            .get_tape(tape_no)
            .map(|tape| PyBidirectionalTape::from_tape(tape.clone()))
    }
    pub fn read(&self, tape_no: TapeNo, position: i64) -> CellState {
        self.multi_tape
            .get_tape(tape_no)
            .map(|tape| tape.read(position))
            .unwrap_or(crate::automata::rule_generator::VOID_STATE)
    }
    pub fn write(
        &mut self, tape_no: TapeNo, position: i64, value: CellState,
    ) -> PyResult<()> {
        let tape = self.multi_tape.get_or_make_tape(tape_no).map_err(tape_err)?;
        tape.write(position, value);
        Ok(())
    }
    #[pyo3(signature = (tape_nos, freeze = false))]
    pub fn init_tapes(&mut self, tape_nos: Vec<TapeNo>, freeze: bool) -> PyResult<()> {
        self.multi_tape.init_tapes(&tape_nos, freeze).map_err(tape_err)
    }
    pub fn get_range(&self) -> (i64, i64) {
        self.multi_tape.get_range()
    }
    pub fn max_state(&self) -> CellState {
        self.multi_tape.max_state()
    }
    pub fn get_all_states(&self) -> Vec<CellState> {
        self.multi_tape.get_all_states().into_iter().collect()
    }
    pub fn prune(&mut self) {
        self.multi_tape.prune()
    }
    pub fn get_minimal_data_region_for(&self, tape_no: TapeNo) -> Vec<CellState> {
        match self.multi_tape.get_tape(tape_no) {
            Some(tape) => tape.clone().get_minimal_data_region(),
            None => vec![],
        }
    }
    pub fn get_minimal_data_region(&mut self) -> PyMultiTapeDataRegion {
        let minimal_region = self.multi_tape.get_minimal_data_region();
        PyMultiTapeDataRegion::from_region(minimal_region)
    }
    #[pyo3(signature = (start_position, length, cell_width = BLANK_INT))]
    pub fn render_tapes(
        &self, start_position: i64, length: usize, cell_width: i64,
    ) -> PyResult<PyRenderFrame> {
        let cell_width = to_cell_width(cell_width)?;
        let frame = self
            .multi_tape
            .render_tapes(start_position, length, cell_width)
            .map_err(tape_err)?;
        Ok(PyRenderFrame::from_frame(frame))
    }
    pub fn __deepcopy__(&self, _memo: &Bound<PyDict>) -> Self {
        self.clone()
    }
    fn __repr__(&self) -> String {
        format!("BiDirectionalMultiTape(tape_nos={:?})", self.multi_tape.get_tape_nos())
    }
}

#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug)]
pub struct PyWriteRecord {
    record: WriteRecord,
}
impl PyWriteRecord {
    pub fn from_record(record: WriteRecord) -> Self {
        PyWriteRecord { record }
    }
    pub fn get_record(&self) -> &WriteRecord {
        &self.record
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyWriteRecord {
    #[getter]
    pub fn origin_product(&self) -> PyMultiTapeProduct {
        PyMultiTapeProduct::from_product(self.record.origin_product.copy())
    }
    /// `(tape_no, position)`
    #[getter]
    pub fn write_target(&self) -> (TapeNo, i64) {
        self.record.write_target
    }
    #[getter]
    pub fn tape_no(&self) -> TapeNo {
        self.record.write_target.0
    }
    #[getter]
    pub fn position(&self) -> i64 {
        self.record.write_target.1
    }
    #[getter]
    pub fn tape_cell_state(&self) -> CellState {
        self.record.tape_cell_state
    }
    #[getter]
    pub fn annotation(&self) -> String {
        self.record.annotation.clone()
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
pub struct PyProcessStepResult {
    result: ProcessStepResult,
}
impl PyProcessStepResult {
    pub fn from_result(result: ProcessStepResult) -> Self {
        PyProcessStepResult { result }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyProcessStepResult {
    #[getter]
    pub fn prev_multi_tape(&self) -> PyBiDirectionalMultiTape {
        PyBiDirectionalMultiTape::from_multi_tape(self.result.prev_multi_tape.clone())
    }
    #[getter]
    pub fn new_multi_tape(&self) -> PyBiDirectionalMultiTape {
        PyBiDirectionalMultiTape::from_multi_tape(self.result.new_multi_tape.clone())
    }
    #[getter]
    pub fn active_writes(&self) -> Vec<PyWriteRecord> {
        self.result
            .active_writes
            .iter()
            .map(|record| PyWriteRecord::from_record(record.clone()))
            .collect()
    }
    pub fn get_num_active_writes(&self) -> usize {
        self.result.active_writes.len()
    }
    /// All annotations of the products that fired during this step.
    pub fn get_annotations(&self) -> Vec<String> {
        self.result
            .active_writes
            .iter()
            .map(|record| record.annotation.clone())
            .collect()
    }
    fn __repr__(&self) -> String {
        format!(
            "ProcessStepResult(active_writes={})",
            self.result.active_writes.len()
        )
    }
}

define_stub_info_gatherer!(stub_info);
