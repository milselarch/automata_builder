from typing import Callable, Final

from automata_builder._rust import D

from automata_builder.rule_generator import TapeCellState, TapeNo

DATA_TAPE: Final[TapeNo] = TapeNo(0)
SIGNALS_TAPE: Final[TapeNo] = TapeNo(1)
CARRY_TAPE: Final[TapeNo] = TapeNo(2)
REDUCER_TAPE: Final[TapeNo] = TapeNo(3)

"""
For the signals tape (LSB first to MSB last):
- bits[0] => whether the rest of the state is a counter state 
    - bits[0] == 0: the state is a counter state
        - bits[1] => whether the counter state is paused or not
        - bits[2...] => 1 + the value of the counter state (in base self.base)
          we add one to the counter value to distinguish between the 
          void state (counter value 0) and the counter state with value 0
    - bits[0] == 1: state is a non-counter state 
        - bits[1] == 1: the state is a REDUCE_START state
"""

DT_DATA: Final[TapeCellState] = TapeCellState(0b10)
"""
- bits[0] == 1: state is a non-counter state 
- bits[1] == 1: the state is a REDUCE_START state
"""
ST_REDUCE_START: Final[TapeCellState] = TapeCellState(0b11)
CT_DATA: Final[TapeCellState] = TapeCellState(0b10)

REDUCER_DATA: Final[TapeCellState] = TapeCellState(0b10)
REDUCER_PAUSED_DATA: Final[TapeCellState] = TapeCellState(0b11)


def prefill_tape(position: int, tape_no: int) -> Callable[[int], D]:
    def set_cell_state(cell_state: int) -> D:
        return D(position, tape_no, cell_state)

    return set_cell_state


def prefill_tape_no(tape_no: int) -> Callable[[int, int], D]:
    def set_position_and_cell_state(position: int, cell_state: int) -> D:
        return D(position, tape_no, cell_state)

    return set_position_and_cell_state


LEFT: Final[int] = -1
MID: Final[int] = 0
RIGHT: Final[int] = 1

ST: Final[Callable[[int, int], D]] = prefill_tape_no(SIGNALS_TAPE)
DT: Final[Callable[[int, int], D]] = prefill_tape_no(DATA_TAPE)
CT: Final[Callable[[int, int], D]] = prefill_tape_no(CARRY_TAPE)

ST_LEFT: Final[Callable[[int], D]] = prefill_tape(LEFT, SIGNALS_TAPE)
ST_MID: Final[Callable[[int], D]] = prefill_tape(MID, SIGNALS_TAPE)
ST_RIGHT: Final[Callable[[int], D]] = prefill_tape(RIGHT, SIGNALS_TAPE)

DT_LEFT: Final[Callable[[int], D]] = prefill_tape(LEFT, DATA_TAPE)
DT_MID: Final[Callable[[int], D]] = prefill_tape(MID, DATA_TAPE)
DT_RIGHT: Final[Callable[[int], D]] = prefill_tape(RIGHT, DATA_TAPE)

CT_LEFT: Final[Callable[[int], D]] = prefill_tape(LEFT, CARRY_TAPE)
CT_MID: Final[Callable[[int], D]] = prefill_tape(MID, CARRY_TAPE)
CT_RIGHT: Final[Callable[[int], D]] = prefill_tape(RIGHT, CARRY_TAPE)

REDUCER_LEFT: Final[Callable[[int], D]] = prefill_tape(LEFT, REDUCER_TAPE)
REDUCER_MID: Final[Callable[[int], D]] = prefill_tape(MID, REDUCER_TAPE)
REDUCER_RIGHT: Final[Callable[[int], D]] = prefill_tape(RIGHT, REDUCER_TAPE)


def build_st_counter_state(counter_digit: int, paused: bool) -> int:
    """
    For the signals tape (LSB first to MSB last):
    - bits[0] - whether the rest of the state is a counter state
        - bits[0] == 0: the state is a counter state
            - bits[1] - whether the counter state is paused or not
            - bits[2...] - 1 + the value of the counter state
              (in base self.base)
              we add one to the counter value to distinguish between the
              void state (counter value 0) and the counter state with
              value 0
        - bits[0] == 1: state is a non-counter state
            - bits[1] == 1: the state is a REDUCE_START state
    """
    assert counter_digit >= 0, "Counter digit must be non-negative"
    # noinspection PyRedundantParentheses
    return (
            (0b00) |  # bit 0: equals 0 when in counter state
            (0b10 if paused else 0b00) |  # bit 1: paused or not
            ((counter_digit + 1) << 2)  # bits 2...: counter value
    )


def paused_counter(counter_digit: int) -> int:
    """
    Encodes the paused counter cell state in the signals tape
    Generally, paused counter states occur in the outputs of
    transition rules rather than the inputs

    :param counter_digit:
    digit value of the counter state
    :return:
    """
    return build_st_counter_state(counter_digit, paused=True)


def active_counter(counter_digit: int) -> int:
    """
    Encodes the active counter cell state in the signals tape
    Generally, active counter states occur in the inputs of
    transition rules rather than the outputs

    :param counter_digit:
    digit value of the counter state
    :return:
    """
    return build_st_counter_state(counter_digit, paused=False)


def from_counter_state(state: int) -> tuple[int, bool]:
    """
    Examples:
    digit=0: paused=6 active=4
    digit=1: paused=10 active=8
    digit=2: paused=14 active=12
    digit=3: paused=18 active=16
    digit=4: paused=22 active=20
    digit=5: paused=26 active=24

    :param state: counter state in the signals tape encoding
    :return:
    - paused: whether the counter state is paused or not
    - counter_digit: the value of the counter state (in base self.base)
    TODO: unittest that this and to_counter_state are inverse operations
    """
    paused = (state & 0b10) != 0
    counter_digit = (state >> 2) - 1
    assert counter_digit >= 0, state
    return counter_digit, paused
