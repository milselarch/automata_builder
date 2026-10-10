use std::fmt;
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use crate::automata::rule_generator::{HALT_STATE, VOID_STATE};
use crate::automata::terms::CellState;
use crate::automata::terms_multitape::{MultiTapeTerm, TapeNo};

// Pure Rust structs (no PyO3 attributes)
#[derive(Clone, Debug)]
pub struct MultiTapeState {
    pub tape_no: TapeNo,
    pub tape_cell_state: CellState,
}

impl MultiTapeState {
    pub fn new(tape_no: TapeNo, tape_cell_state: CellState) -> Self {
        Self {
            tape_no,
            tape_cell_state,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MultiTapeTransition {
    pub input_terms: Vec<MultiTapeTerm>,
    pub output_state: MultiTapeState,
    pub annotation: String,
    pub priority: i32,
}

impl MultiTapeTransition {
    pub fn new(
        input_terms: Vec<MultiTapeTerm>,
        output_state: MultiTapeState,
        annotation: String,
        priority: i32,
    ) -> Self {
        Self {
            input_terms,
            output_state,
            annotation,
            priority,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AddTransitionError {
    AnnotationExpected,
    AllTermsAreVoid,
    HasHaltState
}

impl fmt::Display for AddTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct MultiTapeTransitionsGroup {
    pub transitions: Vec<MultiTapeTransition>,
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

    pub fn add_transition(
        &mut self,
        input_terms: Vec<MultiTapeTerm>,
        output_tape_no: TapeNo,
        output_cell_state: CellState,
        validate_void: bool,
        validate_halt: bool,
        annotation: String,
        priority: i32,
    ) -> Result<(), AddTransitionError> {
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
        self.transitions.push(transition);
        Ok(())
    }

    pub fn or(&self, other: &MultiTapeTransitionsGroup) -> Result<MultiTapeTransitionsGroup, String> {
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
}
