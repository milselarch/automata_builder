import argparse
import time

from pprint import pprint
from automata_builder._rust import A, PyProduct
from tqdm import tqdm

from automata_builder.counter_automata import (
    CounterAutomataRunner, DT_DATA, DATA_TAPE, SIGNALS_TAPE,
    from_counter_state
)
from automata_builder.rule_generator_multitape import (
    MultiTapeBuilder, MultiTapeState
)

"""
Generates a single-tape counter cellular automata 
from its multi-tape variant 
"""

parser = argparse.ArgumentParser()
parser.add_argument(
    '-b', '--base',
    nargs='?', type=int, default=2, const=2,
    help='reduction base'
)
parser.add_argument(
    '-d', '--display-transitions',
    nargs='?', type=int, default=10, const=10,
    help='number of transitions to display from composed transition group'
)
parser.add_argument(
    '--apply-reduction', '-a',
    action='store_true',
    help='Use a automata ruleset with half-reduction'
)

if __name__ == '__main__':
    """
    python -m automata_builder.tests.compose_tapes_test
    # build half-reduced tapes for base 6
    python -m automata_builder.tests.compose_tapes_test -a -b 6
    # build half-reduced tapes for base 8
    python -m automata_builder.tests.compose_tapes_test -a -b 8
    """
    args = parser.parse_args()
    runner = CounterAutomataRunner(
        base=args.base,
        apply_reduction=args.apply_reduction
    )
    multi_tape_builder = MultiTapeBuilder(
        multi_tape_automata=runner.multi_tape_automata
    )
    multi_tape_builder.declare_initial_group_overlaps(
        overlap_states={
            MultiTapeState(tape_no=DATA_TAPE, tape_cell_state=DT_DATA)
        }
    )

    start_stamp = time.time()
    compose_result = multi_tape_builder.compose_tapes()
    end_stamp = time.time()
    duration = end_stamp - start_stamp

    state_mappings = {}
    remapped_states = compose_result.state_remap.get_all_remap_states()

    for remapped_state in remapped_states:
        rev_lookup_res = compose_result.state_remap.rev_lookup(remapped_state)
        original = rev_lookup_res.unwrap()
        state_mappings[remapped_state] = original
        # print(f'{remapped_state} -> {original}')

    print("\nLOADING STATE MAPPINGS:")
    pprint(state_mappings, width=200, sort_dicts=True)
    num_pause_incomparable_transitions = 0

    print(f'Completed in {duration:.02f} seconds')
    num_remapped_states = compose_result.count_unique_states()
    print(f'num remapped states = {num_remapped_states}')
    transitions = compose_result.transitions_group.transitions
    print(f'num transitions = {len(transitions)}')

    # TODO: count transitions with both paused and active states
    """
    for transition in transitions:
        input_terms, output_state = transition
        input_product = PyProduct(input_terms)
        input_multi_term_prod_res = compose_result.remap_prod_to_multi_tape(
            input_product=input_product
        )
        if input_multi_term_prod_res.is_ok():
            input_multi_term_product = input_multi_term_prod_res.unwrap()
        else:
            continue
    """

    for transition in tqdm(transitions):
        input_terms = transition.input_terms
        output_state = transition.output_state
        input_product = PyProduct(input_terms)
        input_multi_term_prod_res = compose_result.remap_prod_to_multi_tape(
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

    for k, transition in enumerate(transitions[:args.display_transitions]):
        input_terms = transition.input_terms
        output_state = transition.output_state
        input_product = PyProduct(input_terms)
        print(f'[{k}]: {input_product} -> {output_state}')

        input_multi_term_prod_res = compose_result.remap_prod_to_multi_tape(
            input_product=input_product
        )
        if input_multi_term_prod_res.is_ok():
            input_multi_term_product = input_multi_term_prod_res.unwrap()
        else:
            input_multi_term_product = 'HALT'

        output_product_res = compose_result.remap_term_to_multi_tape(
            input_term=A(position=0, state=output_state)
        )
        if output_product_res.is_ok():
            output_product = output_product_res.unwrap()
        else:
            output_product = 'HALT'

        print(f'|{k}|: {input_multi_term_product} -> {output_product}')
