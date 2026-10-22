import argparse

from automata_builder.firing_squad_automata import FiringSquadAutomataRunner
from automata_builder.rule_generator import BLANK_INT

if __name__ == '__main__':
    """
    python -m automata_builder.tests.run_firing_squad
    """
    parser = argparse.ArgumentParser(
        description='Run the firing squad automata simulation.'
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
        default=5,
        help='Ending position to write the initial data (default: 20)'
    )
    parser.add_argument(
        '--timesteps', '-t',
        type=int,
        default=BLANK_INT,
        help='Number of timesteps to simulate'
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

    args = parser.parse_args()
    timesteps = args.timesteps
    write_width = args.write_end - args.write_start + 1

    if timesteps == BLANK_INT:
        timesteps = write_width * 2

    runner = FiringSquadAutomataRunner(
        initial_write_start=args.write_start,
        initial_write_end=args.write_end,
    )
    runner.run_simulation(
        num_timesteps=timesteps,
        terminal_width=args.terminal_width
    )
