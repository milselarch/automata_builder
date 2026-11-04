# Refactor plan: annotation String -> AnnotationID int

## Task
- Replace `annotation: String` on transition & product types (rust + python) with an integer annotation ID
- New global `AnnotationMapping`: insert(new) -> new ID, lookup(existing) -> ID
- Implement as rust struct, expose via pyo3 class
- Python reference implementation should use the pyo3 class (not a pure-python one)

## Baseline benchmark (BEFORE)
`python -m automata_builder.tests.run_compiled_tapes_test -a -b 2`
- run1: wall 13.79 s
- run2: wall 13.90 s
- run3: wall 13.95 s
- "Simulation ran in 0.02 seconds" — most time is startup + tape compilation

## TODO
- [ ] survey all `annotation` usages
- [ ] find where product structs live with annotation (PyProduct)
- [ ] implement AnnotationMapping in src/automata/annotation_mapping.rs
- [ ] pyo3 class PyAnnotationMapping w/ insert + lookup
- [ ] change MultiTapeTransition.annotation: String -> usize id
- [ ] update python: Product/TapeTransition classes to store int id
- [ ] rebuild with maturin (develop), rerun tests + benchmarks
- [ ] gitignore: notes.md summary.md *.uuid
- [ ] summary.md w/ findings
