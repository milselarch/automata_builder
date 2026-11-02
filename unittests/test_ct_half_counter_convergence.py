"""
Tests for the cellular automata ruleset created by
`CounterAutomataBuilder.build_ct_reduced_transitions_group`
(automata_builder/counter_automata.py), i.e. the half-counter
(exponential-bits -> linear-bits) ruleset in which the reducer family
of states is collapsed onto the carry tape
(REDUCER_DATA = 0b011 / REDUCER_PAUSED_DATA = 0b100) instead of
having its own dedicated reducer tape.

Covers both:
1. the structure of the produced `PyMultiTapeTransitionsGroup`
   (rules, annotations, tape references)
2. the observed behaviour when a `CounterAutomataRunner` is
   constructed with `apply_reduction=True` and
   `collapse_reduction_tape=True` (which uses exactly this ruleset)
"""

import random
import unittest

from collections import Counter

from automata_builder.counter_automata import (
    CounterAutomataBuilder,
    CounterAutomataRunner,
)
from automata_builder.counter_states import (
    DATA_TAPE,
    SIGNALS_TAPE,
    CARRY_TAPE,
    REDUCER_TAPE,
    REDUCER_DATA,
    REDUCER_PAUSED_DATA,
    CT_DATA,
)
from automata_builder.rule_generator_multitape import VOID_STATE

# the four reducer core rules exist inside the ruleset as plain
# annotation strings
REDUCER_SPAWN_LEFT_END = 'REDUCER_SPAWN_LEFT_END'
REDUCER_DATA_SPREAD_RIGHT = 'REDUCER_DATA_SPREAD_RIGHT'
REDUCER_DATA_SPREAD_LEFT = 'REDUCER_DATA_SPREAD_LEFT'
REDUCER_PAUSE_TO_UNPAUSE = 'REDUCER_PAUSE_TO_UNPAUSE'
REDUCER_CORE_RULES = (
    REDUCER_SPAWN_LEFT_END,
    REDUCER_DATA_SPREAD_RIGHT,
    REDUCER_DATA_SPREAD_LEFT,
    REDUCER_PAUSE_TO_UNPAUSE,
)
REDUCER_FAMILY_STATES = {REDUCER_DATA, REDUCER_PAUSED_DATA}


class TestBuildCTReducedTransitionsGroup(unittest.TestCase):
    """
    pure ruleset (transition-group) tests - no simulation involved
    """

    def test_builds_nonempty_transitions_group(self):
        for base in range(2, 9):
            with self.subTest(base=base):
                ct_group = (
                    CounterAutomataBuilder(base=base)
                    .build_ct_reduced_transitions_group()
                )
                self.assertGreater(len(ct_group.transitions), 0)

    def test_all_transitions_have_annotations(self):
        # the group is constructed with require_annotation=True
        for base in range(2, 9):
            with self.subTest(base=base):
                ct_group = (
                    CounterAutomataBuilder(base=base)
                    .build_ct_reduced_transitions_group()
                )
                for transition in ct_group.transitions:
                    self.assertTrue(
                        transition.annotation,
                        f'found unannotated transition '
                        f'with priority {transition.priority}',
                    )

    def test_no_transition_references_reducer_tape(self):
        # in the collapsed ruleset every rule must be expressible with
        # only data / signals / carry tapes: the reducer family of
        # states now lives on the carry tape
        allowed_tapes = {DATA_TAPE, SIGNALS_TAPE, CARRY_TAPE}
        for base in range(2, 9):
            with self.subTest(base=base):
                ct_group = (
                    CounterAutomataBuilder(base=base)
                    .build_ct_reduced_transitions_group()
                )
                for transition in ct_group.transitions:
                    for term in transition.input_terms:
                        self.assertIn(
                            term.get_tape_no(), allowed_tapes,
                            f'rule {transition.annotation} references '
                            f'dedicated reducer tape {term.get_tape_no()}'
                        )
                    self.assertIn(
                        transition.output_state.tape_no, allowed_tapes,
                        f'rule {transition.annotation} writes to '
                        f'tape {transition.output_state.tape_no}'
                    )

    def test_reducer_core_rules_collapse_onto_carry_tape(self):
        # the four reducer spawn/spread/unpause rules exist in both the
        # `build_reduced_transitions_group` group and the
        # `build_ct_reduced_transitions_group` group; in the ct group
        # every input term that targeted the dedicated reducer tape
        # must target the carry tape instead, while the (data / signals
        # tape) input terms and the output state are unchanged
        for base in range(2, 6):
            with self.subTest(base=base):
                builder = CounterAutomataBuilder(base=base)
                reduced_group = builder.build_reduced_transitions_group()
                ct_group = builder.build_ct_reduced_transitions_group()

                for annotation in REDUCER_CORE_RULES:
                    reduced_rule = next(
                        t for t in reduced_group.transitions
                        if t.annotation == annotation
                    )
                    ct_rule = next(
                        t for t in ct_group.transitions
                        if t.annotation == annotation
                    )

                    self.assertEqual(
                        len(reduced_rule.input_terms),
                        len(ct_rule.input_terms),
                        f'rule {annotation} changed input arity',
                    )
                    for reduced_term, ct_term in zip(
                        reduced_rule.input_terms, ct_rule.input_terms
                    ):
                        if reduced_term.get_tape_no() == REDUCER_TAPE:
                            # collapsed onto the carry tape, same
                            # relative position and cell state
                            self.assertEqual(
                                ct_term.get_tape_no(), CARRY_TAPE,
                                f'rule {annotation}: reducer-tape input '
                                f'term not moved to the carry tape',
                            )
                            self.assertEqual(
                                reduced_term.get_position(),
                                ct_term.get_position(),
                            )
                            self.assertEqual(
                                reduced_term.get_cell_state(),
                                ct_term.get_cell_state(),
                            )
                        else:
                            # other-tape input terms are untouched
                            self.assertEqual(
                                reduced_term.get_state(),
                                ct_term.get_state(),
                                f'rule {annotation}: other-tape input '
                                f'term changed',
                            )
                            self.assertEqual(
                                reduced_term.get_tape_no(),
                                ct_term.get_tape_no(),
                            )

                    if reduced_rule.output_state.tape_no == REDUCER_TAPE:
                        self.assertEqual(
                            ct_rule.output_state.tape_no, CARRY_TAPE,
                        )
                        self.assertEqual(
                            reduced_rule.output_state.tape_cell_state,
                            ct_rule.output_state.tape_cell_state,
                        )
                    else:
                        self.assertEqual(
                            reduced_rule.output_state,
                            ct_rule.output_state,
                        )

    def test_ct_group_is_reduced_group_plus_carry_reducer_rules(self):
        # the ct ruleset is the plain reduced ruleset plus:
        # - the no-carry-over shift rules (one per mid x right digit
        #   combination) acting on REDUCER_DATA cells on the carry tape
        # - the clear-rightmost rules (two per digit) also acting on
        #   REDUCER_DATA cells on the carry tape
        for base in range(2, 9):
            with self.subTest(base=base):
                builder = CounterAutomataBuilder(base=base)
                reduced_group = builder.build_reduced_transitions_group()
                ct_group = builder.build_ct_reduced_transitions_group()

                extra_rules = base * base + 2 * base
                self.assertEqual(
                    len(ct_group.transitions),
                    len(reduced_group.transitions) + extra_rules,
                )

                extra_annotations = [
                    t.annotation for t in ct_group.transitions
                    if f'OVER_{REDUCER_DATA}' in t.annotation
                ]
                self.assertEqual(len(extra_annotations), extra_rules)

                self.assertEqual(
                    sum(
                        a.startswith('SHL_')
                        for a in extra_annotations
                    ),
                    base * base,
                )
                self.assertEqual(
                    sum(
                        a.startswith('CLEAR_RIGHTMOST_')
                        for a in extra_annotations
                    ),
                    2 * base,
                )

    def test_runner_uses_exact_ruleset_when_fully_collapsing(
        self
    ):
        # CounterAutomataRunner(apply_reduction=True,
        # collapse_reduction_tape=True) must use precisely the group
        # returned by build_ct_reduced_transitions_group
        for base in (2, 3, 5, 8):
            with self.subTest(base=base):
                builtin_group = (
                    CounterAutomataBuilder(base=base)
                    .build_ct_reduced_transitions_group()
                )
                runner = CounterAutomataRunner(
                    base=base,
                    initial_write_start=0,
                    initial_write_end=1,
                    apply_reduction=True,
                    collapse_reduction_tape=True,
                )

                builtin_annotations = Counter(
                    t.annotation
                    for t in builtin_group.transitions
                )
                runner_annotations = Counter(
                    t.annotation
                    for t in runner.transitions_group.transitions
                )
                self.assertEqual(builtin_annotations, runner_annotations)

                # fully collapsed implies exactly three tapes
                self.assertEqual(
                    list(runner.init_tapes),
                    [DATA_TAPE, SIGNALS_TAPE, CARRY_TAPE],
                )


class TestCTHalfCounterConvergence(unittest.TestCase):
    """
    behavioural tests: the collapsed ruleset must still compute the
    half-counter - i.e. the signals-tape value read after
    `2 * cells_filled` full steps must equal `ceil(cells_filled / 2)`
    """

    def test_encoded_value_equals_cells_filled_ceil_halves_after_cells_filled_steps(self):
        """
        run the half-counter on an initially filled data tape with
        `initial_write_start <= initial_write_end` and no initial
        signals-tape or carry-tape cells, with the reducer family
        collapsed onto the carry tape - the encoded value on the
        signals-tape after `2 * cells_filled` full steps should be
        equal to the half-rounded `cells_filled` (ceil(cells_filled / 2))
        """
        for _ in range(15):
            # set the base and cell-fill-count
            base = random.randint(2, 8)
            cells_filled = random.randint(2, 50)
            # randomize initial cell-fill start
            initial_write_start = random.randint(-10, 10)

            # run the half-counter with the fully collapsed (ct) ruleset
            runner = CounterAutomataRunner(
                base=base,
                initial_write_start=initial_write_start,
                initial_write_end=initial_write_start + cells_filled,
                apply_reduction=True,
                collapse_reduction_tape=True,
            )
            # run until the automaton "settles"
            runner.run_simulation(
                num_timesteps=2 * cells_filled,
                render=False
            )

            # verify state is encoded on the signals-tape
            encoded_value = runner.read_signals_tape_value().unwrap()
            # verify encoded value matches expected half-rounded value
            expected_value = (cells_filled + 1) // 2
            self.assertEqual(
                encoded_value, expected_value,
                f'base={base} cells_filled={cells_filled} '
                f'start={initial_write_start}: '
                f'got {encoded_value}, expected {expected_value}'
            )

    def test_no_residue_on_carry_tape_after_settling(self):
        """
        after `2 * cells_filled` full steps, cells that remain on the
        carry tape may only be void, counter-carry (CT_DATA) or
        reducer-family (REDUCER_DATA / REDUCER_PAUSED_DATA) states
        """
        allowed_states = REDUCER_FAMILY_STATES | {VOID_STATE, CT_DATA}
        for _ in range(10):
            base = random.randint(2, 6)
            cells_filled = random.randint(4, 40)
            initial_write_start = random.randint(-5, 5)

            runner = CounterAutomataRunner(
                base=base,
                initial_write_start=initial_write_start,
                initial_write_end=initial_write_start + cells_filled,
                apply_reduction=True,
                collapse_reduction_tape=True,
            )
            runner.run_simulation(
                num_timesteps=2 * cells_filled,
                render=False
            )

            carry_tape = runner.multi_tape_automata[CARRY_TAPE]
            data_region = carry_tape.get_minimal_data_region()
            for cell_state in data_region:
                self.assertIn(
                    cell_state, allowed_states,
                    f'unexpected carry-tape state {cell_state} '
                    f'after settling (base={base})'
                )


if __name__ == '__main__':
    unittest.main()
