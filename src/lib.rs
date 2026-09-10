use pyo3::prelude::*;
use crate::automata::py_rule_generator_multitape::{
    PyBiDirectionalMultiTape, PyBidirectionalTape, PyMultiTapeAutomata, PyMultiTapeState,
    PyProcessStepResult, PyRenderFrame, PyWriteRecord,
};
use crate::automata::py_single_tape_automata::{
    PySingleTapeAutomata, PySingleTapeProcessStepResult, PySingleTapeWriteRecord
};
use crate::automata::py_terms::{A, PyProduct, PyExpression};
use crate::automata::py_terms_multitape::{PyMultiTapeExpression, PyMultiTapeProduct, D};

pub mod automata;

/// Native (rust) extension module of the `automata_builder` python package.
#[pymodule]
fn _rust(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<A>()?;
    module.add_class::<PyProduct>()?;
    module.add_class::<PyExpression>()?;

    module.add_class::<D>()?;
    module.add_class::<PyMultiTapeProduct>()?;
    module.add_class::<PyMultiTapeExpression>()?;

    module.add_class::<PyMultiTapeState>()?;
    module.add_class::<PyRenderFrame>()?;
    module.add_class::<PyBidirectionalTape>()?;
    module.add_class::<PyBiDirectionalMultiTape>()?;
    module.add_class::<PyWriteRecord>()?;
    module.add_class::<PyProcessStepResult>()?;
    module.add_class::<PyMultiTapeAutomata>()?;

    module.add_class::<PySingleTapeWriteRecord>()?;
    module.add_class::<PySingleTapeProcessStepResult>()?;
    module.add_class::<PySingleTapeAutomata>()?;
    Ok(())
}
