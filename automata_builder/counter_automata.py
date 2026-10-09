from __future__ import annotations

import os

from typing import Final, Callable, Sequence
from automata_builder._rust import (
    D, PyMultiTapeAutomata, PyProcessStepResult, PyMultiTapeDataRegion,
    PyRenderFrame
)

from automata_builder.counter_states import ST_REDUCE_START, DT_LEFT, ST_MID, ST_RIGHT, DT_DATA, ST_LEFT, DT_MID, \
    SIGNALS_TAPE, paused_counter, active_counter, CARRY_TAPE, CT_DATA, CT_MID, DT_RIGHT, REDUCER_DATA, REDUCER_LEFT, \
    REDUCER_MID, REDUCER_PAUSED_DATA, REDUCER_TAPE, REDUCER_RIGHT, DATA_TAPE
from automata_builder.rule_generator import BLANK_INT
from automata_builder.rule_generator_multitape import (
    MultiTapeTransitionsGroup, TapeNo, TapeCellState,
    MultiTapeRuleGenerator, MultiTapeState, VOID_STATE, generate_prod_priority_map
)


class CounterAutomataBuilder(object):
    def __init__(self, base: int = 2):
        assert base >= 2, "Base must be at least 2"
        self.base = base

    def build_base_transitions_group(
        self, transitions_group: MultiTapeTransitionsGroup | None = None,
        counter_right_state: TapeCellState = ST_REDUCE_START,
    ) -> MultiTapeTransitionsGroup:
        if counter_right_state not in (ST_REDUCE_START, VOID_STATE):
            raise ValueError(
                f'counter_right_state must be either '
                f'{ST_REDUCE_START=} or {VOID_STATE=}, '
                f'got {counter_right_state}'
            )

        if transitions_group is not None:
            _transitions_group = transitions_group
        else:
            _transitions_group = MultiTapeTransitionsGroup(
                require_annotation=True
            )

        # TODO: actually precompute the number of states beforehand (?)
        max_counter_digit = self.base - 1
        _transitions_group = MultiTapeTransitionsGroup(
            require_annotation=True
        )

        # mark exponential bit reduction start
        _transitions_group.add_transition(
            input_terms=(
                ST_LEFT(VOID_STATE), DT_LEFT(DT_DATA),
                DT_MID(VOID_STATE), ST_MID(VOID_STATE),
                # ST_RIGHT(VOID_STATE)  # <- inserted for compilability
            ),
            output_tape_no=SIGNALS_TAPE, output_cell_state=ST_REDUCE_START,
            annotation='EXP_REDUCE_START'
        )

        # begin the counter-accumulator on the right side
        _transitions_group.add_transition(
            input_terms=(
                ST_MID(VOID_STATE), DT_MID(DT_DATA),
                DT_RIGHT(VOID_STATE), ST_RIGHT(VOID_STATE),
                # ST_RIGHT(VOID_STATE).shift(1)
            ),
            output_tape_no=SIGNALS_TAPE,
            output_cell_state=paused_counter(1),
            annotation=f'COUNTER_ACC_START'
        )

        # apply carry cells to counter-cells
        # carry cells stay stationary while counter-cells move left
        for mid_digit in range(self.base):
            for right_digit in range(self.base):
                # when there is no carry state to apply, shift left
                _transitions_group.add_transition(
                    input_terms=(
                        ST_MID(active_counter(mid_digit)),
                        ST_RIGHT(active_counter(right_digit)),
                        CT_MID(VOID_STATE)
                    ),
                    output_tape_no=SIGNALS_TAPE,
                    output_cell_state=paused_counter(right_digit),
                    annotation=f'SHL_{mid_digit}_{right_digit}_NO_CARRY'
                )
                if right_digit < max_counter_digit:
                    # carry but no overflow (right counter-digit < base)
                    carry_no_overflow_combo = (
                        ST_MID(active_counter(mid_digit)),
                        ST_RIGHT(active_counter(right_digit)),
                        CT_MID(CT_DATA)
                    )
                    # move right_digit left and increment
                    _transitions_group.add_transition(
                        input_terms=carry_no_overflow_combo,
                        output_tape_no=SIGNALS_TAPE,
                        output_cell_state=paused_counter(right_digit + 1),
                        annotation=f'SHL_{right_digit}_INC'
                    )
                    # overflow right_digit to 0 and move left, cancel carry
                    _transitions_group.add_transition(
                        input_terms=carry_no_overflow_combo,
                        output_tape_no=CARRY_TAPE,
                        output_cell_state=VOID_STATE,
                        annotation=f'CANCEL_CARRY'
                    )
                else:
                    # overflow to 0 and move left, carry stays for next digit
                    assert right_digit == max_counter_digit
                    _transitions_group.add_transition(
                        input_terms=(
                            ST_MID(active_counter(mid_digit)),
                            ST_RIGHT(active_counter(max_counter_digit)),
                            CT_MID(CT_DATA)
                        ),
                        output_tape_no=SIGNALS_TAPE,
                        output_cell_state=paused_counter(0),
                        annotation=f'SHL_{mid_digit}_OVERFLOW'
                    )

        for digit in range(self.base):
            # if there is a carry, and we're at the end of the built number
            # sequence and the rightmost digit is about to overflow
            right_overflow_combo = (
                ST_MID(active_counter(digit)),
                ST_RIGHT(counter_right_state),
                CT_MID(CT_DATA)
            )
            _transitions_group.add_transition(
                input_terms=right_overflow_combo,
                output_tape_no=SIGNALS_TAPE,
                output_cell_state=paused_counter(1),
                annotation=f'RIGHT_OVERFLOW_INC'
            )
            _transitions_group.add_transition(
                input_terms=right_overflow_combo,
                output_tape_no=CARRY_TAPE,
                output_cell_state=VOID_STATE,
                annotation=f'RIGHT_OVERFLOW_CARRY_CANCEL'
            )

        # clear rightmost counter cell if no carry
        for digit in range(self.base):
            # if cell next to rightmost (signals tape) counter cell
            # is a data cell
            _transitions_group.add_transition(
                input_terms=(
                    ST_MID(active_counter(digit)),
                    ST_RIGHT(counter_right_state),
                    CT_MID(VOID_STATE)
                ),
                output_tape_no=SIGNALS_TAPE,
                output_cell_state=counter_right_state,
                annotation=f'CLEAR_RIGHTMOST_{digit}'
            )
            # if cell next to rightmost (signals tape) counter cell
            # is reduction start marker
            _transitions_group.add_transition(
                input_terms=(
                    ST_MID(active_counter(digit)),
                    ST_RIGHT(counter_right_state),
                    CT_MID(VOID_STATE)
                ),
                output_tape_no=SIGNALS_TAPE,
                output_cell_state=counter_right_state,
                annotation=f'CLEAR_RIGHTMOST_{digit}_ST'
            )

        # paused counter states will transition to unpause
        # this should be the only place where paused counter states occur in
        # the inputs of transition rules
        for digit in range(self.base):
            _transitions_group.add_transition(
                input_terms=(ST_MID(paused_counter(digit)),),
                output_tape_no=SIGNALS_TAPE,
                output_cell_state=active_counter(digit),
                annotation=f'PAUSE_TO_UNPAUSE_{digit}'
            )

        return _transitions_group

    def build_increment_transitions_group(
        self, transitions_group: MultiTapeTransitionsGroup | None = None,
        increment_trigger_term: D = DT_MID(DT_DATA),
        no_increment_states: Sequence[D] = (DT_MID(VOID_STATE),)
    ) -> MultiTapeTransitionsGroup:
        """
        builds transitions group rules for shifting the
        leftmost counter-tape cell left wards and
        incrementing when going over a cell that
        signals to do so (increment_trigger_state)

        :param no_increment_states:
        :param increment_trigger_term:
        :param transitions_group:
        :return:
        """
        if transitions_group is not None:
            _transitions_group = transitions_group
        else:
            _transitions_group = MultiTapeTransitionsGroup(
                require_annotation=True
            )

        increment_tape_no = increment_trigger_term.get_tape_no()
        for no_increment_state in no_increment_states:
            no_increment_state_tape_no = no_increment_state.get_tape_no()
            if no_increment_state_tape_no != increment_tape_no:
                raise ValueError(
                    f'no_increment_state tape no '
                    f'{no_increment_state_tape_no} '
                    f'does not match increment_trigger_state tape no '
                    f'{increment_tape_no}'
                )

        max_counter_digit = self.base - 1
        # shift counter-tape cell leftwards and increment if needed
        for digit in range(self.base):
            if digit == max_counter_digit:
                # overflow digit from max_counter_digit to 0 and add a
                # new max_counter_digit at the end
                _transitions_group.add_transition(
                    input_terms=(
                        increment_trigger_term,
                        ST_MID(VOID_STATE),
                        ST_RIGHT(active_counter(max_counter_digit))
                    ),
                    output_tape_no=SIGNALS_TAPE,
                    output_cell_state=paused_counter(0),
                    annotation=f'LM_OVERFLOW_{max_counter_digit}'
                )
                # spawn a carry cell state to propagate to digits to the right
                _transitions_group.add_transition(
                    input_terms=(
                        increment_trigger_term,
                        ST_MID(VOID_STATE),
                        ST_RIGHT(active_counter(max_counter_digit))
                    ),
                    output_tape_no=CARRY_TAPE,
                    output_cell_state=CT_DATA,
                    annotation=f'LM_SPAWN_CARRY'
                )
            else:
                # move digit leftwards and increment by 1
                assert digit < max_counter_digit
                _transitions_group.add_transition(
                    input_terms=(
                        increment_trigger_term,
                        ST_MID(VOID_STATE),
                        ST_RIGHT(active_counter(digit)),
                    ),
                    output_tape_no=SIGNALS_TAPE,
                    output_cell_state=paused_counter(digit + 1),
                    annotation=f'LM_LEFT_{digit}_AND_INC'
                )

        for no_increment_state in no_increment_states:
            # Bleed leftmost counter cell leftwards past data tape to void
            # Unnecessary if we don't expect to leave data tape range
            for digit in range(self.base):
                _transitions_group.add_transition(
                    input_terms=(
                        no_increment_state,
                        ST_MID(VOID_STATE),
                        ST_RIGHT(active_counter(digit)),
                        CT_MID(VOID_STATE)
                    ),
                    output_tape_no=SIGNALS_TAPE,
                    output_cell_state=paused_counter(digit),
                    annotation=f'BLEED_{digit}_OVER_{no_increment_state}'
                )

        return _transitions_group

    @classmethod
    def build_reducer_transitions_group(
        cls, transitions_group: MultiTapeTransitionsGroup | None = None
    ) -> MultiTapeTransitionsGroup:
        """
        On the reducer tape, we will build rules to
        1. Spawn a data state cell on the leftwards end of the data tape
        2. Spread the date state cells in both directions
           at a speed of 1 cell every 2 timesteps.

        :param transitions_group:
        :return:
        """
        if transitions_group is not None:
            _transitions_group = transitions_group
        else:
            _transitions_group = MultiTapeTransitionsGroup(
                require_annotation=True
            )

        # spawn data at the left of the
        # left end of initial data of the data tape
        _transitions_group.add_transition(
            input_terms=(
                DT_LEFT(VOID_STATE),
                DT_MID(DT_DATA),
                ST_MID(VOID_STATE),  # <- inserted for compilability
                REDUCER_LEFT(VOID_STATE),
                REDUCER_MID(VOID_STATE),
                ST_RIGHT(VOID_STATE)  # <- inserted for compilability
            ),
            output_tape_no=REDUCER_TAPE,
            output_cell_state=REDUCER_PAUSED_DATA,
            annotation='REDUCER_SPAWN_LEFT_END'
        )
        # spread the data state rightwards while overlapping with input data
        _transitions_group.add_transition(
            input_terms=(
                REDUCER_LEFT(REDUCER_DATA),
                DT_MID(DT_DATA),
                REDUCER_MID(VOID_STATE),
                ST_RIGHT(VOID_STATE)  # <- inserted for compilability
            ),
            output_tape_no=REDUCER_TAPE,
            output_cell_state=REDUCER_PAUSED_DATA,
            annotation='REDUCER_DATA_SPREAD_RIGHT'
        )
        # spread the data state leftwards (regardless of input data overlap)
        _transitions_group.add_transition(
            input_terms=(
                REDUCER_LEFT(VOID_STATE),  # <- inserted for compilability rs
                REDUCER_MID(VOID_STATE),
                REDUCER_RIGHT(REDUCER_DATA),
            ),
            output_tape_no=REDUCER_TAPE,
            output_cell_state=REDUCER_PAUSED_DATA,
            annotation='REDUCER_DATA_SPREAD_LEFT'
        )
        # convert the paused half-data tape state to an active state
        _transitions_group.add_transition(
            input_terms=(
                DT_MID(DT_DATA),  # <- inserted for compilability
                ST_MID(VOID_STATE),  # <- inserted for compilability
                ST_RIGHT(VOID_STATE),  # <- inserted for compilability
                REDUCER_MID(REDUCER_PAUSED_DATA),
            ),
            output_tape_no=REDUCER_TAPE,
            output_cell_state=REDUCER_DATA,
            annotation=f'REDUCER_PAUSE_TO_UNPAUSE'
        )
        return _transitions_group

    def build_transitions_group(self) -> MultiTapeTransitionsGroup:
        transitions_group = self.build_base_transitions_group()
        transitions_group = self.build_increment_transitions_group(
            transitions_group=transitions_group,
            increment_trigger_term=DT_MID(DT_DATA),
            no_increment_states=(DT_MID(VOID_STATE),)
        )
        return transitions_group

    def build_reduced_transitions_group(self) -> MultiTapeTransitionsGroup:
        """
        The rules built here are designed to produce an automaton
        that generates the n-nary representation of n/2, where n is
        the number of data cells on the data tape, within n timesteps,
        and where the encoded number is fully contained within the range of
        the data tape at time n without leftover carry states
        on the carry tape.

        Outline of proof:
        TODO: formalize proof in lean or something
        0. b-ary data range with n cells is declared in data tape at the start
        1. reducer tape data cells spawn to the left
           of the data range and travels
           rightwards at speed of 1/2 cells per timestep
           (due to pausing every other timestep)
        2. b-ary counter is initialized (in signals tape)
           to the right and travels
           leftwards at speed of 1/2 cells per timestep
           (due to pausing every other timestep)
        3. counter increments when it passes over a data cell
           on the data tape and there is no reducer tape data cell
           at the same position
        4. at time n, the counter will have incremented n/2 times
           since there would only have been n/2 data tape cells
           with no corresponding reducer cells to pass over
        5. after time n, no more counter increments will occur
        6. since this is an n-ary counter, the accumulated counter
           will never be more than n/2 length in width given
           n/2 increments
        6. carry cells that spawn will therefore take at most
           2*(n/2) = n timesteps to propagate throughout the counter
           (we multiply by 2 since carry also propagates at a
           speed of 1/2 cells per timesteps)
        7. therefore by time n+n = 2n, the counter encodes
           a value of n/2 in base b, and the carry tape is empty,
           and both will remain so forever after
        :return:
        """
        transitions_group = self.build_base_transitions_group()
        transitions_group = self.build_reducer_transitions_group(
            transitions_group=transitions_group
        )
        """
        The presence of a data cell on the 
        reducer tape is a signal to not increment the counter.
        
        Since reduction signal and counter are on opposite ends of 
        the data tape positionally, and they move at half speed, we 
        should expect that only half of the data tape cells will 
        contribute to counter increments.
        """
        transitions_group = self.build_increment_transitions_group(
            transitions_group=transitions_group,
            increment_trigger_term=REDUCER_MID(VOID_STATE),
            no_increment_states=(
                REDUCER_MID(REDUCER_DATA),
                REDUCER_MID(REDUCER_PAUSED_DATA),
            )
        )
        return transitions_group


class CounterAutomataRunner(object):
    def __init__(
        self, base: int = 8, initial_write_start: int = 0,
        initial_write_end: int = 20,
        apply_reduction: bool = False
    ):
        """
        Counter-automata instance with initial cells populated
        from :initial_write_start: to :initial_write_end:
        (inclusive) with the [DATA] cell value
        :param base:
        :param initial_write_start:
        :param initial_write_end:
        """
        self.base = base
        self.apply_reduction = apply_reduction
        self.builder = CounterAutomataBuilder(base=base)

        if apply_reduction:
            transitions_group = self.builder.build_reduced_transitions_group()
        else:
            transitions_group = self.builder.build_transitions_group()

        self.transitions_group = transitions_group
        self.prod_priority_map = generate_prod_priority_map(
            transitions_group=self.transitions_group
        )
        self.state_eq_map = MultiTapeRuleGenerator.generate_equations(
            transitions_group=self.transitions_group
        )

        self.initial_write_start = initial_write_start
        self.initial_write_end = initial_write_end
        self.multi_tape_automata = PyMultiTapeAutomata(self.state_eq_map)

        self.init_tapes = [DATA_TAPE, SIGNALS_TAPE, CARRY_TAPE]
        if apply_reduction:
            self.init_tapes.append(REDUCER_TAPE)

        self.multi_tape_automata.init_tapes(tape_nos=self.init_tapes)
        self.multi_tape_automata.write_region(
            position=self.initial_write_start,
            end_position=self.initial_write_end,
            data=[MultiTapeState(DATA_TAPE, DT_DATA)]
        )

    def get_minimal_data_region(self) -> PyMultiTapeDataRegion:
        return self.multi_tape_automata.get_minimal_data_region()

    def read_data_tape_value(self) -> int:
        data_tape = self.multi_tape_automata[DATA_TAPE]
        data_region = data_tape.get_minimal_data_region()
        if not data_region:
            return 0

        assert VOID_STATE not in data_region
        assert set(data_region) == {DT_DATA}
        return len(data_region)

    def read_signals_tape_value(self) -> int:
        """
        :return:
        The equivalent n-ary numerical value encoded on the signals tape
        Note that data is arranged from LSB (left / decreasing position)
        to MSB (right / increasing position)
        TODO: add option to flip accumulator direction when building automata?
        """
        signals_tape = self.multi_tape_automata[SIGNALS_TAPE]
        data_region = signals_tape.get_minimal_data_region()
        if not data_region:
            return 0

        # print("DATA_REGION", data_region)
        # The last cell in the signals tape is always an ST_REDUCE_START cell
        assert data_region[-1] == ST_REDUCE_START
        digit_data_region = data_region[:-1]

        while digit_data_region and digit_data_region[-1] == ST_REDUCE_START:
            # there may be trailing VIUDs between ST_REDUCE_START and data
            digit_data_region.pop()

        # the remaining relevant_data_region should just encode
        # the counter value in base {self.base}
        assert VOID_STATE not in digit_data_region
        counter_paused: bool | None = None
        encoded_number = 0

        for digit_no in range(len(digit_data_region)):
            tape_cell_state = digit_data_region[digit_no]
            counter_digit, paused = from_counter_state(tape_cell_state)
            encoded_number += counter_digit * self.base ** digit_no

            if counter_paused is None:
                counter_paused = paused
            else:
                assert counter_paused == paused, (
                    f'Inconsistent paused state in signals tape: '
                    f'{paused=} for {tape_cell_state=}'
                )

        assert encoded_number >= 0
        # TODO: consider unpropagated carry states
        return encoded_number

    @staticmethod
    def resolve_terminal_width(terminal_width: int = BLANK_INT) -> int:
        # TODO: refactor out away from runner
        try:
            terminal_size = os.get_terminal_size()
            default_terminal_width = terminal_size.columns - 1
        except OSError:
            default_terminal_width = 100

        if terminal_width == BLANK_INT:
            terminal_width = default_terminal_width

        return terminal_width

    def render_tapes(
        self, start_position: int, length: int, cell_width: int = BLANK_INT
    ) -> PyRenderFrame:
        return self.multi_tape_automata.render_tapes(
            start_position=start_position, length=length,
            cell_width=cell_width
        )

    def step(self, verbose: bool = True) -> PyProcessStepResult:
        return self.multi_tape_automata.step(verbose=verbose)

    def run_simulation(
        self, num_timesteps: int = 30, terminal_width: int = BLANK_INT,
        render_start: int = -5, render: bool = True
    ):
        terminal_width = self.resolve_terminal_width(
            terminal_width=terminal_width
        )
        if render:
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
                render_frame = self.multi_tape_automata.render_tapes(
                    start_position=render_start, length=terminal_width,
                    cell_width=2
                )
                # print(render_frame.get_dimensions())
                print(f'\nTIMESTEP {timestep}:')
                print(render_frame.render())
                encoded_value = self.read_signals_tape_value()
                print(f'{encoded_value=}')
                print('')
