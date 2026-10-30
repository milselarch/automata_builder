# `automata_builder`

Tooling to construct and simulate 1D cellular automata.

The automata code is split into two importable libraries:
- a python package - `automata_builder` (pure Python code plus the
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

### Multi-tape to single-tape compiler
Transitions are declared in python (`counter_automata.py`) and the
compilation is orchestrated from python (`compiled_counter_automata.py`),
but the `compile_tapes` pipeline itself
(tape overlaps FSM, product extension, state path remapping and
transition generation) runs in rust:
```python
from automata_builder.rule_generator_multitape import (
    MultiTapeBuilder, MultiTapeState
)

builder = MultiTapeBuilder(multi_tape_automata=multi_tape_automata)
builder.declare_initial_group_overlaps(
    overlap_states={MultiTapeState(tape_no=1, tape_cell_state=2)}
)
result = builder.compile_tapes()  # python CompileTapesResult
raw_result = builder.compile_tapes_rust()  # rust PyCompileTapesResult
```
- `MultiTapeBuilder` wraps `automata_builder._rust.PyMultiTapeBuilder`
  (`src/automata/py_multi_tape_builder.rs`) and converts its result to the
  python `CompileTapesResult`. The rust implementation lives in
  `src/automata/multi_tape_builder.rs`, `tape_overlaps.rs`,
  `tape_overlaps_fsm.rs` and `multi_tape_product_trie.rs`
- `PythonMultiTapeBuilder` is the original pure python implementation,
  kept as a (much slower) reference

### Automata Builder
1. Run test scripts for automata builder like as follows
   - Make sure to run `python -m pip install -e .` to install 
     `automata_builder` as an editable package first
   - Run `python -m automata_builder.tests.compile_tapes_test` to
     execute `automata_builder/tests/compile_tapes_test.py`
2. Execute counter automata unittests with
   - `python -m unittest discover -s unittests`

### Lean definitions (Aeneas)
[Aeneas](https://github.com/AeneasVerif/aeneas) is used to translate the rust
crate into Lean definitions (in `lean/`) that can be used to reason about /
verify the automata code. The translation is done in two steps:
1. [Charon](https://github.com/AeneasVerif/charon) compiles the crate into
   LLBC (`target/aeneas/automata_builder.llbc`)
2. Aeneas translates the LLBC into Lean (`lean/AutomataBuilder/*.lean`)

To regenerate the Lean definitions after changing the rust code, run
```shell
./scripts/generate_lean.sh
```
and commit the changes in `lean/`. CI (`.github/workflows/lean-generation.yml`)
runs the same script and fails if it produces any git diff.

The script (Linux x86_64 / aarch64 or macOS arm64; needs `rustup` and `curl`):
- downloads the prebuilt Aeneas + Charon binaries of the pinned Aeneas
  release (`AENEAS_RELEASE` in the script) into `.aeneas/`. Set
  `AENEAS_DIR` to use an existing install instead
- installs the nightly rust toolchain Charon was built for (with the
  `rustc-dev`, `llvm-tools`, `rust-src` and `miri` components)
- runs Charon on the pure rust modules only - the pyo3 bindings (`py_*`
  modules), serde derives, `Iterator::{copied,cloned}` (which crash
  Aeneas) and rayon (whose mutually recursive traits Aeneas does not
  support) are excluded
- runs Aeneas with the Lean backend, `-split-files`, `-loops-to-rec`
  (loops become recursive functions, which Lean can prove monotone inside
  mutually recursive definitions) and `-filter-trait-methods` (trait impls
  only fill in the methods modelled by the Aeneas Lean library)

Generated files in `lean/AutomataBuilder/`:
- `Types.lean`, `Funs.lean` - translated types and functions
- `TypesExternal_Template.lean`, `FunsExternal_Template.lean` - `axiom`s
  for definitions which are not part of the Aeneas standard library
  (e.g. `HashMap`, `IndexMap`, `BTreeMap`, `fmt`) and for functions whose
  bodies Aeneas could not translate
- `TypesExternal.lean`, `FunsExternal.lean` - hand-maintained models of the
  external definitions. They are only seeded from the templates when
  missing, so update them by hand when the templates change. `HashMap`,
  `IndexMap` and `Arc` are modelled as structures (instead of axioms) since
  recursive rust types such as `MultiTapeProductTrie` nest through them,
  which the Lean kernel only accepts for inductive types

To type-check the generated definitions (needs
[elan](https://github.com/leanprover/elan); `lean/lake-manifest.json` pins
the Aeneas Lean library and its Mathlib dependency):
```shell
cd lean
lake exe cache get  # download prebuilt Mathlib
lake build AutomataBuilder.Types AutomataBuilder.FunsExternal  # types
lake build  # everything, including Funs.lean
```
CI requires the types and external models to build. `Funs.lean` does not
fully type-check yet, so CI builds it on a best-effort basis
(`continue-on-error`).

Aeneas only supports a subset of rust, so some functions are translated as
axioms (Aeneas reports them as errors in `target/aeneas/aeneas.log`). Notably
`break`/`continue` to outer loops and `return`s inside nested loops are not
supported yet. The remaining `Funs.lean` type errors come from the Aeneas
Lean backend / standard library, e.g. `?` inside loops (`ControlFlow`
projections), the missing `Iterator` instance for `slice::IterMut`,
`Option::Some` used as a function (`.map(Some)`), and local variables named
`automata` shadowing the generated `automata.*` namespace.

To upgrade Aeneas, update `AENEAS_RELEASE` in `scripts/generate_lean.sh`
(a tag from https://github.com/AeneasVerif/aeneas/releases), the `rev` in
`lean/lakefile.toml` (the script checks they match) and `lean/lean-toolchain`
(copy `backends/lean/lean-toolchain` from the release), then regenerate.

## TODO
- design counter automata that operates within data range
- add unittest for single-tape compiled automata
- implement web automata visualizer tool

# DONE
- interactive TUI automata player (`automata-tui`, run with `cargo run` inside
  `automata-tui/` - see [automata-tui/README.md](automata-tui/README.md))
- multi-tape to single-tape cellular automata compiler
- port cellular automata simulator to rust
- port multi-tape to single-tape compiler (`compile_tapes`) to rust
