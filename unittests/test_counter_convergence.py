import random
import unittest
import tqdm

from automata_builder.counter_automata import CounterAutomataRunner


class TestCounterAutomataConvergence(unittest.TestCase):
    @staticmethod
    def _run_steps_and_read(
        base: int,
        write_start: int,
        write_end: int,
        steps: int,
    ) -> int:
        runner = CounterAutomataRunner(
            base=base,
            initial_write_start=write_start,
            initial_write_end=write_end,
        )
        runner.run_simulation(num_timesteps=steps, render=False)
        return runner.read_signals_tape_value()

    def test_encoded_value_equals_cells_filled_after_2n_steps(
        self, num_tests: int = 100, seed: int = 42
    ) -> None:
        random.seed(seed)
        pbar = tqdm.tqdm(range(num_tests))

        for _ in pbar:
            write_start = random.choice(range(-100, 100))
            write_end = write_start + random.choice(range(0, 50))
            base = random.choice(range(2, 9))

            pbar.set_description(
                f'{write_start=} {write_end=} {base=}'
            )

            cells_filled = write_end - write_start + 1
            steps = cells_filled * 4

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
                    cells_filled,
                    msg=(
                        f"Expected encoded value {cells_filled} "
                        f"after {steps} steps "
                        f"(>= 2*cells_filled={steps}), got {encoded}."
                    ),
                )


if __name__ == "__main__":
    unittest.main()
