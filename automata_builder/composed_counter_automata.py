from automata_builder._rust import (
    D, PySingleTapeAutomata, PyMultiTapeProduct, PyProcessStepResult,
    PySingleTapeProcessStepResult
)

from automata_builder.counter_automata import (
    CounterAutomataRunner, DATA_TAPE, DT_DATA
)
from automata_builder.rule_generator import (
    VOID_STATE, RuleGenerator, AutomataRuleSet
)
from automata_builder.rule_generator_multitape import MultiTapeBuilder
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

        self.compose_result = self.multi_tape_builder.compose_tapes()
        self.composed_ruleset: AutomataRuleSet = RuleGenerator.to_ruleset(
            transitions_group=self.compose_result.transitions_group,
            verbose=True
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
        self, verbose: bool = False
    ) -> tuple[PySingleTapeProcessStepResult, PyProcessStepResult]:
        single_tape_result = self.single_tape_automata.step(verbose=verbose)
        multi_tape_result = self.multi_tape_runner.step(verbose=verbose)
        return single_tape_result, multi_tape_result

    def step_assert(
        self, verbose: bool = False
    ) -> tuple[PySingleTapeProcessStepResult, PyProcessStepResult]:
        step_result = self.step(verbose=verbose)
        single_tape_result, multi_tape_result = step_result
        new_multi_tape_state = multi_tape_result.new_multi_tape
        multi_tape_region = new_multi_tape_state.get_minimal_data_region()
        new_single_tape_state = single_tape_result.new_tape
        single_tape_region = new_single_tape_state.get_minimal_data_region()

        for index in range(len(single_tape_region)):
            product_slice = multi_tape_region[index].to_py_product()
            # TODO: translate to single tape and check equal
            assert single_tape_region[index] == multi_tape_region[index]

        assert single_tape_result == multi_tape_result
        return step_result
