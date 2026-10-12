use std::collections::{BTreeSet, HashMap};
use std::fmt;
use indexmap::IndexMap;
use crate::automata::multi_tape_transition::{MultiTapeTransitionsGroup, TransitionPriority};
use crate::automata::product_writes_map::ProductWritesMap;
use crate::automata::renderer::RenderFrame;
use crate::automata::rule_generator::{BidirectionalTape, VOID_STATE};
use crate::automata::rule_generator_multitape::{AutomataError, BiDirectionalMultiTape};
use crate::automata::tape_overlaps::MultiTapeState;
use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{AbstractMultiTapeExpression, MultiTapeExpression, MultiTapeProduct, TapeNo};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WriteRecord {
    pub origin_product: MultiTapeProduct,
    /// `(tape_no, position)`
    pub write_target: (TapeNo, i64),
    pub tape_cell_state: CellState,
    /// for debugging purposes (to trace originating product)
    pub annotation: String,
}

impl WriteRecord {
    pub fn log(&self) {
        println!("{}", self);
    }
}

impl fmt::Display for WriteRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (tape_no, position) = self.write_target;
        write!(
            f,
            "{} | ({}, {}) -> {} ({})",
            self.origin_product._to_string("D"),
            tape_no, position, self.tape_cell_state, self.annotation
        )
    }
}

#[derive(Debug, Clone)]
pub struct ProcessStepResult {
    pub prev_multi_tape: BiDirectionalMultiTape,
    pub new_multi_tape: BiDirectionalMultiTape,
    pub active_writes: Vec<WriteRecord>,
}

pub struct MultiTapeAutomata {
    multi_tape: BiDirectionalMultiTape,
    prod_to_state_map: ProductWritesMap,
    leftmost_extent: i64,
    rightmost_extent: i64,

    transitions_group: MultiTapeTransitionsGroup,
    /// `IndexMap` mirrors the insertion-ordered Python `dict`.
    state_eq_map: IndexMap<MultiTapeState, MultiTapeExpression>,
    prod_priority_map: IndexMap<MultiTapeProduct, TransitionPriority>
}

impl MultiTapeAutomata {
    pub fn new(
        transitions_group: MultiTapeTransitionsGroup,
        require_annotations: bool
    ) -> Result<MultiTapeAutomata, AutomataError> {
        let state_eq_map = transitions_group.generate_equations(require_annotations)?;
        let prod_priority_map = transitions_group.build_prod_priority_map();
        let prod_to_state_map = Self::reverse_state_eq_map(&state_eq_map, true)?;
        let (leftmost_extent, rightmost_extent) =
            Self::compute_rule_range(&prod_to_state_map);

        Ok(MultiTapeAutomata {
            multi_tape: BiDirectionalMultiTape::default(),
            prod_to_state_map,
            leftmost_extent,
            rightmost_extent,
            transitions_group,
            state_eq_map,
            prod_priority_map
        })
    }

    /// Python `__getitem__`; `None` when the tape has not been allocated yet.
    pub fn get_tape(&self, tape_no: TapeNo) -> Option<&BidirectionalTape> {
        self.multi_tape.get_tape(tape_no)
    }

    pub fn get_tape_nos(&self) -> Vec<TapeNo> {
        self.multi_tape.get_tape_nos()
    }

    pub fn get_prod_to_state_map(&self) -> ProductWritesMap {
        self.prod_to_state_map.clone()
    }

    pub fn get_state_eq_map(&self) -> IndexMap<MultiTapeState, MultiTapeExpression> {
        self.state_eq_map.clone()
    }

    pub fn get_multi_tape(&self) -> &BiDirectionalMultiTape {
        &self.multi_tape
    }

    pub fn leftmost_extent(&self) -> i64 {
        self.leftmost_extent
    }

    pub fn rightmost_extent(&self) -> i64 {
        self.rightmost_extent
    }

    pub fn get_rule_range(&self) -> (i64, i64) {
        (self.leftmost_extent, self.rightmost_extent)
    }

    fn compute_rule_range(prod_to_state_map: &ProductWritesMap) -> (i64, i64) {
        let mut leftmost_extent: i64 = 0;
        let mut rightmost_extent: i64 = 0;

        for product in prod_to_state_map.products() {
            for term in product.to_flat_terms() {
                let offset = term.position;
                leftmost_extent = leftmost_extent.min(offset);
                rightmost_extent = rightmost_extent.max(offset);
            }
        }

        assert!(leftmost_extent <= 0);
        assert!(rightmost_extent >= 0);
        (leftmost_extent, rightmost_extent)
    }

    pub fn init_tapes(&mut self, tape_nos: &[TapeNo]) -> Result<(), AutomataError> {
        self.multi_tape.init_tapes(tape_nos, false)?;
        Ok(())
    }

    /// Populate the automata cells from `position` to `end_position`
    /// (inclusive) using `data` as a pattern.
    pub fn write_region(
        &mut self,
        position: i64,
        end_position: i64,
        data: &[MultiTapeState],
    ) -> Result<(), AutomataError> {
        self.multi_tape.write_region(position, end_position, data)?;
        Ok(())
    }

    /// `cell_width == None` is the `BLANK_INT` sentinel on the Python side.
    pub fn render_tapes(
        &self,
        start_position: i64,
        length: usize,
        cell_width: Option<usize>,
    ) -> Result<RenderFrame, AutomataError> {
        Ok(self.multi_tape.render_tapes(start_position, length, cell_width)?)
    }

    /// Given a mapping from output tape states to expressions over input tape
    /// states, create a mapping of tape state products to the `tape_no` and
    /// tape cell state they write to:
    ///
    /// `product -> tape_no -> output tape cell state`
    ///
    /// The reason this doesn't return `product -> MultiTapeState` is so that
    /// write collisions can be detected (given the same tape, a product should
    /// only ever write a single unique cell state, if at all).
    pub fn reverse_state_eq_map(
        state_eq_map: &IndexMap<MultiTapeState, MultiTapeExpression>,
        require_annotations: bool,
    ) -> Result<ProductWritesMap, AutomataError> {
        let mut prod_to_state_map = ProductWritesMap::new();

        for (multi_tape_output, expr) in state_eq_map.iter() {
            for product in expr._get_products() {
                /*
                Whether a product transitions a contiguous region of void
                states into a non-void state. This can't be allowed because
                it would make the simulation range infinite.
                */
                let product_is_void = product
                    .to_flat_terms()
                    .iter()
                    .all(|term| term.state.1 == VOID_STATE);

                if product_is_void {
                    return Err(AutomataError::VoidProduct {
                        product: product._to_string("D"),
                        output: *multi_tape_output,
                    });
                }

                let annotation = product.get_annotation();
                if require_annotations && annotation.is_empty() {
                    return Err(AutomataError::EmptyAnnotation {
                        product: product._to_string("D"),
                    });
                }

                prod_to_state_map.insert(product.copy(), *multi_tape_output)?;
            }
        }

        Ok(prod_to_state_map)
    }

    /// Check if the given product is satisfied at the given position
    /// on the tapes.
    pub fn product_satisfies(&self, product: &MultiTapeProduct, position: i64) -> bool {
        for term in product.to_flat_terms() {
            let term_offset = term.position;
            let (tape_no, tape_cell_state) = term.state;
            let term_position = position + term_offset;

            // a tape that has not been allocated reads as VOID_STATE
            // everywhere, matching `get_or_make_tape(...).read(...)`
            let read_state = self
                .multi_tape
                .get_tape(tape_no)
                .map(|tape| tape.read(term_position))
                .unwrap_or(VOID_STATE);

            if read_state != tape_cell_state {
                return false;
            }
        }
        true
    }

    pub fn process_step(
        &self, log_active_writes: bool,
    ) -> Result<ProcessStepResult, AutomataError> {
        // i.e., no void states filled in by default
        let existing_tape_nos = self.multi_tape.get_tape_nos();
        let (min_pos, max_pos) = self.multi_tape.get_range();
        let mut new_multi_tape = self.multi_tape.clone();
        let scan_start = min_pos + self.leftmost_extent;
        let scan_end = max_pos + self.rightmost_extent + 1;

        // record all (tape_no, position) -> tape_cell_state writes
        let mut writes_map: HashMap<(TapeNo, i64), CellState> = HashMap::new();
        let mut annotations_map: HashMap<(TapeNo, i64), BTreeSet<String>> =
            HashMap::new();
        let mut active_writes: Vec<WriteRecord> = Vec::new();

        for position in scan_start..scan_end {
            let mut written_tape_nos: BTreeSet<TapeNo> = BTreeSet::new();

            // apply all matching rules at this position to get new tape states
            for (matching_product, product_writes) in self.prod_to_state_map.iter() {
                if !self.product_satisfies(matching_product, position) {
                    continue;
                }

                let annotation = matching_product.get_annotation().to_string();

                for (tape_no, tape_cell_state) in product_writes.iter() {
                    let tape_no = *tape_no;
                    let tape_cell_state = *tape_cell_state;
                    let write_target: (TapeNo, i64) = (tape_no, position);
                    // previously recorded write to this tape cell, if any
                    let prev_write = writes_map
                        .get(&write_target)
                        .copied()
                        .unwrap_or(tape_cell_state);

                    if prev_write != tape_cell_state {
                        let prev_annotations = annotations_map
                            .get(&write_target)
                            .map(|annotations| {
                                annotations.iter().cloned().collect::<Vec<String>>()
                            })
                            .unwrap_or_default();

                        return Err(AutomataError::ConflictingWrite {
                            tape_no,
                            position,
                            product: matching_product._to_string("D"),
                            annotation,
                            previous: prev_write,
                            incoming: tape_cell_state,
                            previous_annotations: prev_annotations,
                        });
                    }

                    let write_record = WriteRecord {
                        origin_product: matching_product.copy(),
                        write_target,
                        tape_cell_state,
                        annotation: annotation.clone(),
                    };
                    if log_active_writes {
                        write_record.log();
                    }
                    active_writes.push(write_record);

                    writes_map.insert(write_target, tape_cell_state);
                    annotations_map
                        .entry(write_target)
                        .or_default()
                        .insert(annotation.clone());

                    let output_tape = new_multi_tape.get_or_make_tape(tape_no)?;
                    output_tape.write(position, tape_cell_state);
                    debug_assert_eq!(output_tape.read(position), tape_cell_state);
                    written_tape_nos.insert(tape_no);
                }
            }

            // copy over unchanged tape cells for tapes that
            // were not written to at this position
            for tape_no in existing_tape_nos.iter().copied() {
                if written_tape_nos.contains(&tape_no) {
                    continue;
                }

                let previous_tape_val: CellState = self
                    .multi_tape
                    .get_tape(tape_no)
                    .map(|tape| tape.read(position))
                    .unwrap_or(VOID_STATE);

                let new_tape = new_multi_tape.get_or_make_tape(tape_no)?;
                new_tape.write(position, previous_tape_val);
            }
        }

        Ok(ProcessStepResult {
            prev_multi_tape: self.multi_tape.clone(),
            new_multi_tape,
            active_writes,
        })
    }

    /// Set the new state of the multi-tape after going forward a single step.
    /// The returned result also carries the previous multi-tape state.
    pub fn step(&mut self, verbose: bool) -> Result<ProcessStepResult, AutomataError> {
        let process_result = self.process_step(verbose)?;
        self.multi_tape = process_result.new_multi_tape.clone();
        Ok(process_result)
    }
}
