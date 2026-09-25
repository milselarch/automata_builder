use crate::automata::terms::{CellState, Product, Term};

pub const BLANK_INT: i64 = -1;
pub const VOID_STATE: CellState = 0;
pub const HALT_STATE: CellState = 1;

pub fn is_halt_state(term: &Term) -> bool {
    term.state == HALT_STATE
}

use std::collections::HashMap;
use std::fmt;
use std::fmt::Debug;
use std::ops::Index;
use indexmap::IndexSet;
use crate::automata::renderer::RenderError;
use crate::automata::renderer::TapeRenderFrame;
use crate::automata::terms_multitape::TapeNo;

/// Contains a set of transitions for a cellular automaton,
/// defined as a mapping from input states to output state:
/// `A[] -> output state`
#[derive(Debug, Clone, Default)]
pub struct AutomataTransitionsGroup {
    pub num_states: Option<usize>,
    pub transitions_map: HashMap<Vec<Term>, usize>,
    pub transitions: Vec<(Vec<Term>, usize)>,
}

impl AutomataTransitionsGroup {
    /// Creates a fresh, empty transitions group.
    pub fn spawn_new(num_states: Option<usize>) -> Self {
        Self {
            num_states,
            transitions_map: HashMap::new(),
            transitions: Vec::new(),
        }
    }

    /// Equivalent of Python's `__getitem__`.
    /// Returns the input terms (wrapped as a `PyProduct`) and the output state
    /// at the given index.
    pub fn get(&self, index: usize) -> (Product, usize) {
        let (input_terms, output_state) = &self.transitions[index];
        let input_product = Product::new(input_terms.clone());
        (input_product, *output_state)
    }

    /// Attempts to add a new transition.
    ///
    /// Returns `Ok(true)` if the transition was newly added, `Ok(false)` if an
    /// identical transition already existed, and `Err(_)` on conflicting
    /// transitions or invalid states.
    pub fn add_transition(
        &mut self,
        input_terms: Vec<Term>,
        output_state: usize,
        ban_halt_state: bool,
    ) -> Result<bool, String> {
        if let Some(&existing_state) = self.transitions_map.get(&input_terms) {
            if existing_state == output_state {
                return Ok(false);
            }
            return Err(format!(
                "Conflicting transition for input terms {input_terms:?}: \
                 {output_state} vs {existing_state}"
            ));
        }

        let num_states = self.num_states.unwrap_or(usize::MAX);
        assert!(output_state < num_states);

        for term in &input_terms {
            let state = term.state;
            assert!(state < num_states as u32);

            if ban_halt_state && is_halt_state(term) {
                return Err(format!(
                    "Cannot add transition with halt state term: {term:?}"
                ));
            }
        }

        self.transitions.push((input_terms.clone(), output_state));
        self.transitions_map.insert(input_terms, output_state);
        Ok(true)
    }

    /// Merges another group's transitions into this one, consuming `self`
    /// to allow fluent chaining (mirrors Python's `-> Self` return).
    pub fn merge(mut self, other: AutomataTransitionsGroup) -> Self {
        for (input_terms, output_state) in other.transitions {
            self.add_transition(input_terms, output_state, false)
                .expect("conflicting transition encountered during merge");
        }
        self
    }
}


/// Static so that `Index<i64>` can hand out a reference for out-of-range cells.


#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TapeError {
    /// Requested cell width cannot fit the largest state present on the tape.
    CellWidthTooSmall {
        cell_width: usize,
        max_state: CellState,
    },
    /// `write_region` was called with an empty pattern.
    EmptyWritePattern,
    /// Tried to allocate a new tape after the tape set was frozen.
    TapesFrozen { tape_no: TapeNo },
    /// Frames produced during rendering could not be laid out.
    Render(RenderError),
}
impl From<RenderError> for TapeError {
    fn from(err: RenderError) -> Self {
        TapeError::Render(err)
    }
}

impl fmt::Display for TapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TapeError::CellWidthTooSmall {
                cell_width, max_state
            } => write!(
                f,
                "Cell width {} is too small to fit the largest state {}",
                cell_width, max_state
            ),
            TapeError::EmptyWritePattern => {
                write!(f, "write_region requires a non-empty pattern")
            }
            TapeError::TapesFrozen { tape_no } => {
                write!(f, "Cannot allocate new tape after tape set is frozen (tape_no={})", tape_no)
            },
            TapeError::Render(render_err) => {
                write!(f, "Render error: {}", render_err)
            }
        }
    }
}
impl std::error::Error for TapeError {}


#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PruneResult {
    pub forward_popped: usize,
    pub reverse_popped: usize,
}

#[derive(Debug, Clone, Default)]
pub struct BidirectionalTape {
    /// automata cell states from position 0 and higher
    /// note that position increases for cells as we go rightwards in data
    data: Vec<CellState>,
    /// automata cell states from position -1 and lower
    /// note that position decreases for cells as we go rightwards in rev_data
    rev_data: Vec<CellState>,
}
impl BidirectionalTape {
    pub fn new(data: Vec<CellState>) -> BidirectionalTape {
        BidirectionalTape { data, rev_data: vec![] }
    }
    /// Inclusive range of positions for which cell data is currently allocated.
    /// Note that `max_pos` is `-1` when the rightwards half is empty.
    pub fn get_range(&self) -> (i64, i64) {
        let min_pos = -(self.rev_data.len() as i64);
        let max_pos = self.data.len() as i64 - 1;
        (min_pos, max_pos)
    }

    pub fn get_min_data_pos(&self) -> Option<i64> {
        /*
        Get the minimum position where a non-VOID cell can be found.
        */
        let rev_data_length = self.rev_data.len();
        for (i, state) in self.rev_data.iter().rev().enumerate() {
            let pos = -((rev_data_length - i) as i64);
            if *state != VOID_STATE {
                return Some(pos);
            }
        }
        for (pos, state) in self.data.iter().enumerate() {
            if *state != VOID_STATE {
                return Some(pos as i64);
            }
        }
        None
    }

    pub fn get_max_data_pos(&self) -> Option<i64> {
        /*
        Get the maximum position where a non-VOID cell can be found.
        */
        for (i, state) in self.data.iter().rev().enumerate() {
            if *state != VOID_STATE {
                let pos = self.data.len() as i64 - 1 - i as i64;
                return Some(pos);
            }
        }
        for (pos, state) in self.rev_data.iter().enumerate() {
            if *state != VOID_STATE {
                return Some(-(pos as i64));
            }
        }
        None
    }

    pub fn get_minimal_data_range(&self) -> Option<(i64, i64)> {
        /*
        Inclusive range of positions for which non-VOID
        cell data is currently allocated.
        Returns `None` if all cells are void.
        */
        let min_pos = self.get_min_data_pos()?;
        let max_pos = self.get_max_data_pos()?;
        assert!(min_pos <= max_pos);
        Some((min_pos, max_pos))
    }

    // TODO: consider tracking unique states instead of recomputing
    pub fn get_all_states(&self) -> IndexSet<CellState> {
        self.data.iter().chain(self.rev_data.iter()).copied().collect()
    }

    pub fn max_state(&self) -> CellState {
        self.data
            .iter()
            .chain(self.rev_data.iter())
            .copied()
            .max()
            .unwrap_or(VOID_STATE)
    }

    /// Prune trailing void cells in both directions.
    pub fn prune(&mut self) -> PruneResult {
        let mut forward_popped = 0;
        let mut reverse_popped = 0;

        while self.data.last() == Some(&VOID_STATE) {
            forward_popped += 1;
            self.data.pop();
        }
        while self.rev_data.last() == Some(&VOID_STATE) {
            reverse_popped += 1;
            self.rev_data.pop();
        }

        PruneResult { forward_popped, reverse_popped }
    }
    /// Get the minimal contiguous region of tape data
    /// that contains all non-void states.
    pub fn get_minimal_data_region(&mut self) -> Vec<CellState> {
        self.prune();

        let data_region = self
            .rev_data
            .iter()
            .rev()
            .chain(self.data.iter())
            .copied();

        let mut minimal_data_region: Vec<CellState> = Vec::new();
        let mut data_region_started = false;

        for tape_cell_state in data_region {
            if tape_cell_state != VOID_STATE {
                data_region_started = true;
            }
            if !data_region_started {
                continue;
            }
            minimal_data_region.push(tape_cell_state);
        }

        // remove trailing void state cells
        // this can happen if all data cells are from the rev_data region
        while minimal_data_region.last() == Some(&VOID_STATE) {
            minimal_data_region.pop();
        }

        minimal_data_region
    }

    pub fn read_region(
        &self, start_position: i64, length: usize
    ) -> Vec<CellState> {
        let mut region = Vec::with_capacity(length);
        for k in 0..length {
            let position = start_position + k as i64;
            let state = self.read(position);
            region.push(state);
        }
        region
    }

    /// `cell_width == None` means "derive the width from the largest state"
    /// (equivalent to the `BLANK_INT` sentinel on the Python side).
    pub fn render_line(
        &self,
        start_position: i64,
        length: usize,
        cell_width: Option<usize>,
    ) -> Result<TapeRenderFrame, TapeError> {
        let max_state = self.max_state();
        let min_cell_width = max_state.to_string().len();

        let cell_width = match cell_width {
            None => min_cell_width,
            Some(width) if width < min_cell_width => {
                return Err(TapeError::CellWidthTooSmall { cell_width: width, max_state });
            }
            Some(width) => width,
        };

        let cells_to_render = length / (cell_width + 1);
        let mut line = String::with_capacity(length);

        for k in 0..cells_to_render {
            let position = start_position + k as i64;
            let state = self.read(position);
            line.push_str(&format!("{:0width$}", state, width = cell_width));
            line.push('|');
        }

        // pad out to the requested length
        line.push_str(&" ".repeat(length.saturating_sub(line.len())));

        Ok(TapeRenderFrame::new(&*line, cells_to_render, cell_width))
    }

    pub fn read(&self, position: i64) -> CellState {
        if position >= 0 {
            let index = position as usize;
            self.data.get(index).copied().unwrap_or(VOID_STATE)
        } else {
            let rev_index = (-position - 1) as usize;
            self.rev_data.get(rev_index).copied().unwrap_or(VOID_STATE)
        }
    }

    pub fn write(&mut self, position: i64, value: CellState) {
        if position >= 0 {
            let index = position as usize;
            if index >= self.data.len() {
                self.data.resize(index + 1, VOID_STATE);
            }
            self.data[index] = value;
        } else {
            let rev_index = (-position - 1) as usize;
            if rev_index >= self.rev_data.len() {
                self.rev_data.resize(rev_index + 1, VOID_STATE);
            }
            self.rev_data[rev_index] = value;
        }
    }

    /// Populate the cells from `position` to `end_position` (inclusive)
    /// using `values` as a repeating pattern.
    pub fn write_region(
        &mut self,
        position: i64,
        end_position: i64,
        values: &[CellState],
    ) -> Result<(), TapeError> {
        if values.is_empty() {
            return Err(TapeError::EmptyWritePattern);
        }

        for new_position in position..=end_position {
            let offset = (new_position - position) as usize;
            let value = values[offset % values.len()];
            self.write(new_position, value);
        }
        Ok(())
    }
}

impl Index<i64> for BidirectionalTape {
    type Output = CellState;

    fn index(&self, position: i64) -> &Self::Output {
        if position >= 0 {
            self.data.get(position as usize).unwrap_or(&VOID_STATE)
        } else {
            self.rev_data
                .get((-position - 1) as usize)
                .unwrap_or(&VOID_STATE)
        }
    }
}
