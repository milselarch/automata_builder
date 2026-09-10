# `automata_builder`

Tooling to construct and simulate 1D cellular automata.

The automata code is split into two importable libraries:
- a python package - `automata_builder` (pure python code plus the
  compiled `automata_builder._rust` extension module)
- a rust crate - `automata_builder` (the `automata_builder::automata` module)

Goals of this project:
1. Compile multi-tape cellular automata down to single-tape cellular automata
2. Compose cellular automata together from smaller cellular automata
3. There should be a fixed position or range of positions from
   which it can be determined whether the simulated program has halted

# Setup
Rust is required to build the native extension module.
1. Create a python3.12 virtual environment - `python3.12 -m venv venv`
2. Activate the virtual environment - `source venv/bin/activate`
3. Install python dependencies - `pip install -r requirements.txt`
4. Install this project as an editable package - `python -m pip install -e .`
   - This builds the rust extension module (`automata_builder._rust`) with
     [maturin](https://www.maturin.rs/) and installs the `automata_builder`
     python package
5. Regenerate the pyO3 type stubs (`automata_builder/_rust.pyi`) after changing
   any of the rust bindings - `cargo run --bin stub_gen`

## Usage

As a python package:
```python
from automata_builder.rule_generator import RuleGenerator
from automata_builder._rust import A, PyProduct, PyExpression
```

As a rust crate, add it as a dependency in `Cargo.toml`:
```toml
[dependencies]
automata_builder = { git = "https://github.com/milselarch/automata_builder" }
```
```rust
use automata_builder::automata::single_tape_automata::SingleTapeAutomata;
```

### Automata Builder
1. Run test scripts for automata builder like as follows
   - Make sure to run `python -m pip install -e .` to install 
     `automata_builder` as an editable package first
   - Run `python -m automata_builder.tests.compose_tapes_test` to
     execute `automata_builder/tests/compose_tapes_test.py`
2. Execute counter automata unittests with
   - `python -m unittest discover -s unittests`

## TODO
- design counter automata that operates within data range
- add unittest for single-tape compiled automata
- implement web automata visualizer tool
- implement TUI for automata visualization

# DONE
- multi-tape to single-tape cellular automata compiler
- port cellular automata simulator to rust
