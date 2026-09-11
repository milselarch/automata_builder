use indexmap::IndexSet;
use std::collections::BTreeMap;
use std::fmt;

use crate::automata::product_writes_map::ProductWritesError;
use crate::automata::renderer::{RenderFrame, TapeRenderFrame};
use crate::automata::rule_generator::{BidirectionalTape, TapeError, VOID_STATE};
use crate::automata::tape_overlaps::MultiTapeState;
use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{
    MultiTapeProduct, MultiTapeTerm, TapeNo
};

#[derive(Debug, Clone, Default)]
pub struct MultiTapeDataRegion {
    pub start_position: i64,
    pub length: usize,
    pub tape_regions: BTreeMap<TapeNo, Vec<CellState>>,
}
impl MultiTapeDataRegion {
    pub fn new(start_position: i64, length: usize) -> MultiTapeDataRegion {
        MultiTapeDataRegion { start_position, length, tape_regions: BTreeMap::new() }
    }
    pub fn add_tape_region(&mut self, tape_no: TapeNo, region: Vec<CellState>) {
        assert_eq!(region.len(), self.length);
        self.tape_regions.insert(tape_no, region);
    }
    pub fn get_product_slice_at(&self, index: usize) -> MultiTapeProduct {
        let position = self.start_position + index as i64;
        let mut terms: Vec<MultiTapeTerm> = vec![];

        for tape_no in self.tape_regions.keys() {
            let tape_region = self.tape_regions.get(tape_no).unwrap();
            let cell_state = tape_region[index];
            terms.push(MultiTapeTerm::new(
                position, (*tape_no, cell_state),
                false,
            ));
        }

        MultiTapeProduct::new(terms)
    }
}


#[derive(Debug, Clone, Default)]
pub struct BiDirectionalMultiTape {
    /// `BTreeMap` so that iteration is always in ascending `TapeNo` order,
    /// matching the `sorted(self._tapes.keys())` calls on the Python side.
    tapes: BTreeMap<TapeNo, BidirectionalTape>,
    freeze_tapes: bool,
}
impl BiDirectionalMultiTape {
    pub fn new(tapes: BTreeMap<TapeNo, BidirectionalTape>) -> BiDirectionalMultiTape {
        BiDirectionalMultiTape { tapes, freeze_tapes: false }
    }

    pub fn get_or_make_tape(
        &mut self,
        tape_no: TapeNo,
    ) -> Result<&mut BidirectionalTape, TapeError> {
        if !self.tapes.contains_key(&tape_no) && self.freeze_tapes {
            return Err(TapeError::TapesFrozen { tape_no });
        }
        Ok(self.tapes.entry(tape_no).or_default())
    }

    /// Read-only lookup; unlike `get_or_make_tape` this never allocates a tape.
    pub fn get_tape(&self, tape_no: TapeNo) -> Option<&BidirectionalTape> {
        self.tapes.get(&tape_no)
    }

    pub fn init_tapes(&mut self, tape_nos: &[TapeNo], freeze: bool) -> Result<(), TapeError> {
        for tape_no in tape_nos {
            self.get_or_make_tape(*tape_no)?;
        }
        if freeze {
            self.freeze_tapes = true;
        }
        Ok(())
    }

    pub fn get_tape_nos(&self) -> Vec<TapeNo> {
        self.tapes.keys().copied().collect()
    }

    /// Populate the automata cells from `position` to `end_position`
    /// (inclusive) using `data` as a repeating pattern.
    pub fn write_region(
        &mut self,
        position: i64,
        end_position: i64,
        data: &[MultiTapeState],
    ) -> Result<(), TapeError> {
        if data.is_empty() {
            return Err(TapeError::EmptyWritePattern);
        }
        for new_position in position..=end_position {
            let offset = (new_position - position) as usize;
            let value = data[offset % data.len()];
            self.write(new_position, value)?;
        }
        Ok(())
    }

    pub fn write(&mut self, position: i64, value: MultiTapeState) -> Result<(), TapeError> {
        let tape = self.get_or_make_tape(value.tape_no)?;
        tape.write(position, value.tape_cell_state);
        Ok(())
    }

    pub fn get_all_states(&self) -> IndexSet<CellState> {
        let mut all_states = IndexSet::new();
        for tape in self.tapes.values() {
            all_states.extend(tape.get_all_states());
        }
        all_states
    }

    pub fn max_state(&self) -> CellState {
        self.tapes
            .values()
            .map(|tape| tape.max_state())
            .max()
            .unwrap_or(VOID_STATE)
    }

    pub fn prune(&mut self) {
        for tape in self.tapes.values_mut() {
            tape.prune();
        }
    }

    /// `cell_width == None` derives the width from the largest state present
    /// across all tapes (the `BLANK_INT` sentinel on the Python side).
    pub fn render_tapes(
        &self,
        start_position: i64,
        length: usize,
        cell_width: Option<usize>,
    ) -> Result<RenderFrame, TapeError> {
        let max_state = self.max_state();
        let min_cell_width = max_state.to_string().len();

        let cell_width = match cell_width {
            None => min_cell_width,
            Some(width) if width < min_cell_width => {
                return Err(TapeError::CellWidthTooSmall { cell_width: width, max_state });
            }
            Some(width) => width,
        };

        // tape 0 is always rendered, even if it holds no data yet
        let mut tape_nos: Vec<TapeNo> = self.get_tape_nos();
        if !tape_nos.contains(&0) {
            tape_nos.push(0);
            tape_nos.sort_unstable();
        }

        let left_tabs: Vec<String> = tape_nos
            .iter()
            .map(|tape_no| format!("Tape {}: ", tape_no))
            .collect();
        // padding keeps the sidebar rectangular even when tape numbers
        // differ in digit count
        let left_sidebar = RenderFrame::from_padded_lines(left_tabs);
        let content_width = length.saturating_sub(left_sidebar.get_width());

        let empty_tape = BidirectionalTape::default();
        let mut tape_view_lines: Vec<TapeRenderFrame> = Vec::with_capacity(tape_nos.len());

        for tape_no in &tape_nos {
            let tape = self.tapes.get(tape_no).unwrap_or(&empty_tape);
            let tape_line = tape.render_line(start_position, content_width, Some(cell_width))?;
            tape_view_lines.push(tape_line);
        }

        // TODO: align by actual space consumed by tape
        let num_cells = tape_view_lines.first().map(|frame| frame.num_cells).unwrap_or(0);
        // width of text actually consumed by tape cells, excluding padding
        let tape_content_width = tape_view_lines
            .first()
            .map(|frame| frame.get_space_consumed())
            .unwrap_or(0);

        let start_pos_str = format!("{}<", start_position);
        let end_pos_str = format!(">{}", start_position + num_cells as i64 - 1);
        let buffer_len = tape_content_width
            .saturating_sub(start_pos_str.len())
            .saturating_sub(end_pos_str.len());

        let position_str = format!(
            "{}{}{}{}{}",
            " ".repeat(left_sidebar.get_width()),
            start_pos_str,
            " ".repeat(buffer_len),
            end_pos_str,
            " ".repeat(content_width.saturating_sub(tape_content_width)),
        );

        let tape_frames: Vec<RenderFrame> =
            tape_view_lines.iter().map(|frame| (*frame).clone()).collect();
        let tapes_frame = RenderFrame::join_vertically(&tape_frames)?;
        let body = RenderFrame::join_horizontally(&[left_sidebar, tapes_frame])?;

        Ok(RenderFrame::join_vertically(&[
            RenderFrame::from_line(position_str),
            body,
        ])?)
    }

    /// Inclusive range of positions for which tape cell data is currently
    /// allocated across all tapes. Always contains 0.
    pub fn get_range(&self) -> (i64, i64) {
        let mut min_pos: i64 = 0;
        let mut max_pos: i64 = 0;

        for tape in self.tapes.values() {
            let (tape_min, tape_max) = tape.get_range();
            min_pos = min_pos.min(tape_min);
            max_pos = max_pos.max(tape_max);
        }
        (min_pos, max_pos)
    }

    pub fn get_minimal_data_range(&self) -> Option<(i64, i64)> {
        /*
        Returns the smallest range of positions for which
        all the non-VOID cells are contained across all individual tapes.
        */
        let mut start_pos: Option<i64> = None;
        let mut end_pos: Option<i64> = None;

        for (_, tape) in self.tapes.iter() {
            if let Some((tape_start, tape_end)) = tape.get_minimal_data_range() {
                start_pos = Some(start_pos.map_or(tape_start, |s| s.min(tape_start)));
                end_pos = Some(end_pos.map_or(tape_end, |e| e.max(tape_end)));
            }
        }
        start_pos.zip(end_pos)
    }

    pub fn get_minimal_data_region(&self) -> MultiTapeDataRegion {
        let minimal_data_range = self.get_minimal_data_range();
        if let None = minimal_data_range {
            return MultiTapeDataRegion::default();
        }

        let (start_pos, end_pos) = self.get_range();
        let length = usize::try_from(end_pos - start_pos + 1).unwrap();
        let mut multi_tape_data_region = MultiTapeDataRegion::new(start_pos, length);

        for (tape_no, tape) in self.tapes.iter() {
            let tape_minimal_data_region = tape.read_region(start_pos, length);
            multi_tape_data_region.add_tape_region(
                *tape_no, tape_minimal_data_region
            );
        }
        multi_tape_data_region
    }
}

#[derive(Debug, Clone)]
pub enum AutomataError {
    Tape(TapeError),
    Writes(ProductWritesError),
    /// A product made purely out of void states would make the simulation
    /// range infinite, so it is rejected up-front.
    VoidProduct { product: String, output: MultiTapeState },
    /// `require_annotations` was set, but the product carried no annotation.
    EmptyAnnotation { product: String },
    /// Two different products want to write different states to the same cell.
    ConflictingWrite {
        tape_no: TapeNo,
        position: i64,
        product: String,
        annotation: String,
        previous: CellState,
        incoming: CellState,
        previous_annotations: Vec<String>,
    },
}

impl From<TapeError> for AutomataError {
    fn from(err: TapeError) -> Self {
        AutomataError::Tape(err)
    }
}
impl From<ProductWritesError> for AutomataError {
    fn from(err: ProductWritesError) -> Self {
        AutomataError::Writes(err)
    }
}

impl fmt::Display for AutomataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AutomataError::Tape(err) => write!(f, "{}", err),
            AutomataError::Writes(err) => write!(f, "{}", err),
            AutomataError::VoidProduct { product, output } => write!(
                f,
                "Product {} transitions void states to non-void state {}, \
                 which is not allowed since it would make the simulation \
                 range infinite",
                product, output
            ),
            AutomataError::EmptyAnnotation { product } => {
                write!(f, "EMPTY ANNOTATION product={}", product)
            }
            AutomataError::ConflictingWrite {
                tape_no, position, product, annotation,
                previous, incoming, previous_annotations,
            } => write!(
                f,
                "Conflicting writes to tape {} from matching_product={} \
                 annotation={:?} at position {}: {} vs {} \
                 (prev_annotations={:?})",
                tape_no, product, annotation, position,
                previous, incoming, previous_annotations
            ),
        }
    }
}
impl std::error::Error for AutomataError {}

