from automata_builder._rust import (
    D, PySingleTapeAutomata, PyMultiTapeProduct, PyProcessStepResult,
    PySingleTapeProcessStepResult
)

from automata_builder.counter_automata import (
    CounterAutomataRunner, DATA_TAPE, DT_DATA, paused_counter, active_counter
)
from automata_builder.rule_generator import (
    VOID_STATE, RuleGenerator, AutomataRuleSet, BLANK_INT
)
from automata_builder.rule_generator_multitape import (
    MultiTapeBuilder, ComposeTapesResult
)
from automata_builder.tape_overlaps import MultiTapeState

INPUT_DATA_STATE = MultiTapeState(tape_no=DATA_TAPE, tape_cell_state=DT_DATA)


class ComposedCounterAutomataRunner(object):
    def __init__(
        self, base: int = 8, initial_write_start: int = 0,
        initial_write_end: int = 20,
        apply_reduction: bool = False
    ):
        self.base = base
        self.initial_write_start = initial_write_start
        self.initial_write_end = initial_write_end
        self.apply_reduction = apply_reduction

        self.multi_tape_runner = CounterAutomataRunner(
            base=self.base,
            initial_write_start=self.initial_write_start,
            initial_write_end=self.initial_write_end,
            apply_reduction=self.apply_reduction
        )
        self.multi_tape_builder = MultiTapeBuilder(
            multi_tape_automata=self.multi_tape_runner.multi_tape_automata
        )
        self.multi_tape_builder.declare_initial_group_overlaps(
            overlap_states={INPUT_DATA_STATE}
        )

        self.compose_result: ComposeTapesResult = (
            self.multi_tape_builder.compose_tapes()
        )
        self.composed_ruleset: AutomataRuleSet = RuleGenerator.to_ruleset(
            transitions_group=self.compose_result.transitions_group,
            require_consistent_flat_term_offsets=False,
            verbose=False
        )
        self.single_tape_automata = PySingleTapeAutomata(
            state_eq_map=self.composed_ruleset.expansion_map
        )

        input_data_product = self.build_full_init_data_product()
        input_data_state = self.compose_result.remap_from_product_to_state(
            input_data_product
        )
        self.single_tape_automata.write_region(
            position=self.initial_write_start,
            end_position=self.initial_write_end,
            data=[input_data_state]
        )
        self.assert_tapes_consistency()

    def read_multi_tape_signal_value(self) -> int:
        return self.multi_tape_runner.read_signals_tape_value()

    def build_full_init_data_product(
        self, position: int = 0
    ) -> PyMultiTapeProduct:
        init_multi_tape_terms: list[D] = []
        for tape_no in self.multi_tape_runner.init_tapes:
            tape_cell_state = VOID_STATE
            if tape_no == DATA_TAPE:
                tape_cell_state = DT_DATA

            init_multi_tape_terms.append(
                D(position=position, tape_no=tape_no, state=tape_cell_state)
            )

        return PyMultiTapeProduct(init_multi_tape_terms)

    def step(
        self, verbose: bool = False, assert_consistency: bool = True
    ) -> tuple[PySingleTapeProcessStepResult, PyProcessStepResult]:
        def log(*args, **kwargs):
            if verbose:
                print(*args, **kwargs)

        log("<" * 10, "Single-tape", ">" * 10)
        single_tape_result = self.single_tape_automata.step(verbose=verbose)
        log()
        log("<" * 10, "Multi-tape", ">" * 10)
        multi_tape_result = self.multi_tape_runner.step(verbose=verbose)

        if assert_consistency:
            self.assert_tapes_consistency()

        return single_tape_result, multi_tape_result

    def assert_tapes_consistency(self):
        multi_tape_region = self.multi_tape_runner.get_minimal_data_region()
        single_tape_region = (
            self.single_tape_automata.get_minimal_data_region()
        )
        assert len(multi_tape_region) == len(single_tape_region), (
            f"Multi-tape region length {len(multi_tape_region)} != "
            f"single-tape region length {len(single_tape_region)}"
        )
        for index in range(len(single_tape_region)):
            product_slice = multi_tape_region[index].to_py_product()
            # TODO: translate to single tape and check equal
            remapped_state = self.compose_result.remap_from_product_to_state(
                product=product_slice
            )
            assert single_tape_region[index] == remapped_state, (
                f"Single tape state {single_tape_region[index]} != "
                f"remapped multi-tape {remapped_state} at index {index}"
            )

    def read_signals_tape_value(self) -> int:
        return self.multi_tape_runner.read_signals_tape_value()

    def resolve_terminal_width(self, terminal_width: int = BLANK_INT) -> int:
        return self.multi_tape_runner.resolve_terminal_width(
            terminal_width=terminal_width
        )

    def render(
        self, render_start: int = -5, terminal_width: int = BLANK_INT,
        header: str = ''
    ):
        terminal_width = self.resolve_terminal_width(
            terminal_width=terminal_width
        )
        single_render_frame = self.single_tape_automata.render_tape(
            start_position=render_start, length=terminal_width,
            header_tag='-', cell_width=2
        )
        multi_render_frame = self.multi_tape_runner.render_tapes(
            start_position=render_start, length=terminal_width,
            cell_width=2
        )
        # print(render_frame.get_dimensions())
        if header:
            print(header)

        print(single_render_frame.render())
        print('-' * terminal_width)
        print(multi_render_frame.render())
        encoded_value = self.read_signals_tape_value()
        print(f'{encoded_value=}')
        print('')

    def run_simulation(
        self, num_timesteps: int = 30, terminal_width: int = BLANK_INT,
        render_start: int = -5, render: bool = True,
        assert_consistency: bool = True
    ):
        terminal_width = self.resolve_terminal_width(
            terminal_width=terminal_width
        )
        if render:
            self.render(
                render_start=render_start,
                terminal_width=terminal_width,
                header=f'\nTIMESTEP 0 START'
            )

            for digit in range(self.base):
                print(
                    f'{digit=}: '
                    f'paused={paused_counter(digit)} '
                    f'active={active_counter(digit)}'
                )

        for timestep in range(num_timesteps):
            # print(f'{terminal_width=}')
            if timestep > 0:
                self.step(verbose=render)

            if render:
                # print(f'\nTIMESTEP {timestep}:')
                self.render(
                    render_start=render_start,
                    terminal_width=terminal_width,
                    header=f'\nTIMESTEP {timestep} END'
                )

            if assert_consistency:
                self.assert_tapes_consistency()
