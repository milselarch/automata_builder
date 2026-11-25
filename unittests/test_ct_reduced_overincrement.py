import unittest

from automata_builder.counter_automata import CounterAutomataRunner


class TestCTReducedOverincrement(unittest.TestCase):
    """
    Regression tests for the over-incrementing bug in
    ``build_ct_reducer_transitions_group`` (the
    ``apply_reduction=True, collapse_reduction_tape=True`` path, i.e. the
    reduced counter automaton whose reducer tape is folded into the carry
    tape).

    Root cause
    ----------
    In the collapsed reducer the carry tape holds the reducer band, which
    contains BOTH ``REDUCER_DATA`` and ``REDUCER_PAUSED_DATA`` cells (the
    latter arise from the pause/unpause mechanism that makes the band travel
    at half speed). The counter's left-shift (``SHL``) and rightmost-clear
    (``CLEAR_RIGHTMOST``) rules key on the carry-tape cell under the head,
    but were only generated for ``carry_tape_state=REDUCER_DATA``. When the
    head's old position sat on a ``REDUCER_PAUSED_DATA`` cell, neither the
    VOID rule nor the ``REDUCER_DATA`` rule fired, so the old head position
    was never cleared. The digit was then duplicated as the head moved left,
    corrupting the encoded value (and, once two digits carried inconsistent
    pause states, making ``read_signals_tape_value`` raise).

    The fix loops the ``SHL``/``CLEAR_RIGHTMOST`` generation over BOTH
    reducer-band states. These tests guard against reintroducing the bug and
    against the (explicitly required) introduction of any new tape.
    """

    # (base, cells_filled) pairs that exercise the reducer band. A variety of
    # cell counts is used so that the head's old position lands on a
    # REDUCER_PAUSED_DATA cell during the leftward sweep (the exact condition
    # that triggered the original bug).
    CASES = [
        (2, 4), (2, 10), (2, 12), (2, 20),
        (3, 4), (3, 10), (3, 12), (3, 20),
        (4, 4), (4, 10), (4, 12), (4, 20),
        (6, 4), (6, 10), (6, 12), (6, 16), (6, 20), (6, 24), (6, 30),
    ]

    @staticmethod
    def _run_and_read(base: int, cells_filled: int) -> int:
        """Run the collapsed-reducer automaton for 2*cells steps and read the
        encoded value. The original bug made this raise (inconsistent paused
        counter digits), so a successful read is itself part of the
        regression check."""
        write_start = 0
        write_end = cells_filled - 1
        runner = CounterAutomataRunner(
            base=base,
            initial_write_start=write_start,
            initial_write_end=write_end,
            apply_reduction=True,
            collapse_reduction_tape=True,
        )
        runner.run_simulation(num_timesteps=2 * cells_filled, render=False)
        return runner.read_signals_tape_value().unwrap()

    def test_collapsed_reducer_encodes_half_cells_after_2n_steps(self) -> None:
        for base, cells_filled in self.CASES:
            with self.subTest(base=base, cells_filled=cells_filled):
                expected_value = (cells_filled + 1) // 2
                encoded = self._run_and_read(base=base, cells_filled=cells_filled)
                self.assertEqual(
                    encoded,
                    expected_value,
                    msg=(
                        f"base={base}, cells_filled={cells_filled}: "
                        f"collapsed reducer encoded {encoded} after "
                        f"2*cells={2 * cells_filled} steps, expected "
                        f"{expected_value}. (Over-incrementing regression: "
                        f"the head's old position was not cleared when it "
                        f"sat on a REDUCER_PAUSED_DATA carry cell.)"
                    ),
                )

    def test_collapsed_reducer_matches_separate_reducer(self) -> None:
        """The collapsed reducer must agree with the (known-good) separate
        reducer tape for the same input."""
        for base, cells_filled in self.CASES[:6]:
            with self.subTest(base=base, cells_filled=cells_filled):
                expected_value = (cells_filled + 1) // 2

                # separate reducer tape (known-good reference)
                ref_runner = CounterAutomataRunner(
                    base=base,
                    initial_write_start=0,
                    initial_write_end=cells_filled - 1,
                    apply_reduction=True,
                    collapse_reduction_tape=False,
                )
                ref_runner.run_simulation(
                    num_timesteps=2 * cells_filled, render=False
                )
                reference = ref_runner.read_signals_tape_value().unwrap()

                # collapsed reducer tape
                collapsed = self._run_and_read(
                    base=base, cells_filled=cells_filled
                )

                self.assertEqual(reference, expected_value)
                self.assertEqual(
                    collapsed,
                    reference,
                    msg=(
                        f"base={base}, cells_filled={cells_filled}: "
                        f"collapsed reducer ({collapsed}) disagrees with "
                        f"separate reducer ({reference})."
                    ),
                )

    def test_no_new_tape_introduced(self) -> None:
        """Hard requirement: the over-incrementing bugfix must not introduce
        any new tape. The collapsed reducer must still use exactly 3 tapes
        (data, signals, carry), while the separate-reducer variant uses 4
        (data, signals, carry, reducer)."""
        for base, cells_filled in [(4, 10)]:
            with self.subTest(base=base, cells_filled=cells_filled):
                collapsed_runner = CounterAutomataRunner(
                    base=base,
                    initial_write_start=0,
                    initial_write_end=cells_filled - 1,
                    apply_reduction=True,
                    collapse_reduction_tape=True,
                )
                collapsed_tapes = collapsed_runner.multi_tape_automata.get_tape_nos()

                separate_runner = CounterAutomataRunner(
                    base=base,
                    initial_write_start=0,
                    initial_write_end=cells_filled - 1,
                    apply_reduction=True,
                    collapse_reduction_tape=False,
                )
                separate_tapes = separate_runner.multi_tape_automata.get_tape_nos()

                self.assertEqual(
                    len(collapsed_tapes),
                    3,
                    msg=(
                        f"collapsed reducer uses {len(collapsed_tapes)} tapes "
                        f"{collapsed_tapes}; expected exactly 3 (no new tape "
                        f"introduced by the over-incrementing bugfix)."
                    ),
                )
                self.assertEqual(
                    len(separate_tapes),
                    4,
                    msg=(
                        f"separate reducer uses {len(separate_tapes)} tapes "
                        f"{separate_tapes}; expected exactly 4."
                    ),
                )
                # the collapsed tape set must be a strict subset of the
                # separate tape set (the reducer tape is folded into carry).
                self.assertTrue(
                    set(collapsed_tapes) < set(separate_tapes),
                    msg=(
                        f"collapsed tape set {collapsed_tapes} is not a "
                        f"strict subset of separate tape set {separate_tapes}."
                    ),
                )


if __name__ == "__main__":
    unittest.main()
