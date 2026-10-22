use pyo3::prelude::*;
use crate::automata::py_multi_tape_automata::PyMultiTapeAutomata;
use crate::automata::py_multi_tape_builder::{
    PyCompileTapesResult, PyMultiTapeBuilder, PyMultiTapeStatePathRemap,
    PyTapeOverlaps, PyTapeOverlapsFSM
};
use crate::automata::py_multi_tape_transition::{
    PyMultiTapeTransition, PyMultiTapeTransitionsGroup
};
use crate::automata::py_rule_generator_multitape::{
    PyBiDirectionalMultiTape, PyBidirectionalTape,
    PyMultiTapeDataRegion, PyMultiTapeState, PyProcessStepResult,
    PyRenderFrame, PyWriteRecord
};
use crate::automata::py_single_tape_automata::{
    PySingleTapeAutomata, PySingleTapeProcessStepResult,
    PySingleTapeWriteRecord
};
use crate::automata::py_terms::{A, PyProduct, PyExpression};
use crate::automata::py_terms_multitape::{
    PyMultiTapeExpression, PyMultiTapeProduct, D
};

pub mod automata;

/// Native (rust) extension module of the `automata_builder` python package.
#[pymodule]
fn _rust(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("debug")
    ).try_init();

    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("=== RUST PANIC ===");
        eprintln!("{}", panic_info);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }));

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

    module.add_class::<PyMultiTapeTransition>()?;
    module.add_class::<PyMultiTapeTransitionsGroup>()?;

    module.add_class::<PySingleTapeWriteRecord>()?;
    module.add_class::<PySingleTapeProcessStepResult>()?;
    module.add_class::<PySingleTapeAutomata>()?;
    module.add_class::<PyMultiTapeDataRegion>()?;

    module.add_class::<PyTapeOverlaps>()?;
    module.add_class::<PyTapeOverlapsFSM>()?;
    module.add_class::<PyMultiTapeStatePathRemap>()?;
    module.add_class::<PyCompileTapesResult>()?;
    module.add_class::<PyMultiTapeBuilder>()?;
    Ok(())
}
