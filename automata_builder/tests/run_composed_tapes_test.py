import argparse
import time

from automata_builder._rust import PyProduct
from tqdm import tqdm

from automata_builder.composed_counter_automata import (
    ComposedCounterAutomataRunner
)
from automata_builder.counter_automata import SIGNALS_TAPE, from_counter_state
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
    start_stamp = time.time()

    runner = ComposedCounterAutomataRunner(
        base=args.base,
        initial_write_start=args.write_start,
        initial_write_end=args.write_end,
        apply_reduction=args.apply_reduction
    )

    end_stamp = time.time()
    duration = end_stamp - start_stamp

    print(f'Completed in {duration:.02f} seconds')
    num_remapped_states = runner.count_unique_states()
    print(f'num remapped states = {num_remapped_states}')
    transitions = runner.transitions_group.transitions
    print(f'num transitions = {len(transitions)}')

    num_pause_incomparable_transitions = 0

    for transition in tqdm(transitions):
        input_terms = transition.input_terms
        output_state = transition.output_state
        input_product = PyProduct(input_terms)
        input_multi_term_prod_res = runner.remap_prod_to_multi_tape(
            input_product=input_product
        )
        if input_multi_term_prod_res.is_err():
            continue

        input_multi_term_product = input_multi_term_prod_res.unwrap()
        counter_terms = [
            term for term in input_multi_term_product.get_flat_terms() if
            term.get_tape_no() == SIGNALS_TAPE and
            term.get_cell_state() % 2 == 0 and
            term.get_cell_state() >= 4
        ]
        # print(f'{counter_terms=}')
        paused_list = [
            from_counter_state(term.get_cell_state())[1]
            for term in counter_terms
        ]
        if len(set(paused_list)) <= 1:
            num_pause_incomparable_transitions += 1

    print(f'{num_pause_incomparable_transitions=}')

    runner.run_simulation(
        num_timesteps=args.timesteps,
        terminal_width=args.terminal_width
    )
