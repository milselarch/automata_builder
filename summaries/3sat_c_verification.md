# 3SAT Automata: C Reference-Implementation Verification

## Task
Re-implement the 3SAT cellular automata (5 tapes, 87 transitions) in C, and
verify it against the Rust multi-tape engine (`PyMultiTapeAutomata`).

## What was built

- **`automata_builder/lean/C/mtasim_c.c`** — a self-contained C engine (~350
  lines, no external dependencies) that:
  - Loads transitions and initial tape state from JSON
  - Runs the parallel multi-tape CA simulation (clone-then-write semantics,
    identical to the Rust `process_step`)
  - Detects conflicting writes (two rules writing different values to the same
    `(tape, position)` in the same step) — matching the Rust engine's
    `ConflictingWrite` error
  - Reports the verdict cell state after N steps
  - Supports `TRACE=1` env to print the scan-tape and verdict per step

- **`automata_builder/lean/C/Makefile`** — build rule

## Key semantics matched to Rust

| Aspect | Rust | C |
|--------|------|---|
| Clone-then-write | `new_multi_tape` deep-clones before writes | `set_cell_direct` applied after scan |
| Conflicting writes | `Err(ConflictingWrite)` if different values | `fprintf(stderr,...); exit(1)` |
| Unconstrained cells | Cells not written retain prior value | `get_cell` returns stored value (0 if unset) |
| Void/halt | VOID=0, HALT=1 | Same constants |
| Term matching | `D(position, tape, state)`, subset constraint | Same, `get_cell(tape, pos+offset)` |

## Verification results

All 6 unit-test cases from `unittests/test_sat_automata.py` pass:

| Test | C verdict | Rust verdict | Match |
|------|-----------|--------------|-------|
| Satisfiable (x1=0,x2=0,x3=0) | SAT | SAT | OK |
| Unsatisfiable (x1=0,x2=1,x3=1) | UNSAT | UNSAT | OK |
| Clause with both polarities | SAT | SAT | OK |
| Invalid equation | UNSAT | UNSAT | OK |
| Invalid assignment | UNSAT | UNSAT | OK |
| Missing variable | UNSAT | UNSAT | OK |

Additionally verified:
- **Input-independence**: the transition set is identical for all
  equation/assignment pairs (same 87 rules). Only the initial tape state
  changes.
- **Scan-head trajectory**: the scan accumulator spawns at the rightmost cell,
  travels leftward one cell per step, and the verdict is written at
  position -1 after `num_cells + 1` steps — identical to the Rust trace.
- **Conflict-freedom**: no `ConflictingWrite` errors in any case, confirming
  the rule set is well-formed.

## How to run

```bash
cd automata_builder/lean/C
make
./mtasim trans.json tapes.json <num_steps> <verdict_tape> <verdict_pos>
```

JSON format for transitions:
```json
[{"input_terms": [[tape, offset, state], ...],
  "out_tape": int, "out_state": int, "annotation": str}, ...]
```

JSON format for initial tapes:
```json
{"num_tapes": N, "tapes": [{pos: state, ...}, ...]}
```
