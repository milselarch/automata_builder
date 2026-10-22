use pyo3_stub_gen::Result;
use automata_builder::automata::{
    py_multi_tape_builder, py_multi_tape_transition, py_rule_generator_multitape,
    py_single_tape_automata, py_terms, py_terms_multitape
};

fn main() -> Result<()> {
    // `stub_info` is a function defined by `define_stub_info_gatherer!` macro.
    py_terms::stub_info()?.generate()?;
    py_terms_multitape::stub_info()?.generate()?;
    py_rule_generator_multitape::stub_info()?.generate()?;
    py_single_tape_automata::stub_info()?.generate()?;
    py_multi_tape_transition::stub_info()?.generate()?;
    py_multi_tape_builder::stub_info()?.generate()?;
    Ok(())
}
