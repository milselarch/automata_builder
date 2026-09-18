import argparse

from automata_builder.composed_counter_automata import (
    ComposedCounterAutomataRunner
)
from automata_builder.rule_generator import BLANK_INT

"""
python -m automata_builder.tests.run_composed_tapes_test
python -m automata_builder.tests.run_composed_tapes_test -n 2
python -m automata_builder.tests.run_composed_tapes_test -b 6
python -m automata_builder.tests.run_composed_tapes_test -a -b 6
"""
parser = argparse.ArgumentParser(
    description='Run the counter automata simulation.'
)
parser.add_argument(
    '--base', '-b',
    type=int,
    default=2,
    help='Numerical base for the counter automata (default: 2)'
)
parser.add_argument(
    '--write-start', '-s',
    type=int,
    default=0,
    help='Starting position to write the initial data (default: 0)'
)
parser.add_argument(
    '--write-end', '-e',
    type=int,
    default=20,
    help='Ending position to write the initial data (default: 20)'
)
parser.add_argument(
    '--timesteps', '-t',
    type=int,
    default=30,
    help='Number of timesteps to simulate (default: 30)'
)
parser.add_argument(
    '--terminal-width', '-w',
    type=int,
    default=BLANK_INT,
    help=f'Terminal width for rendering'
)
parser.add_argument(
    '--render-start', '-r',
    type=int,
    default=-5,
    help='Starting position for rendering the tapes (default: -5)'
)
parser.add_argument(
    '--apply-reduction', '-a',
    action='store_true',
    help='Use a automata ruleset with half-reduction'
)

if __name__ == '__main__':
    args = parser.parse_args()
    runner = ComposedCounterAutomataRunner(
        base=args.base,
        initial_write_start=args.write_start,
        initial_write_end=args.write_end,
        apply_reduction=args.apply_reduction
    )
    runner.run_simulation(
        num_timesteps=args.timesteps,
        terminal_width=args.terminal_width
    )
