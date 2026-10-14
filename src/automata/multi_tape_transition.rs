use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;
use indexmap::{IndexMap, IndexSet};
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use crate::automata::py_terms_multitape::PyMultiTapeProduct;
use crate::automata::rule_generator::{HALT_STATE, VOID_STATE};
use crate::automata::rule_generator_multitape::AutomataError;
use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{
    MultiTapeExpression, MultiTapeProduct, MultiTapeProductFactory,
    MultiTapeState, MultiTapeTerm, TapeNo
};

pub type TransitionPriority = u32;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MultiTapeTransition {
    pub input_terms: Vec<MultiTapeTerm>,
    pub output_state: MultiTapeState,
    pub annotation: String,
    pub priority: TransitionPriority,
}
impl MultiTapeTransition {
    pub fn new(
        input_terms: Vec<MultiTapeTerm>,
        output_state: MultiTapeState,
        annotation: String,
        priority: TransitionPriority,
    ) -> Self {
        Self {
            input_terms,
            output_state,
            annotation,
            priority,
        }
    }
    pub fn to_product(&self) -> MultiTapeProduct {
        MultiTapeProductFactory::new(self.input_terms.clone())
            .with_annotation(self.annotation.clone())
            .to_product()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AddTransitionError {
    AnnotationExpected,
    AllTermsAreVoid,
    HasHaltState,
    MultipleTransitionPriorities {
        existing_transitions: Vec<MultiTapeTransition>,

    }
}

impl fmt::Display for AddTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct MultiTapeTransitionsGroup {
    /*
    contains a set of transitions for a multi-tape cellular automaton
    defined as a mapping from input states to output state
    map D[] -> (output tape_no, output state)
    */
    pub transitions: Vec<MultiTapeTransition>,
    pub prod_to_transitions: HashMap<PyMultiTapeProduct, HashSet<MultiTapeTransition>>,
    pub prod_priority_map: HashMap<MultiTapeProduct, TransitionPriority>,
    pub require_annotation: bool,
}

impl MultiTapeTransitionsGroup {
    pub fn new(require_annotation: bool) -> Self {
        Self {
            transitions: Vec::new(),
            require_annotation,
        }
    }

    pub fn len(&self) -> usize {
        self.transitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }

    pub fn get_all_states(&self) -> IndexSet<MultiTapeState> {
        let mut all_states: IndexSet<MultiTapeState> = IndexSet::new();

        for transition in self.transitions.iter() {
            for term in transition.input_terms.iter() {
                all_states.insert(MultiTapeState::from_term(term.clone()));
            }
            all_states.insert(transition.output_state.clone());
        }
        all_states
    }

    pub fn add_transition(
        &mut self,
        input_terms: Vec<MultiTapeTerm>,
        output_tape_no: TapeNo,
        output_cell_state: CellState,
        validate_void: bool,
        validate_halt: bool,
        annotation: String,
        priority: TransitionPriority,
    ) -> Result<(), AddTransitionError> {
        /*
        param input_terms:
        param output_tape_no:
        param output_cell_state:
        param validate_void:
        If true, check that the input terms do not all have a void state
        :param validate_halt:
        If true, check that the halt state is not within input terms
        :param annotation:
        :param priority:
        :return:
         */
        if self.require_annotation && annotation.is_empty() {
            return Err(AddTransitionError::AnnotationExpected)
        }
        if validate_void {
            let is_all_void = input_terms.iter().all(|term| {
                let (_, cell_state) = term.state;
                cell_state == VOID_STATE
            });
            if is_all_void {
                return Err(AddTransitionError::AllTermsAreVoid);
            }
        }
        if validate_halt {
            for term in &input_terms {
                let (_, cell_state) = term.state;
                let has_halt = cell_state == HALT_STATE;
                if has_halt {
                    return Err(AddTransitionError::HasHaltState);
                }
            }
        }
        let output_state = MultiTapeState::new(output_tape_no, output_cell_state);
        let transition = MultiTapeTransition::new(
            input_terms, output_state, annotation, priority
        );
        println!("TRANSITION_ADD {:?}", transition);
        self._insert_transition(transition);
        Ok(())
    }

    pub fn _insert_transition(
        &mut self, transition: MultiTapeTransition
    ) -> Result<(), > {
        let product = transition.to_product();
        let existing_priority = self.prod_priority_map.get(&product)
            .unwrap_or(&transition.priority);

        if (existing_priority != transition.priority) {

        }
        self.transitions.push(transition);
    }

    pub fn or(
        &self, other: &MultiTapeTransitionsGroup
    ) -> Result<MultiTapeTransitionsGroup, String> {
        let require_annotation = self.require_annotation || other.require_annotation;

        if require_annotation {
            if !self.require_annotation {
                return Err(
                    "Cannot combine transitions while other group does not require annotation"
                        .to_string(),
                );
            }
            if !other.require_annotation {
                return Err(
                    "Cannot combine transitions while own group requires annotation".to_string(),
                );
            }
        }

        let mut combined = MultiTapeTransitionsGroup::new(require_annotation);
        combined.transitions.extend(self.transitions.clone());
        combined.transitions.extend(other.transitions.clone());

        Ok(combined)
    }

    pub fn generate_equations(
        &self, require_annotations: bool
    ) -> Result<
        IndexMap<MultiTapeState, MultiTapeExpression>,
        AutomataError
    > {
        let mut state_eq_terms_map: IndexMap<
            MultiTapeState, Vec<MultiTapeProduct>
        > = IndexMap::new();

        for transition in &self.transitions {
            let input_terms = transition.input_terms.clone();
            let output_state = transition.output_state.clone();
            let annotation = transition.annotation.clone();

            let product = MultiTapeProductFactory::new(input_terms)
                .with_annotation(annotation)
                .to_product();

            state_eq_terms_map
                .entry(output_state.clone())
                .or_insert_with(Vec::new)
                .push(product);

            if require_annotations {
                // Assuming MultiTapeProduct has a method `get_annotation()`
                let product = state_eq_terms_map.get(&output_state).unwrap().last().unwrap();
                if product.get_annotation().is_empty() {
                    return Err(AutomataError::EmptyAnnotation {
                        product: product.clone()
                    });
                }
            }
        }

        let mut state_eq_map: IndexMap<
            MultiTapeState, MultiTapeExpression
        > = IndexMap::new();

        for next_state in state_eq_terms_map.keys() {
            let expr = MultiTapeExpression::new(state_eq_terms_map[next_state].clone());
            state_eq_map.insert(next_state.clone(), expr);
        }

        // Final validation of equations
        for (next_state, expr) in &state_eq_map {
            let flat_products = expr._get_products(); // Assuming this method exists
            if flat_products.is_empty() {
                return Err(AutomataError::NoProductsForState {
                    state: next_state.clone()
                });
            }

            if require_annotations {
                for product in flat_products {
                    // Assuming PyMultiTapeProduct has a method `get_annotation()`
                    if product.get_annotation().is_empty() {
                        return Err(AutomataError::EmptyAnnotation {
                            product: product.clone()
                        });
                    }
                }
            }
        }

        Ok(state_eq_map)
    }

    pub fn build_prod_priority_map(&self) -> IndexMap<
        MultiTapeProduct, TransitionPriority
    > {
        let mut prod_priority_map: IndexMap<
            MultiTapeProduct, TransitionPriority
        > = IndexMap::new();

        for transition in &self.transitions {
            let input_terms = transition.input_terms.clone();
            let annotation = transition.annotation.clone();
            let product = MultiTapeProductFactory::new(input_terms)
                .with_annotation(annotation)
                .to_product();

            let priority = transition.priority;
            let prev_opt = prod_priority_map.insert(product, priority);

            match prev_opt {
                None => {}
                Some(prev_priority) => {
                    // TODO: return error, or raise more useful error message
                    assert_eq!(prev_priority, priority)
                }
            }
        }

        prod_priority_map
    }
}
