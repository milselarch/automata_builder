import unittest

from automata_builder.sat_automata import (
    ASSIGNMENTS_TAPE,
    CF_FALSE,
    CF_SETTLED,
    CF_TRUE,
    CLAUSES_TAPE,
    CONFLICT_TAPE,
    CT_CLAUSE_START,
    DATA_START_POSITION,
    LITERALS_TAPE,
    LT_EITHER,
    VERDICT_TAPE,
    VT_CONFLICTING,
    VT_PENDING,
    VT_SAT,
    VT_UNSAT,
    ThreeSATAutomataBuilder,
    ThreeSATAutomataRunner,
    encode_3sat,
    parse_3sat_equation,
    parse_assignment,
)
from automata_builder.rule_generator_multitape import (
    MultiTapeState,
    TapeCellState,
)


class TestThreeSATAutomata(unittest.TestCase):
    def test_satisfiable_equation_assignment_pair(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|~x2|x3)&(~x1|x2|x3)',
            assignment='x1=1,x2=0,x3=1',
        )
        runner.run_simulation(render=False)
        self.assertEqual(runner.read_verdict_state(), VT_SAT)
        self.assertTrue(runner.is_satisfiable())

    def test_unsatisfiable_equation_assignment_pair(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|x2|x3)&(~x1|~x2|~x3)',
            assignment='x1=1,x2=1,x3=1',
        )
        runner.run_simulation(render=False)
        self.assertEqual(runner.read_verdict_state(), VT_UNSAT)
        self.assertFalse(runner.is_satisfiable())

    def test_clause_with_variable_and_its_negation_is_satisfied(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|~x1|x2)&(x2|x2|x1)',
            assignment='x1=1,x2=0',
        )
        runner.run_simulation(render=False)
        self.assertEqual(runner.read_verdict_state(), VT_SAT)

    def test_invalid_equation_is_unsat(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|x2)&(x3|~x4|x5)',
            assignment='x1=1,x2=1,x3=0,x4=0,x5=1',
        )
        runner.run_simulation(render=False)
        self.assertEqual(runner.read_verdict_state(), VT_UNSAT)

    def test_invalid_assignment_is_unsat(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|x2|x3)',
            assignment='x1=1,x2=maybe,x3=0',
        )
        runner.run_simulation(render=False)
        self.assertEqual(runner.read_verdict_state(), VT_UNSAT)

    def test_missing_variable_assignment_is_unsat(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|x2|x3)',
            assignment='x1=1,x2=0',
        )
        runner.run_simulation(render=False)
        self.assertEqual(runner.read_verdict_state(), VT_UNSAT)

    def test_transition_rules_are_input_independent(self):
        """
        The transition rules are universal, so building them for
        different equation and assignment pairs (or for input that
        cannot be encoded at all) must give the exact same ruleset
        """
        base_rules = ThreeSATAutomataBuilder().build_transitions_group()
        runners = [
            ThreeSATAutomataRunner(
                equation='(x1|~x2|x3)&(~x1|x2|x3)',
                assignment='x1=1,x2=0,x3=1',
            ),
            ThreeSATAutomataRunner(
                equation='(a|b|c)&(~a|~b|~c)&(a|~b|c)',
                assignment='a=0,b=1,c=1',
            ),
            ThreeSATAutomataRunner(
                equation='(x1|x2)', assignment='x1=1,x2=0'
            ),
        ]

        for runner in runners:
            self.assertEqual(
                runner.transitions_group.transitions,
                base_rules.transitions
            )

    def test_clause_blocks_hold_one_cell_per_variable(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|~x2|x3)&(x1|x2|~x3)',
            assignment='x1=1,x2=0,x3=1',
        )

        literals_region = runner.multi_tape_automata[
            LITERALS_TAPE
        ].get_minimal_data_region()
        assignments_region = runner.multi_tape_automata[
            ASSIGNMENTS_TAPE
        ].get_minimal_data_region()
        clauses_region = runner.multi_tape_automata[
            CLAUSES_TAPE
        ].get_minimal_data_region()

        # 2 clause blocks of 3 variable cells each
        self.assertEqual(len(literals_region), 6)
        self.assertEqual(len(assignments_region), 6)
        self.assertEqual(
            clauses_region,
            [CT_CLAUSE_START, 0, 0, CT_CLAUSE_START]
        )

    def test_repeated_variable_polarities_share_a_cell(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|~x1|x2)', assignment='x1=1,x2=0'
        )

        literals_region = runner.multi_tape_automata[
            LITERALS_TAPE
        ].get_minimal_data_region()
        # one cell per variable, and x1 occurs with both polarities
        self.assertEqual(len(literals_region), 2)
        self.assertEqual(literals_region[0], LT_EITHER)

    def test_duplicate_assignment_is_not_rejected_by_the_parser(self):
        """
        Whether an assignment that names a variable twice is usable has to
        be decided by the automata rules, so the parser must accept it
        """
        parsed = parse_assignment('x1=1,x1=0,x2=0')
        self.assertIsNotNone(parsed)
        self.assertEqual(parsed.occurrences('x1'), (True, False))
        self.assertTrue(parsed.is_conflicting('x1'))
        self.assertFalse(parsed.is_conflicting('x2'))

        runner = ThreeSATAutomataRunner(
            equation='(x1|x2|x3)', assignment='x1=1,x1=0,x2=0,x3=0'
        )
        self.assertFalse(runner.invalid_input)
        self.assertIsNotNone(runner.encoding)

    def test_conflicting_assignment_is_rejected_by_the_rules(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|x1|x1)', assignment='x1=1,x1=0'
        )
        runner.run_simulation(render=False)

        self.assertFalse(runner.invalid_input)
        self.assertEqual(runner.read_verdict_state(), VT_CONFLICTING)
        self.assertTrue(runner.conflicting_assignment)
        self.assertFalse(runner.is_satisfiable())
        self.assertEqual(runner.read_verdict(), 'UNSAT')

    def test_conflict_overrides_satisfied_equation(self):
        """
        x1=1 would satisfy the equation, but the automata cannot evaluate
        a variable that was given two values, so the conflict wins
        """
        satisfied = ThreeSATAutomataRunner(
            equation='(x1|x1|x1)', assignment='x1=1'
        )
        satisfied.run_simulation(render=False)
        self.assertEqual(satisfied.read_verdict_state(), VT_SAT)

        conflicting = ThreeSATAutomataRunner(
            equation='(x1|x1|x1)', assignment='x1=1,x1=0'
        )
        conflicting.run_simulation(render=False)
        self.assertEqual(conflicting.read_verdict_state(), VT_CONFLICTING)

    def test_agreeing_duplicate_assignment_is_not_conflicting(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|~x2|x3)&(~x1|x2|x3)',
            assignment='x1=1,x1=true,x2=0,x3=1',
        )
        runner.run_simulation(render=False)

        self.assertFalse(runner.conflicting_assignment)
        self.assertEqual(runner.encoding.conflicting_variables, ())
        self.assertEqual(runner.read_verdict_state(), VT_SAT)

    def test_conflict_is_detected_wherever_it_sits_on_the_tape(self):
        """
        The consistency check may not rely on knowing how many variables
        or clause blocks there are, so it has to find a disagreement in
        the first, a middle, and the last variable of the tape
        """
        equation = (
            '(x1|x2|x3)&(~x1|~x2|x3)&(x1|~x2|~x3)&(~x1|x2|~x3)'
        )
        for assignment, variable in (
            ('x1=0,x1=1,x2=0,x3=0', 'x1'),
            ('x1=0,x2=0,x2=1,x3=0', 'x2'),
            ('x1=0,x2=0,x3=1,x3=0', 'x3'),
        ):
            runner = ThreeSATAutomataRunner(
                equation=equation, assignment=assignment
            )
            runner.run_simulation(render=False)
            self.assertEqual(
                runner.read_verdict_state(), VT_CONFLICTING, assignment
            )
            self.assertEqual(
                runner.encoding.conflicting_variables, (variable,), assignment
            )

    def test_consistency_check_runs_on_tape_data_not_on_the_parser(self):
        """
        The rules have to verify the replication itself, so a value that is
        overwritten on the conflict tape must be caught even though the
        assignment the runner was given is perfectly consistent
        """
        runner = ThreeSATAutomataRunner(
            equation='(x1|x2|x3)&(~x1|x2|x3)',
            assignment='x1=1,x2=0,x3=1',
        )
        self.assertEqual(runner.encoding.conflicting_variables, ())

        # one cell per clause block, so the group of x2 sits at positions
        # 2 and 3, and its second cell is overwritten with the opposite
        # of what the assignment says
        self.assertEqual(runner.read_conflict_state(3), CF_FALSE)
        runner.multi_tape_automata.write_region(
            position=3, end_position=3,
            data=[MultiTapeState(
                tape_no=CONFLICT_TAPE, tape_cell_state=TapeCellState(
                    int(CF_TRUE)
                )
            )]
        )
        runner.run_simulation(render=False)

        self.assertFalse(runner.invalid_input)
        self.assertEqual(runner.read_verdict_state(), VT_CONFLICTING)
        self.assertTrue(runner.conflicting_assignment)

    def test_consistent_replication_is_marked_settled(self):
        runner = ThreeSATAutomataRunner(
            equation='(x1|x2|x3)&(~x1|x2|x3)',
            assignment='x1=1,x2=0,x3=1',
        )
        runner.run_simulation(render=False)

        self.assertFalse(runner.conflicting_assignment)
        self.assertEqual(runner.read_conflict_state(DATA_START_POSITION),
                         CF_SETTLED)
        self.assertEqual(runner.read_verdict_state(), VT_SAT)

    def test_verdict_is_pending_before_the_decision_timestep(self):
        """
        The verdict has to stay pending until both the scan and the
        consistency sweep have run past the verdict cell, and then has to
        stay at the value it resolved to
        """
        for equation, assignment in (
            ('(x1|x2|x3)&(~x1|~x2|~x3)', 'x1=1,x2=1,x3=1'),
            ('(x1|x2|x3)&(~x1|x2|x3)', 'x1=1,x2=0,x3=1'),
            ('(x1|x1|x1)', 'x1=1,x1=0'),
            ('(x1|x2)', 'x1=1,x2=0'),
        ):
            runner = ThreeSATAutomataRunner(
                equation=equation, assignment=assignment
            )
            required = runner.get_required_timesteps()
            resolved_at: int | None = None
            resolved: TapeCellState | None = None

            for timestep in range(required + 3):
                if timestep:
                    runner.step()

                state = runner.read_verdict_state()
                if resolved_at is None:
                    if state != VT_PENDING:
                        resolved_at = timestep
                        resolved = state
                else:
                    self.assertEqual(
                        state, resolved,
                        f'{equation} {assignment} changed its mind at '
                        f'timestep {timestep}'
                    )

            self.assertIsNotNone(resolved_at, assignment)
            self.assertLess(resolved_at, required, assignment)

    def test_conflict_tape_holds_one_group_per_variable(self):
        parsed_equation = parse_3sat_equation(
            '(x1|x2|x3)&(~x1|~x2|x3)&(x1|~x2|~x3)'
        )
        encoding = encode_3sat(
            parsed_equation, parse_assignment('x1=1,x2=0,x3=1')
        )
        # 3 clause blocks, 3 variables: 9 replication cells
        self.assertEqual(encoding.num_clauses, 3)
        self.assertEqual(encoding.num_conflict_cells, 9)
        self.assertEqual(encoding.conflicting_variables, ())

        conflicting = encode_3sat(
            parsed_equation,
            parse_assignment('x1=1,x2=0,x3=1,x2=0,x2=0,x2=0')
        )
        # x2 now has four occurrences, so its group grows to 4 cells
        self.assertEqual(conflicting.num_conflict_cells, 10)
        self.assertEqual(conflicting.conflicting_variables, ())

        conflicting = encode_3sat(
            parsed_equation, parse_assignment('x1=1,x2=0,x3=1,x2=1')
        )
        self.assertEqual(conflicting.conflicting_variables, ('x2',))


if __name__ == '__main__':
    unittest.main()
