use std::collections::{BTreeSet, HashMap};
use std::fmt;

use indexmap::{IndexMap, IndexSet};

use crate::automata::renderer::RenderFrame;
use crate::automata::rule_generator::{BidirectionalTape, TapeError, VOID_STATE};
use crate::automata::terms::{AbstractExpression, CellState, Expression, Product};

#[derive(Debug, Clone)]
pub enum SingleTapeAutomataError {
    Tape(TapeError),
    /// The same product wants to write two different output states.
    ConflictingOutput {
        product: Product,
        existing: CellState,
        incoming: CellState,
    },
    /// A product made purely out of void states would make the simulation
    /// range infinite, so it is rejected up-front.
    VoidProduct { product: Product, output: CellState },
    /// Two different products want to write different states to the same cell.
    ConflictingWrite {
        position: i64,
        product: Product,
        previous: CellState,
        incoming: CellState,
        previous_products: Vec<Product>,
    },
}

impl From<TapeError> for SingleTapeAutomataError {
    fn from(err: TapeError) -> Self {
        SingleTapeAutomataError::Tape(err)
    }
}

impl fmt::Display for SingleTapeAutomataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SingleTapeAutomataError::Tape(err) => write!(f, "{}", err),
            SingleTapeAutomataError::ConflictingOutput { product, existing, incoming } => {
                write!(
                    f,
                    "Conflicting output states for product={}: {} vs {}",
                    product, existing, incoming
                )
            }
            SingleTapeAutomataError::VoidProduct { product, output } => write!(
                f,
                "Product {} transitions void states to non-void state {}, \
                 which is not allowed since it would make the simulation \
                 range infinite",
                product, output
            ),
            SingleTapeAutomataError::ConflictingWrite {
                position, product, previous, incoming, previous_products,
            } => {
                let product_strings = previous_products.iter().map(
                    |p| p._to_string_with_annotation("A")
                ).collect::<Vec<_>>();
                write!(
                    f,
                    "Conflicting writes from matching_product={} at position {}: \
                    {} vs {} (prev_products={:?})",
                    product._to_string_with_annotation("A"), position,
                    previous, incoming, product_strings
                )
            },
        }
    }
}
impl std::error::Error for SingleTapeAutomataError {}

/// Single-tape analogue of `ProductWritesMap`.
///
/// Because there is only one tape, the `product -> tape_no -> CellState`
/// nesting collapses to `product -> CellState`.
///
/// `IndexMap` preserves insertion order, mirroring the multi-tape version.
#[derive(Debug, Clone, Default)]
pub struct ProductWriteMap {
    prod_to_state_map: IndexMap<Product, CellState>,
    frozen: bool,
}

impl ProductWriteMap {
    pub fn new() -> Self {
        Self { prod_to_state_map: IndexMap::new(), frozen: false }
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn len(&self) -> usize {
        self.prod_to_state_map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.prod_to_state_map.is_empty()
    }

    pub fn contains_product(&self, product: &Product) -> bool {
        self.prod_to_state_map.contains_key(product)
    }

    pub fn get(&self, product: &Product) -> Option<CellState> {
        self.prod_to_state_map.get(product).copied()
    }

    pub fn products(&self) -> impl Iterator<Item = &Product> {
        self.prod_to_state_map.keys()
    }

    pub fn iter(&self) -> indexmap::map::Iter<'_, Product, CellState> {
        self.prod_to_state_map.iter()
    }

    pub fn insert(
        &mut self, product: Product, output_state: CellState,
    ) -> Result<(), SingleTapeAutomataError> {
        if self.frozen {
            // mirrors ProductWritesError::Frozen
            return Err(SingleTapeAutomataError::ConflictingOutput {
                product,
                existing: output_state,
                incoming: output_state,
            });
        }

        if let Some(&existing) = self.prod_to_state_map.get(&product) {
            if existing != output_state {
                return Err(SingleTapeAutomataError::ConflictingOutput {
                    product,
                    existing,
                    incoming: output_state,
                });
            }
            return Ok(());
        }

        self.prod_to_state_map.insert(product, output_state);
        Ok(())
    }

    /// All (input) states referenced by the products in this map.
    pub fn get_states_set(&self) -> IndexSet<CellState> {
        let mut states_set: IndexSet<CellState> = IndexSet::new();
        for product in self.products() {
            for term in product.to_flat_terms() {
                states_set.insert(term.state);
            }
        }
        states_set
    }

    /// maps state -> products that produce it as their output.
    pub fn build_state_to_products_map(&self) -> IndexMap<CellState, Vec<Product>> {
        let mut state_to_products: IndexMap<CellState, Vec<Product>> = IndexMap::new();
        for (product, output_state) in self.iter() {
            state_to_products.entry(*output_state).or_default().push(product.copy());
        }
        state_to_products
    }

    /// maps state -> products that contain it in their input terms.
    pub fn build_input_state_to_prod_map(&self) -> IndexMap<CellState, Vec<Product>> {
        let mut input_state_to_prod: IndexMap<CellState, Vec<Product>> = IndexMap::new();
        for product in self.products() {
            for term in product.to_flat_terms() {
                input_state_to_prod.entry(term.state).or_default().push(product.copy());
            }
        }
        input_state_to_prod
    }
}

impl<'a> IntoIterator for &'a ProductWriteMap {
    type Item = (&'a Product, &'a CellState);
    type IntoIter = indexmap::map::Iter<'a, Product, CellState>;

    fn into_iter(self) -> Self::IntoIter {
        self.prod_to_state_map.iter()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WriteRecord {
    pub origin_product: Product,
    pub position: i64,
    pub cell_state: CellState,
}

impl WriteRecord {
    pub fn log(&self) {
        println!("{}", self);
    }
}

impl fmt::Display for WriteRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} | {} -> {}",
            self.origin_product._to_string("A"),
            self.position,
            self.cell_state
        )
    }
}

#[derive(Debug, Clone)]
pub struct ProcessStepResult {
    pub prev_tape: BidirectionalTape,
    pub new_tape: BidirectionalTape,
    pub active_writes: Vec<WriteRecord>,
}

pub struct SingleTapeAutomata {
    tape: BidirectionalTape,
    prod_to_state_map: ProductWriteMap,
    leftmost_extent: i64,  // this must be negative or zero
    rightmost_extent: i64,  // this must be positive or zero
    state_eq_map: IndexMap<CellState, Expression>,
}

impl SingleTapeAutomata {
    pub fn new(
        state_eq_map: IndexMap<CellState, Expression>,
    ) -> Result<SingleTapeAutomata, SingleTapeAutomataError> {
        let prod_to_state_map = Self::reverse_state_eq_map(&state_eq_map)?;
        let (leftmost_extent, rightmost_extent) = Self::compute_rule_range(&prod_to_state_map);

        Ok(SingleTapeAutomata {
            tape: BidirectionalTape::default(),
            prod_to_state_map,
            leftmost_extent,
            rightmost_extent,
            state_eq_map,
        })
    }

    pub fn get_tape(&self) -> &BidirectionalTape {
        &self.tape
    }

    pub fn get_tape_mut(&mut self) -> &mut BidirectionalTape {
        &mut self.tape
    }

    pub fn get_prod_to_state_map(&self) -> ProductWriteMap {
        self.prod_to_state_map.clone()
    }

    pub fn get_state_eq_map(&self) -> IndexMap<CellState, Expression> {
        self.state_eq_map.clone()
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

    fn compute_rule_range(prod_to_state_map: &ProductWriteMap) -> (i64, i64) {
        let mut leftmost_extent: i64 = 0;
        let mut rightmost_extent: i64 = 0;

        for product in prod_to_state_map.products() {
            for term in product.to_flat_terms() {
                leftmost_extent = leftmost_extent.min(term.position);
                rightmost_extent = rightmost_extent.max(term.position);
            }
        }

        assert!(leftmost_extent <= 0);
        assert!(rightmost_extent >= 0);
        (leftmost_extent, rightmost_extent)
    }

    pub fn read(&self, position: i64) -> CellState {
        self.tape.read(position)
    }

    pub fn write(&mut self, position: i64, value: CellState) {
        self.tape.write(position, value);
    }

    /// Populate the automata cells from `position` to `end_position`
    /// (inclusive) using `data` as a repeating pattern.
    pub fn write_region(
        &mut self, position: i64, end_position: i64, data: &[CellState],
    ) -> Result<(), SingleTapeAutomataError> {
        self.tape.write_region(position, end_position, data)?;
        Ok(())
    }

    /// `cell_width == None` is the `BLANK_INT` sentinel on the Python side.
    pub fn render_tape(
        &self, start_position: i64, length: usize,
        header_tag: &str, cell_width: Option<usize>,
    ) -> Result<RenderFrame, SingleTapeAutomataError> {
        let left_tab = format!("Tape {}: ", header_tag);
        let left_sidebar = RenderFrame::from_padded_lines(vec![left_tab]);
        let content_width = length.saturating_sub(left_sidebar.get_width());

        let tape_line = self.tape.render_line(start_position, content_width, cell_width)?;
        let num_cells = tape_line.num_cells;
        let tape_content_width = tape_line.get_space_consumed();

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

        let body = RenderFrame::join_horizontally(&[left_sidebar, (*tape_line).clone()])
            .map_err(TapeError::from)?;

        Ok(RenderFrame::join_vertically(&[
            RenderFrame::from_line(position_str),
            body,
        ])
            .map_err(TapeError::from)?)
    }

    /// Given a mapping from output states to expressions over input states,
    /// create a mapping of state products to the cell state they write to:
    ///
    /// `product -> output cell state`
    pub fn reverse_state_eq_map(
        state_eq_map: &IndexMap<CellState, Expression>,
    ) -> Result<ProductWriteMap, SingleTapeAutomataError> {
        let mut prod_to_state_map = ProductWriteMap::new();

        for (output_state, expr) in state_eq_map.iter() {
            for product in expr._get_products() {
                /*
                Whether a product transitions a contiguous region of void
                states into a non-void state. This can't be allowed because
                it would make the simulation range infinite.
                */
                let product_is_void =
                    product.to_flat_terms().iter().all(|term| term.state == VOID_STATE);
                let output_is_void = *output_state == VOID_STATE;

                if product_is_void && !output_is_void {
                    /*
                    Product transitions a contiguous region of void
                    to a non-void state. This can't be allowed because
                    it would make the simulation range infinite.
                    */
                    return Err(SingleTapeAutomataError::VoidProduct {
                        product: product.copy(),
                        output: *output_state,
                    });
                }

                prod_to_state_map.insert(product.copy(), *output_state)?;
            }
        }

        Ok(prod_to_state_map)
    }

    /// Check if the given product is satisfied at the given position.
    pub fn product_satisfies(&self, product: &Product, position: i64) -> bool {
        for term in product.to_flat_terms() {
            if self.tape.read(position + term.position) != term.state {
                return false;
            }
        }
        true
    }

    pub fn process_step(
        &self, log_active_writes: bool,
    ) -> Result<ProcessStepResult, SingleTapeAutomataError> {
        let (min_pos, max_pos) = self.tape.get_range();
        let mut new_tape = self.tape.clone();
        let scan_start = min_pos + self.leftmost_extent;
        let scan_end = max_pos + self.rightmost_extent + 1;

        let mut writes_map: HashMap<i64, CellState> = HashMap::new();
        let mut origins_map: HashMap<i64, BTreeSet<Product>> = HashMap::new();
        let mut active_writes: Vec<WriteRecord> = Vec::new();

        for position in scan_start..scan_end {
            let mut written = false;

            for (matching_product, output_state) in self.prod_to_state_map.iter() {
                if !self.product_satisfies(matching_product, position) {
                    continue;
                }

                let output_state = *output_state;
                let prev_write = writes_map.get(&position).copied().unwrap_or(output_state);

                if prev_write != output_state {
                    let previous_products = origins_map
                        .get(&position)
                        .map(|products| products.iter().cloned().collect::<Vec<Product>>())
                        .unwrap_or_default();

                    return Err(SingleTapeAutomataError::ConflictingWrite {
                        position,
                        product: matching_product.copy(),
                        previous: prev_write,
                        incoming: output_state,
                        previous_products,
                    });
                }

                let write_record = WriteRecord {
                    origin_product: matching_product.copy(),
                    position,
                    cell_state: output_state,
                };
                if log_active_writes {
                    write_record.log();
                }
                active_writes.push(write_record);
                writes_map.insert(position, output_state);
                origins_map
                    .entry(position)
                    .or_default()
                    .insert(matching_product.copy());

                new_tape.write(position, output_state);
                debug_assert_eq!(new_tape.read(position), output_state);
                written = true;
            }

            // copy over the unchanged cell if no rule matched here
            if !written {
                new_tape.write(position, self.tape.read(position));
            }
        }

        Ok(ProcessStepResult {
            prev_tape: self.tape.clone(),
            new_tape,
            active_writes,
        })
    }

    /// Set the new state of the tape after going forward a single step.
    /// The returned result also carries the previous tape state.
    pub fn step(&mut self, verbose: bool) -> Result<ProcessStepResult, SingleTapeAutomataError> {
        let process_result = self.process_step(verbose)?;
        self.tape = process_result.new_tape.clone();
        Ok(process_result)
    }
}
