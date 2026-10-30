import unittest

import tqdm

from automata_builder._rust import PyProduct
from automata_builder.compiled_counter_automata import (
    CachedCounterAutomataCompiler,
    CompiledCounterAutomataRunner,
    CounterAutomataSettings,
)
from automata_builder.counter_automata import SIGNALS_TAPE
from automata_builder.counter_states import from_counter_state


# Shared cache so compilation results are reused across tests.
_cache = CachedCounterAutomataCompiler()

NO_REDUCTION_BASES = list(range(2, 9))
REDUCTION_BASES = list(range(2, 4))


class TestCompiledTapesScriptBases(unittest.TestCase):
    """
    Mirrors ``python -m automata_builder.tests.run_compiled_tapes_test -b n``
    for every base n in 2..8 (inclusive), both with and without reduction.
    """

    # ------------------------------------------------------------------
    # helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _make_runner(
        base: int,
        apply_reduction: bool = False,
        write_start: int = 0,
        write_end: int = 20,
    ) -> CompiledCounterAutomataRunner:
        return CompiledCounterAutomataRunner(
            initial_write_start=write_start,
            initial_write_end=write_end,
            automata_settings=CounterAutomataSettings(
                base=base, apply_reduction=apply_reduction
            ),
            cache=_cache,
        )

    @staticmethod
    def _count_pause_incomparable_transitions(
        runner: CompiledCounterAutomataRunner,
    ) -> int:
        """Reproduce the pause-incomparability scan from the script."""
        num_pause_incomparable = 0
        for transition in runner.transitions_group.transitions:
            input_product = PyProduct(transition.input_terms)
            remap_res = runner.remap_prod_to_multi_tape(
                input_product=input_product
            )
            if remap_res.is_err():
                continue

            multi_term_product = remap_res.unwrap()
            counter_terms = [
                term
                for term in multi_term_product.get_flat_terms()
                if (
                    term.get_tape_no() == SIGNALS_TAPE
                    and term.get_cell_state() % 2 == 0
                    and term.get_cell_state() >= 4
                )
            ]
            paused_list = [
                from_counter_state(term.get_cell_state())[1]
                for term in counter_terms
            ]
            if len(set(paused_list)) > 1:
                num_pause_incomparable += 1

        return num_pause_incomparable

    # ------------------------------------------------------------------
    # compilation / structural checks (no reduction)
    # ------------------------------------------------------------------

    def test_runner_compiles_for_all_bases(self):
        for base in tqdm.tqdm(NO_REDUCTION_BASES, desc='compile'):
            with self.subTest(base=base):
                runner = self._make_runner(base=base, apply_reduction=False)
                self.assertGreater(runner.count_unique_states(), 0)
                self.assertGreater(
                    len(runner.transitions_group.transitions), 0
                )

    def test_runner_compiles_with_reduction_for_all_bases(self):
        for base in tqdm.tqdm(REDUCTION_BASES, desc='compile-reduction'):
            with self.subTest(base=base):
                runner = self._make_runner(base=base, apply_reduction=True)
                self.assertGreater(runner.count_unique_states(), 0)
                self.assertGreater(
                    len(runner.transitions_group.transitions), 0
                )

    # ------------------------------------------------------------------
    # simulation runs without errors (default settings)
    # ------------------------------------------------------------------

    def test_simulation_runs_for_all_bases(self):
        for base in tqdm.tqdm(NO_REDUCTION_BASES, desc='simulate'):
            with self.subTest(base=base):
                runner = self._make_runner(base=base, apply_reduction=False)
                # run_simulation with assert_consistency=True verifies
                # that the single-tape and multi-tape stay in sync.
                runner.run_simulation(
                    num_timesteps=30, render=False, assert_consistency=True
                )

    def test_simulation_runs_with_reduction_for_all_bases(self):
        for base in tqdm.tqdm(REDUCTION_BASES, desc='simulate-reduction'):
            with self.subTest(base=base):
                runner = self._make_runner(base=base, apply_reduction=True)
                runner.run_simulation(
                    num_timesteps=30, render=False, assert_consistency=True
                )

    # ------------------------------------------------------------------
    # convergence checks
    # ------------------------------------------------------------------

    def test_full_counter_convergence_for_all_bases(self):
        """
        Without reduction, after >= 4*cells_filled steps the signals
        tape should encode exactly cells_filled.
        """
        write_start, write_end = 0, 20
        cells_filled = write_end - write_start + 1  # 21
        steps = cells_filled * 4

        for base in tqdm.tqdm(REDUCTION_BASES, desc='convergence'):
            with self.subTest(base=base, cells_filled=cells_filled):
                runner = self._make_runner(
                    base=base,
                    apply_reduction=False,
                    write_start=write_start,
                    write_end=write_end,
                )
                runner.run_simulation(
                    num_timesteps=steps, render=False,
                    assert_consistency=True
                )
                encoded = runner.read_signals_tape_value().unwrap()
                self.assertEqual(
                    encoded,
                    cells_filled,
                    msg=(
                        f"base={base}: expected {cells_filled} "
                        f"after {steps} steps, got {encoded}"
                    ),
                )

    def test_half_counter_convergence_for_all_bases(self):
        """
        With reduction, after >= 2*cells_filled steps the signals
        tape should encode (cells_filled + 1) // 2.
        """
        write_start, write_end = 0, 20
        cells_filled = write_end - write_start + 1  # 21
        expected_value = (cells_filled + 1) // 2      # 11
        steps = cells_filled * 2

        for base in tqdm.tqdm(
            REDUCTION_BASES, desc='convergence-reduction'
        ):
            with self.subTest(base=base, cells_filled=cells_filled):
                runner = self._make_runner(
                    base=base,
                    apply_reduction=True,
                    write_start=write_start,
                    write_end=write_end,
                )
                runner.run_simulation(
                    num_timesteps=steps, render=False, assert_consistency=True
                )
                encoded = runner.read_signals_tape_value().unwrap()
                self.assertEqual(
                    encoded,
                    expected_value,
                    msg=(
                        f"base={base}: expected {expected_value} "
                        f"after {steps} steps, got {encoded}"
                    ),
                )

    # ------------------------------------------------------------------
    # step-by-step consistency (every step checked)
    # ------------------------------------------------------------------

    def test_step_by_step_consistency_for_all_bases(self):
        for base in tqdm.tqdm(
            NO_REDUCTION_BASES, desc='step-consistency'
        ):
            with self.subTest(base=base):
                runner = self._make_runner(base=base, apply_reduction=False)
                for _ in range(10):
                    runner.step(verbose=False, assert_consistency=True)

    def test_step_by_step_consistency_with_reduction(self):
        for base in tqdm.tqdm(
            REDUCTION_BASES, desc='step-consistency-reduction'
        ):
            with self.subTest(base=base):
                runner = self._make_runner(base=base, apply_reduction=True)
                for _ in range(10):
                    runner.step(verbose=False, assert_consistency=True)


if __name__ == "__main__":
    unittest.main()
