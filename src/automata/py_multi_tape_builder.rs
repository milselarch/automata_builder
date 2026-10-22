use std::collections::BTreeSet;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use pyo3_stub_gen::define_stub_info_gatherer;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use crate::automata::multi_tape_builder::{
    CompileTapesError, CompileTapesResult, MultiTapeBuilder,
    MultiTapeStatePathRemap, StatePath,
};
use crate::automata::py_multi_tape_automata::PyMultiTapeAutomata;
use crate::automata::py_rule_generator_multitape::{
    extract_multi_tape_state, PyMultiTapeState,
};
use crate::automata::py_terms::A;
use crate::automata::py_terms_multitape::PyMultiTapeProduct;
use crate::automata::tape_overlaps::TapeOverlaps;
use crate::automata::tape_overlaps_fsm::TapeOverlapsFSM;
use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{MultiTapeState, TapeNo};

/// `(tape_no, tape_cell_state)` pair representing a `MultiTapeState`
pub type PyStateTuple = (TapeNo, CellState);

fn compile_err(err: CompileTapesError) -> PyErr {
    PyValueError::new_err(err.to_string())
}

fn state_to_tuple(state: &MultiTapeState) -> PyStateTuple {
    (state.tape_no, state.tape_cell_state)
}

fn path_to_tuples(path: &[MultiTapeState]) -> Vec<PyStateTuple> {
    path.iter().map(state_to_tuple).collect()
}

fn tuples_to_path(path: &[PyStateTuple]) -> StatePath {
    path.iter()
        .map(|(tape_no, tape_cell_state)| MultiTapeState::new(*tape_no, *tape_cell_state))
        .collect()
}

/// Extract a set of multi-tape states from any python iterable of
/// `MultiTapeState` (python or rust) objects or `(tape_no, state)` tuples
fn extract_multi_tape_states(obj: &Bound<'_, PyAny>) -> PyResult<BTreeSet<MultiTapeState>> {
    let mut states = BTreeSet::new();
    for item in obj.try_iter()? {
        states.insert(extract_multi_tape_state(&item?)?);
    }
    Ok(states)
}

/// Python wrapper around the rust `TapeOverlaps`
/// (multi-tape state -> relative offset -> overlapping multi-tape states)
#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug)]
pub struct PyTapeOverlaps {
    pub(crate) overlaps: TapeOverlaps,
}
impl PyTapeOverlaps {
    pub fn from_overlaps(overlaps: TapeOverlaps) -> Self {
        Self { overlaps }
    }
    pub fn get_overlaps(&self) -> &TapeOverlaps {
        &self.overlaps
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyTapeOverlaps {
    fn __len__(&self) -> usize {
        self.overlaps.len()
    }

    /// Accepts a `MultiTapeState`, `PyMultiTapeState`
    /// or `(tape_no, tape_cell_state)` tuple
    fn __contains__(&self, state: &Bound<'_, PyAny>) -> PyResult<bool> {
        Ok(self.overlaps.contains(&extract_multi_tape_state(state)?))
    }

    /// All source states, as sorted `(tape_no, tape_cell_state)` tuples
    pub fn get_all_states(&self) -> Vec<PyStateTuple> {
        self.overlaps.get_all_states().iter().map(state_to_tuple).collect()
    }

    /// All source states, as sorted `PyMultiTapeState` objects
    pub fn get_all_multi_tape_states(&self) -> Vec<PyMultiTapeState> {
        self.overlaps
            .get_all_states()
            .into_iter()
            .map(PyMultiTapeState::from_state)
            .collect()
    }

    pub fn get_all_tape_nos(&self) -> Vec<TapeNo> {
        self.overlaps.get_all_tape_nos().into_iter().collect()
    }

    pub fn get_states_for_tape(&self, tape_no: TapeNo) -> Vec<PyStateTuple> {
        self.overlaps.get_states_for_tape(tape_no).iter().map(state_to_tuple).collect()
    }

    /// States that can overlap with `source_state` at the given relative offset
    pub fn get_overlaps_for_offset(
        &self, source_state: &Bound<'_, PyAny>, offset: i64,
    ) -> PyResult<Vec<PyStateTuple>> {
        let source_state = extract_multi_tape_state(source_state)?;
        Ok(self
            .overlaps
            .get_overlaps_for_offset(&source_state, offset)
            .map(|states| states.iter().map(state_to_tuple).collect())
            .unwrap_or_default())
    }

    /// Render the overlaps of the given source states (all states
    /// if `source_states` is None) as a list of lines
    #[pyo3(signature=(source_states=None))]
    pub fn visualize_for_states(
        &self, source_states: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Vec<String>> {
        let source_states = match source_states {
            Some(states) => extract_multi_tape_states(states)?,
            None => self.overlaps.get_all_states(),
        };
        Ok(self.overlaps.visualize_for_states(&source_states))
    }

    #[pyo3(signature=(source_states=None))]
    pub fn print_for_states(&self, source_states: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        match source_states {
            Some(states) => {
                let states = extract_multi_tape_states(states)?;
                self.overlaps.print_for_states(Some(&states));
            }
            None => self.overlaps.print_for_states(None),
        }
        Ok(())
    }
}

/// Python wrapper around the rust `TapeOverlapsFSM`
#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug)]
pub struct PyTapeOverlapsFSM {
    pub(crate) fsm: TapeOverlapsFSM,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyTapeOverlapsFSM {
    /// Number of transitions in the FSM
    fn __len__(&self) -> usize {
        self.fsm.len()
    }

    pub fn num_states(&self) -> usize {
        self.fsm.num_states()
    }

    /// Tape overlaps of the FSM state at the given index
    pub fn get_state_overlaps(&self, index: usize) -> PyResult<PyTapeOverlaps> {
        self.fsm
            .get_state(index)
            .map(|state| PyTapeOverlaps::from_overlaps(state.tape_overlaps().clone()))
            .ok_or_else(|| PyValueError::new_err(format!("No FSM state at index {}", index)))
    }

    /// Merge the tape overlaps of all FSM states into a single `PyTapeOverlaps`
    pub fn merge(&self) -> PyTapeOverlaps {
        PyTapeOverlaps::from_overlaps(self.fsm.merge())
    }
}

/// Remap of multi-tape state paths (combinations of tape cell states
/// at the same position across all tapes) to single tape cell states
#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug)]
pub struct PyMultiTapeStatePathRemap {
    pub(crate) remap: MultiTapeStatePathRemap,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMultiTapeStatePathRemap {
    fn __len__(&self) -> usize {
        self.remap.num_normal_remaps()
    }

    #[getter]
    pub fn remap_counter_start(&self) -> CellState {
        self.remap.remap_counter_start()
    }
    #[getter]
    pub fn remap_counter_end(&self) -> CellState {
        self.remap.remap_counter_end()
    }
    #[getter]
    pub fn next_free_counter(&self) -> CellState {
        self.remap.next_free_counter()
    }
    #[getter]
    pub fn num_normal_remaps(&self) -> usize {
        self.remap.num_normal_remaps()
    }
    #[getter]
    pub fn num_void_remaps(&self) -> usize {
        self.remap.num_void_remaps()
    }
    #[getter]
    pub fn num_halt_remaps(&self) -> usize {
        self.remap.num_halt_remaps()
    }

    /// `(state path, remapped state)` pairs in insertion order
    pub fn get_tape_state_path_remap(&self) -> Vec<(Vec<PyStateTuple>, CellState)> {
        self.remap
            .tape_state_path_remap()
            .iter()
            .map(|(path, state)| (path_to_tuples(path), *state))
            .collect()
    }

    pub fn get_void_state_paths(&self) -> Vec<Vec<PyStateTuple>> {
        self.remap.void_state_paths().iter().map(|path| path_to_tuples(path)).collect()
    }

    pub fn get_halt_state_paths(&self) -> Vec<Vec<PyStateTuple>> {
        self.remap.halt_state_paths().iter().map(|path| path_to_tuples(path)).collect()
    }

    pub fn get_all_remap_states(&self) -> Vec<CellState> {
        self.remap.get_all_remap_states().into_iter().collect()
    }

    pub fn remap(&self, state_path: Vec<PyStateTuple>) -> PyResult<CellState> {
        self.remap.remap(&tuples_to_path(&state_path)).map_err(compile_err)
    }

    /// Returns None for the HALT state
    pub fn rev_lookup(&self, tape_cell_state: CellState) -> PyResult<Option<Vec<PyStateTuple>>> {
        Ok(self
            .remap
            .rev_lookup(tape_cell_state)
            .map_err(compile_err)?
            .map(|path| path_to_tuples(path)))
    }

    pub fn remap_from_product_to_term(&self, product: &PyMultiTapeProduct) -> PyResult<A> {
        let term = self
            .remap
            .remap_from_product_to_term(&product.product)
            .map_err(compile_err)?;
        Ok(A::from_term(&term))
    }
}

/// Result of compiling a multi-tape automata into a single tape automata
#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug)]
pub struct PyCompileTapesResult {
    pub(crate) result: CompileTapesResult,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyCompileTapesResult {
    fn __len__(&self) -> usize {
        self.result.transitions_group.len()
    }

    #[getter]
    pub fn num_unrepresentable_products(&self) -> usize {
        self.result.num_unrepresentable_products
    }

    pub fn count_unique_states(&self) -> usize {
        self.result.count_unique_states()
    }

    /// `(input terms, output state, annotation)` for every
    /// transition, in the order in which they were added
    pub fn get_transitions(&self) -> Vec<(Vec<A>, CellState, String)> {
        self.result
            .transitions_group
            .transitions
            .iter()
            .map(|transition| (
                transition.input_terms.iter().map(A::from_term).collect(),
                transition.output_state,
                transition.annotation.clone(),
            ))
            .collect()
    }

    pub fn get_state_remap(&self) -> PyMultiTapeStatePathRemap {
        PyMultiTapeStatePathRemap { remap: self.result.state_remap.clone() }
    }

    pub fn remap_from_product_to_term(&self, product: &PyMultiTapeProduct) -> PyResult<A> {
        let term = self
            .result
            .state_remap
            .remap_from_product_to_term(&product.product)
            .map_err(compile_err)?;
        Ok(A::from_term(&term))
    }

    pub fn remap_from_product_to_state(&self, product: &PyMultiTapeProduct) -> PyResult<CellState> {
        Ok(self.remap_from_product_to_term(product)?.get_term().state)
    }
}

/// Rust implementation of the multi-tape to single tape compiler
/// (port of the python `MultiTapeBuilder.compile_tapes` pipeline)
#[gen_stub_pyclass]
#[pyclass]
#[derive(Clone, Debug)]
pub struct PyMultiTapeBuilder {
    builder: MultiTapeBuilder,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMultiTapeBuilder {
    #[new]
    pub fn new(multi_tape_automata: &PyMultiTapeAutomata) -> Self {
        Self { builder: MultiTapeBuilder::new(multi_tape_automata.get_automata()) }
    }

    #[getter]
    pub fn leftmost_extent(&self) -> i64 {
        self.builder.leftmost_extent()
    }
    #[getter]
    pub fn rightmost_extent(&self) -> i64 {
        self.builder.rightmost_extent()
    }

    pub fn get_tape_nos(&self) -> Vec<TapeNo> {
        self.builder.get_tape_nos()
    }

    /// Declare that every state in overlap_states can overlap with any
    /// other state at any relative offset in the initial automata tape.
    /// Accepts an iterable of `MultiTapeState`, `PyMultiTapeState`
    /// or `(tape_no, tape_cell_state)` tuples
    pub fn declare_initial_group_overlaps(
        &mut self, overlap_states: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let states = extract_multi_tape_states(overlap_states)?;
        self.builder.declare_initial_group_overlaps(&states);
        Ok(())
    }

    pub fn get_initial_overlaps(&self) -> PyTapeOverlaps {
        PyTapeOverlaps::from_overlaps(self.builder.get_initial_overlaps().clone())
    }

    #[pyo3(signature=(verbose=false))]
    pub fn build_overlaps_fsm(&self, py: Python<'_>, verbose: bool) -> PyResult<PyTapeOverlapsFSM> {
        let builder = &self.builder;
        let fsm = py
            .allow_threads(|| builder.build_overlaps_fsm(verbose))
            .map_err(compile_err)?;
        Ok(PyTapeOverlapsFSM { fsm })
    }

    /// Combine the multi-tape automata into a single tape automata
    #[pyo3(signature=(verbose=false))]
    pub fn compile_tapes(&self, py: Python<'_>, verbose: bool) -> PyResult<PyCompileTapesResult> {
        let builder = &self.builder;
        let result = py
            .allow_threads(|| builder.compile_tapes(verbose))
            .map_err(compile_err)?;
        Ok(PyCompileTapesResult { result })
    }
}

define_stub_info_gatherer!(stub_info);
