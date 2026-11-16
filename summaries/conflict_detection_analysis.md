# 3SAT Automata: Variable Assignment Conflict Detection — Implementation Summary

## Summary

The 3SAT cellular automata now detects conflicting variable assignments
(same variable assigned different truth values in different clause blocks)
and returns a `CONFLICTING` verdict. All 11 tests pass, including 2 new
tests specifically for conflict detection.

## Problem

The original 3SAT automata rule set did not verify that the assignment
vector was non-contradictory. The TODO comment in `sat_automata.py` noted:
"THE AUTOMATA RULES DO NOT IMPLEMENT THE MOST IMPORTANT PART WHICH IS
VERIFYING THAT VAR ASSIGNMENTS LAID OUT IN TAPE ARE NON-CONFLICTING."

A conflict is: for some variable at offset `i`, the value at position
`k·num_vars + i` differs from the value at position `k'·num_vars + i`
for some `k ≠ k'`. This is a **non-local** comparison (cells are
`num_vars` apart) that cannot be directly expressed with the local
3-cell neighborhood of the universal rule set.

## Solution: Redefined Encoding (Option C)

The key insight: **rearrange the assignment vector so that all values for
the same variable are adjacent**. A conflict then becomes two **adjacent**
cells with different truth values, which is locally detectable.

### Implementation Details

1. **New tape**: `CONFLICT_TAPE = TapeNo(5)` — carries the rearranged
   assignment vector.

2. **Layout**: For each variable (in sorted order), its values across all
   clause blocks are contiguous, separated by `CF_SEP` markers.

   Example (3 variables, 2 clauses, assignment x1=T, x2=F, x3=T):
   ```
   Position: 0     1     2     3     4     5     6     7
   Value:    x1=T  x1=T   SEP  x2=F  x2=F   SEP  x3=T  x3=T
   ```

   A conflict (e.g., x1=T in block 0, x1=F in block 1) would appear as:
   ```
   Position: 0     1     2     3     4     5     6     7
   Value:    x1=T  x1=F   SEP  x2=F  x2=F   SEP  x3=T  x3=T
              ^^^^  ^^^^
              conflict: adjacent cells with different values
   ```

3. **Conflict detection rules** (local and universal):
   - If `CONFLICT(MID) = AT_TRUE` and `CONFLICT(RIGHT) = AT_FALSE`,
     write `CF_CONFLICTING` to the CONFLICT tape.
   - If `CONFLICT(MID) = AT_FALSE` and `CONFLICT(RIGHT) = AT_TRUE`,
     write `CF_CONFLICTING` to the CONFLICT tape.

4. **Propagation**: `CF_CONFLICTING` propagates leftward one cell per step
   until it reaches position -1, where the verdict rule reads it.

5. **Verdict rules**:
   - If `VERDICT(-1) = VT_PENDING` and `CONFLICT(-1) = CF_CONFLICTING`,
     write `VT_CONFLICTING` to the VERDICT tape.
   - Otherwise, the original SAT/UNSAT verdict rules apply.

6. **Verdict**: If `VERDICT(-1) = VT_CONFLICTING`, the verdict is
   `CONFLICTING`.

### Why This Works

- **Local**: The conflict detection rule only looks at MID and RIGHT.
- **Universal**: The rule does not depend on `num_variables` or the formula.
- **Correct**: A conflict (same variable, different values in different
  blocks) becomes two adjacent cells with different truth values.
- **Complete**: If there is no conflict, the CONFLICT tape has no adjacent
  cells with different truth values, so no conflict is detected.

### Timing

- The conflict propagation takes at most `num_cells` steps (from the
  rightmost conflict position to -1).
- The scan takes `num_cells + 1` steps.
- So the conflict reaches -1 before the scan completes, ensuring the
  conflict verdict takes precedence.

## Files Modified

- `automata_builder/sat_automata.py`:
  - Added `CONFLICT_TAPE`, `CF_SEP`, `CF_CONFLICTING`, `VT_CONFLICTING`
    states.
  - Added `build_conflict_transitions_group` method.
  - Modified `build_verdict_transitions_group` to check for conflicts.
  - Modified `_initialize_tapes` to populate the CONFLICT tape.
  - Modified `read_verdict` to return `CONFLICTING`.

- `unittests/test_sat_automata.py`:
  - Added `test_conflicting_assignment_is_conflicting`.
  - Added `test_conflicting_assignment_takes_precedence_over_sat`.

## Test Results

All 11 tests pass (9 existing + 2 new):

```
unittests/test_sat_automata.py::TestThreeSATAutomata::test_clause_blocks_hold_one_cell_per_variable PASSED
unittests/test_sat_automata.py::TestThreeSATAutomata::test_clause_with_variable_and_its_negation_is_satisfied PASSED
unittests/test_sat_automata.py::TestThreeSATAutomata::test_conflicting_assignment_is_conflicting PASSED
unittests/test_sat_automata.py::TestThreeSATAutomata::test_conflicting_assignment_takes_precedence_over_sat PASSED
unittests/test_sat_automata.py::TestThreeSATAutomata::test_invalid_assignment_is_unsat PASSED
unittests/test_sat_automata.py::TestThreeSATAutomata::test_invalid_equation_is_unsat PASSED
unittests/test_sat_automata.py::TestThreeSATAutomata::test_missing_variable_assignment_is_unsat PASSED
unittests/test_sat_automata.py::TestThreeSATAutomata::test_repeated_variable_polarities_share_a_cell PASSED
unittests/test_sat_automata.py::TestThreeSATAutomata::test_satisfiable_equation_assignment_pair PASSED
unittests/test_sat_automata.py::TestThreeSATAutomata::test_transition_rules_are_input_independent PASSED
unittests/test_sat_automata.py::TestThreeSATAutomata::test_unsatisfiable_equation_assignment_pair PASSED
```

## Notes

- The conflict check is **redundant for correctly-encoded inputs** because
  `parse_assignment` rejects duplicate variable names and `encode_3sat`
  replicates the assignment consistently across all blocks.
- The check is useful for **defensive programming** and for detecting
  corrupted or hand-crafted tape data.
- The rule set remains **universal** (input-independent), as verified by
  `test_transition_rules_are_input_independent`.
