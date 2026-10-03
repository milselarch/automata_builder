import random
import unittest
import tqdm

from automata_builder.compiled_counter_automata import (
    CompiledCounterAutomataRunner, CounterAutomataSettings,
    CachedCounterAutomataCompiler
)

_cache = CachedCounterAutomataCompiler()


class TestHalfCounterAutomataConvergence(unittest.TestCase):
    """
    The half-reducer automata is designed to transform t unary data cells
    into encoded cells with a value of (x+1)//2 in base n and in 2*t time,
    and such that the encoded n-ary is contained with the same
    position range as the original data cells

    (and in fact the left end of the encoded n-ary is at the same
    position as the left end of the original unary data cell range)
    """
    @staticmethod
    def _run_steps_and_read(
        base: int,
        write_start: int,
        write_end: int,
        steps: int,
    ) -> int:
        runner = CompiledCounterAutomataRunner(
            initial_write_start=write_start,
            initial_write_end=write_end,
            cache=_cache,
            automata_settings=CounterAutomataSettings(
                base=base, apply_reduction=True
            )
        )
        runner.run_simulation(num_timesteps=steps, render=False)
        return runner.read_signals_tape_value()

    def test_encoded_value_equals_half_cells_filled_after_2n_steps(
            self, num_tests: int = 100, seed: int = 42
    ) -> None:
        random.seed(seed)
        pbar = tqdm.tqdm(range(num_tests))

        for _ in pbar:
            write_start = random.choice(range(-100, 100))
            write_end = write_start + random.choice(range(0, 50))
            base = random.choice(range(2, 4))

            pbar.set_description(
                f'{write_start=} {write_end=} {base=}'
            )

            cells_filled = write_end - write_start + 1
            expected_value = (cells_filled + 1) // 2
            steps = cells_filled * 2

            with self.subTest(
                base=base,
                write_start=write_start,
                write_end=write_end,
                cells_filled=cells_filled,
                steps=steps,
            ):
                encoded = self._run_steps_and_read(
                    base=base,
                    write_start=write_start,
                    write_end=write_end,
                    steps=steps,
                )
                self.assertEqual(
                    encoded,
                    expected_value,
                    msg=(
                        f"Expected encoded value {expected_value} "
                        f"after {steps} steps "
                        f"(>= 2*cells_filled={steps}), got {encoded}."
                    ),
                )


if __name__ == "__main__":
    unittest.main()
