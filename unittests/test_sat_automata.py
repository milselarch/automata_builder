import unittest

from automata_builder.sat_automata import (
    ASSIGNMENTS_TAPE,
    AT_FALSE,
    AT_TRUE,
    CLAUSES_TAPE,
    CONFLICT_TAPE,
    CF_CONFLICTING,
    CT_CLAUSE_START,
    LITERALS_TAPE,
    LT_EITHER,
    VT_CONFLICTING,
    VT_SAT,
    VT_UNSAT,
    ThreeSATAutomataBuilder,
    ThreeSATAutomataRunner,
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

    def test_conflicting_assignment_is_conflicting(self):
        """A conflicting assignment vector (same variable, different values
        in different clause blocks) should be detected as CONFLICTING.
        """
        runner = ThreeSATAutomataRunner(
            equation='(x1|x2|x3)&(~x1|~x2|x3)',
            assignment='x1=1,x2=0,x3=0',
        )
        # Manually set up a conflict on the CONFLICT tape:
        # position 0 = AT_TRUE, position 1 = AT_FALSE
        # This simulates x1 being assigned true in one block and false in another.
        runner.multi_tape_automata.write_region(
            position=0, end_position=1,
            data=[
                MultiTapeState(
                    tape_no=CONFLICT_TAPE,
                    tape_cell_state=TapeCellState(int(AT_TRUE))
                ),
                MultiTapeState(
                    tape_no=CONFLICT_TAPE,
                    tape_cell_state=TapeCellState(int(AT_FALSE))
                ),
            ]
        )
        runner.run_simulation(render=False)
        self.assertEqual(runner.read_verdict_state(), VT_CONFLICTING)
        self.assertEqual(runner.read_verdict(), 'CONFLICTING')
        self.assertFalse(runner.is_satisfiable())

    def test_conflicting_assignment_takes_precedence_over_sat(self):
        """Even if the formula is satisfiable, a conflicting assignment
        vector should be detected as CONFLICTING.
        """
        runner = ThreeSATAutomataRunner(
            equation='(x1|x2|x3)',
            assignment='x1=1,x2=0,x3=0',
        )
        # Set up a conflict
        runner.multi_tape_automata.write_region(
            position=0, end_position=1,
            data=[
                MultiTapeState(
                    tape_no=CONFLICT_TAPE,
                    tape_cell_state=TapeCellState(int(AT_TRUE))
                ),
                MultiTapeState(
                    tape_no=CONFLICT_TAPE,
                    tape_cell_state=TapeCellState(int(AT_FALSE))
                ),
            ]
        )
        runner.run_simulation(render=False)
        self.assertEqual(runner.read_verdict_state(), VT_CONFLICTING)
        self.assertEqual(runner.read_verdict(), 'CONFLICTING')


if __name__ == '__main__':
    unittest.main()
