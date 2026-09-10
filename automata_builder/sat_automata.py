from __future__ import annotations

import os
import re

from dataclasses import dataclass
from typing import Callable, Final

from automata_builder._rust import D, PyMultiTapeAutomata, PyProcessStepResult

from automata_builder.rule_generator import BLANK_INT
from automata_builder.rule_generator_multitape import (
    MultiTapeRuleGenerator,
    MultiTapeState,
    MultiTapeTransitionsGroup,
    TapeCellState,
    TapeNo,
    VOID_STATE,
)

LITERALS_TAPE: Final[TapeNo] = TapeNo(0)
ASSIGNMENTS_TAPE: Final[TapeNo] = TapeNo(1)
CLAUSES_TAPE: Final[TapeNo] = TapeNo(2)
SCAN_TAPE: Final[TapeNo] = TapeNo(3)
VERDICT_TAPE: Final[TapeNo] = TapeNo(4)

# TODO: THE AUTOMATA RULES DO NOT IMPLEMENT THE MOST IMPORTANT PART
#   WHICH IS VERIFYING THAT VAR ASSIGNMENTS LAID OUT IN TAPE
#   ARE NON-CONFLICTING
"""
The 3SAT automata evaluates a CNF formula against a variable assignment
using a single ruleset that is completely independent of the formula and
of the assignment being checked (i.e. the transition rules are universal).

The formula and the assignment are instead encoded as tape data:
the tape is split into one contiguous clause block per clause, and every
clause block holds one cell per variable of the formula, i.e. cell
(clause_no * num_variables + variable_no) belongs to clause :clause_no:
and to variable :variable_no:. For each such cell:

- LITERALS_TAPE holds how the variable occurs within the clause
  (not at all, un-negated, negated, or both un-negated and negated)
- ASSIGNMENTS_TAPE holds the truth value assigned to the variable
  (the assignment vector is replicated once per clause block so that
  every literal sits next to the value of its own variable)
- CLAUSES_TAPE marks the leftmost cell of every clause block

Because a variable is identified by its offset within a clause block,
all the rules ever need to look at is the cell they are standing on,
which is what makes them independent of the input.
"""

LT_POSITIVE: Final[TapeCellState] = TapeCellState(0b10)
"""variable occurs un-negated within the clause"""
LT_NEGATED: Final[TapeCellState] = TapeCellState(0b11)
"""variable occurs negated within the clause"""
LT_EITHER: Final[TapeCellState] = TapeCellState(0b100)
"""variable occurs both un-negated and negated within the clause"""

AT_FALSE: Final[TapeCellState] = TapeCellState(0b10)
AT_TRUE: Final[TapeCellState] = TapeCellState(0b11)

CT_CLAUSE_START: Final[TapeCellState] = TapeCellState(0b10)
"""marks the leftmost cell of a clause block"""
CT_INVALID_INPUT: Final[TapeCellState] = TapeCellState(0b11)
"""marks input that could not be encoded, which is rejected outright"""

"""
For the scan tape (LSB first to MSB last):
- bits[2] => whether the rest of the state is a scan accumulator state
    - bits[2] == 1: the state is a scan accumulator state
        - bits[0] => whether the clause being scanned is satisfied so far
        - bits[1] => whether all fully scanned clauses are satisfied
    - bits[2] == 0: the state is not a scan accumulator state
        - bits[1] == 1: the state is a spent (already scanned) cell

Note that no cell state may be encoded as HALT_STATE (0b1), which is
reserved by the multi-tape framework, hence the offset bits everywhere
"""
ST_SPENT: Final[TapeCellState] = TapeCellState(0b10)

VT_PENDING: Final[TapeCellState] = TapeCellState(0b10)
VT_UNSAT: Final[TapeCellState] = TapeCellState(0b11)
VT_SAT: Final[TapeCellState] = TapeCellState(0b100)

LITERAL_STATES: Final[tuple[TapeCellState, ...]] = (
    VOID_STATE, LT_POSITIVE, LT_NEGATED, LT_EITHER
)
ASSIGNMENT_STATES: Final[tuple[TapeCellState, ...]] = (AT_FALSE, AT_TRUE)
CLAUSE_STATES: Final[tuple[TapeCellState, ...]] = (VOID_STATE, CT_CLAUSE_START)

VERDICT_POSITION: Final[int] = -1
"""fixed position from which the verdict of the automata can be read"""
DATA_START_POSITION: Final[int] = 0
"""position of the leftmost cell of the leftmost clause block"""

LEFT: Final[int] = -1
MID: Final[int] = 0
RIGHT: Final[int] = 1

_VAR_NAME_RE: Final[re.Pattern[str]] = re.compile(r'^[A-Za-z][A-Za-z0-9_]*$')


def prefill_tape(position: int, tape_no: int) -> Callable[[int], D]:
    def set_cell_state(cell_state: int) -> D:
        return D(position, tape_no, cell_state)

    return set_cell_state


LT_MID: Final[Callable[[int], D]] = prefill_tape(MID, LITERALS_TAPE)

AT_MID: Final[Callable[[int], D]] = prefill_tape(MID, ASSIGNMENTS_TAPE)
AT_RIGHT: Final[Callable[[int], D]] = prefill_tape(RIGHT, ASSIGNMENTS_TAPE)

CT_MID: Final[Callable[[int], D]] = prefill_tape(MID, CLAUSES_TAPE)

ST_MID: Final[Callable[[int], D]] = prefill_tape(MID, SCAN_TAPE)
ST_RIGHT: Final[Callable[[int], D]] = prefill_tape(RIGHT, SCAN_TAPE)

VT_MID: Final[Callable[[int], D]] = prefill_tape(MID, VERDICT_TAPE)


def scan_state(clause_sat: bool, formula_sat: bool) -> int:
    """
    Encodes the scan accumulator cell state in the scan tape

    For the scan tape (LSB first to MSB last):
    - bits[2] - whether the rest of the state is a scan accumulator state
        - bits[2] == 1: the state is a scan accumulator state
            - bits[0] - whether the clause being scanned is satisfied so far
            - bits[1] - whether all fully scanned clauses are satisfied
        - bits[2] == 0: the state is not a scan accumulator state
            - bits[1] == 1: the state is a spent (already scanned) cell

    :param clause_sat:
    whether the clause currently being scanned has been satisfied by any
    of the literals scanned so far
    :param formula_sat:
    whether every clause that has been scanned in full is satisfied
    :return:
    """
    # noinspection PyRedundantParentheses
    return (
        (0b100) |  # bit 2: equals 1 when in a scan accumulator state
        (0b001 if clause_sat else 0b000) |  # bit 0: clause satisfied so far
        (0b010 if formula_sat else 0b000)  # bit 1: scanned clauses satisfied
    )


def is_scan_state(state: int) -> bool:
    """
    :param state: scan tape cell state
    :return: whether the cell holds a scan accumulator state
    """
    return (state & 0b100) != 0


def from_scan_state(state: int) -> tuple[bool, bool]:
    """
    :param state: scan accumulator state in the scan tape encoding
    :return:
    - clause_sat: whether the clause being scanned is satisfied so far
    - formula_sat: whether all fully scanned clauses are satisfied
    """
    assert is_scan_state(state), state
    clause_sat = (state & 0b001) != 0
    formula_sat = (state & 0b010) != 0
    return clause_sat, formula_sat


def evaluate_literal(
    literal_state: TapeCellState, assignment_state: TapeCellState
) -> bool:
    """
    Evaluate the literal of a single clause block cell, i.e. whether the
    clause is satisfied by the way the cell's variable occurs within it
    :param literal_state: how the variable occurs within the clause
    :param assignment_state: truth value assigned to the variable
    :return:
    """
    if literal_state == LT_EITHER:
        # both the variable and its negation are in the clause
        return True
    elif literal_state == LT_POSITIVE:
        return assignment_state == AT_TRUE
    elif literal_state == LT_NEGATED:
        return assignment_state == AT_FALSE

    # the variable does not occur within the clause at all
    assert literal_state == VOID_STATE, literal_state
    return False


class ThreeSATAutomataBuilder(object):
    """
    Builds the transition rules of the 3SAT automata.

    The ruleset is universal: it is built from the cell state encoding
    alone and is identical for every formula and every assignment,
    which are supplied to the automata as tape data instead.
    """

    @classmethod
    def advance_scan_state(
        cls, clause_sat: bool, formula_sat: bool,
        literal_sat: bool, is_clause_start: bool
    ) -> int:
        """
        Fold the cell the scan accumulator is moving onto into
        the accumulated verdicts it is carrying.

        Since the accumulator travels leftwards, the leftmost cell of a
        clause block is the last cell of that clause to be scanned, which
        is where the clause verdict is folded into the formula verdict and
        the clause verdict is reset for the next clause block.

        :param clause_sat:
        :param formula_sat:
        :param literal_sat:
        :param is_clause_start:
        :return:
        """
        new_clause_sat = clause_sat or literal_sat
        if is_clause_start:
            return scan_state(
                clause_sat=False,
                formula_sat=formula_sat and new_clause_sat
            )

        return scan_state(clause_sat=new_clause_sat, formula_sat=formula_sat)

    def build_scan_transitions_group(
        self, transitions_group: MultiTapeTransitionsGroup | None = None
    ) -> MultiTapeTransitionsGroup:
        """
        Builds the rules that spawn a scan accumulator on the rightmost
        clause block cell and move it leftwards by one cell per timestep,
        evaluating every literal it passes over along the way.

        :param transitions_group:
        :return:
        """
        if transitions_group is not None:
            _transitions_group = transitions_group
        else:
            _transitions_group = MultiTapeTransitionsGroup(
                require_annotation=True
            )

        for literal_state in LITERAL_STATES:
            for assignment_state in ASSIGNMENT_STATES:
                for clause_state in CLAUSE_STATES:
                    literal_sat = evaluate_literal(
                        literal_state=literal_state,
                        assignment_state=assignment_state
                    )
                    is_clause_start = clause_state == CT_CLAUSE_START
                    # the cell that the scan accumulator moves onto
                    # scan tape cell must be untouched by earlier scans
                    cell_terms = (
                        LT_MID(literal_state), AT_MID(assignment_state),
                        CT_MID(clause_state), ST_MID(VOID_STATE)
                    )
                    cell_tag = (
                        f'{literal_state}_{assignment_state}_{clause_state}'
                    )

                    # start the scan on the rightmost clause block cell,
                    # i.e. the cell without assignment data to its right
                    _transitions_group.add_transition(
                        input_terms=cell_terms + (AT_RIGHT(VOID_STATE),),
                        output_tape_no=SCAN_TAPE,
                        output_cell_state=self.advance_scan_state(
                            clause_sat=False, formula_sat=True,
                            literal_sat=literal_sat,
                            is_clause_start=is_clause_start
                        ),
                        annotation=f'SCAN_START_{cell_tag}'
                    )

                    for clause_sat in (False, True):
                        for formula_sat in (False, True):
                            # move the scan accumulator on the right leftwards
                            _transitions_group.add_transition(
                                input_terms=cell_terms + (ST_RIGHT(scan_state(
                                    clause_sat=clause_sat,
                                    formula_sat=formula_sat
                                )),),
                                output_tape_no=SCAN_TAPE,
                                output_cell_state=self.advance_scan_state(
                                    clause_sat=clause_sat,
                                    formula_sat=formula_sat,
                                    literal_sat=literal_sat,
                                    is_clause_start=is_clause_start
                                ),
                                annotation=(
                                    f'SCAN_SHL_{cell_tag}_'
                                    f'{int(clause_sat)}{int(formula_sat)}'
                                )
                            )

        # mark cells the scan accumulator has moved off as spent so that
        # they are neither rescanned nor used to start a new scan
        for clause_sat in (False, True):
            for formula_sat in (False, True):
                _transitions_group.add_transition(
                    input_terms=(ST_MID(scan_state(
                        clause_sat=clause_sat, formula_sat=formula_sat
                    )),),
                    output_tape_no=SCAN_TAPE,
                    output_cell_state=ST_SPENT,
                    annotation=(
                        f'SCAN_SPEND_{int(clause_sat)}{int(formula_sat)}'
                    )
                )

        return _transitions_group

    @classmethod
    def build_verdict_transitions_group(
        cls, transitions_group: MultiTapeTransitionsGroup | None = None
    ) -> MultiTapeTransitionsGroup:
        """
        Builds the rules that resolve the pending verdict cell once the
        scan accumulator has run past the leftmost clause block cell,
        as well as the rule that rejects input that could not be encoded.

        :param transitions_group:
        :return:
        """
        if transitions_group is not None:
            _transitions_group = transitions_group
        else:
            _transitions_group = MultiTapeTransitionsGroup(
                require_annotation=True
            )

        for formula_sat in (False, True):
            # the scan is complete once the accumulator sits on the leftmost
            # clause block cell, i.e. the cell without assignment data on
            # the side the accumulator is travelling towards
            _transitions_group.add_transition(
                input_terms=(
                    VT_MID(VT_PENDING), AT_MID(VOID_STATE),
                    ST_RIGHT(scan_state(
                        clause_sat=False, formula_sat=formula_sat
                    ))
                ),
                output_tape_no=VERDICT_TAPE,
                output_cell_state=VT_SAT if formula_sat else VT_UNSAT,
                annotation=f'VERDICT_{int(formula_sat)}'
            )

        _transitions_group.add_transition(
            input_terms=(VT_MID(VT_PENDING), CT_MID(CT_INVALID_INPUT)),
            output_tape_no=VERDICT_TAPE,
            output_cell_state=VT_UNSAT,
            annotation='VERDICT_INVALID_INPUT'
        )
        return _transitions_group

    def build_transitions_group(self) -> MultiTapeTransitionsGroup:
        transitions_group = self.build_scan_transitions_group()
        transitions_group = self.build_verdict_transitions_group(
            transitions_group=transitions_group
        )
        return transitions_group


@dataclass(frozen=True)
class Literal(object):
    variable: str
    negated: bool = False


@dataclass(frozen=True)
class Parsed3SAT(object):
    clauses: tuple[tuple[Literal, Literal, Literal], ...]

    @property
    def variable_names(self) -> tuple[str, ...]:
        names: list[str] = []
        for clause in self.clauses:
            for literal in clause:
                names.append(literal.variable)

        return tuple(sorted(set(names)))


def _parse_literal(raw_literal: str) -> Literal | None:
    literal = raw_literal.strip()
    if not literal:
        return None

    negated = False
    if literal[0] in ('~', '!'):
        negated = True
        literal = literal[1:].strip()

    if not _VAR_NAME_RE.match(literal):
        return None

    return Literal(variable=literal, negated=negated)


def parse_3sat_equation(equation: str) -> Parsed3SAT | None:
    """
    Parse a strict 3SAT equation in CNF form:
    (a|~b|c)&(~a|d|e)&...
    with optional whitespace.
    """
    expression = equation.strip()
    if not expression:
        return None

    clauses: list[tuple[Literal, Literal, Literal]] = []
    index = 0
    expr_len = len(expression)

    expect_clause = True

    while index < expr_len:
        while index < expr_len and expression[index].isspace():
            index += 1
        if index >= expr_len:
            break

        if expect_clause:
            if expression[index] != '(':
                return None

            close_idx = expression.find(')', index + 1)
            if close_idx < 0:
                return None

            inner_clause = expression[index + 1:close_idx]
            literal_chunks = [
                chunk.strip() for chunk in inner_clause.split('|')
            ]
            if len(literal_chunks) != 3:
                return None

            parsed_literals: list[Literal] = []
            for chunk in literal_chunks:
                literal = _parse_literal(chunk)
                if literal is None:
                    return None
                parsed_literals.append(literal)

            clauses.append(
                (parsed_literals[0], parsed_literals[1], parsed_literals[2])
            )
            index = close_idx + 1
            expect_clause = False
        else:
            if expression[index] != '&':
                return None
            index += 1
            expect_clause = True

    if not clauses:
        return None
    if expect_clause:
        return None

    return Parsed3SAT(clauses=tuple(clauses))


def parse_assignment(assignment: str) -> dict[str, bool] | None:
    """
    Parse assignment text:
    x1=1,x2=0,x3=true
    """
    raw_assignment = assignment.strip()
    if not raw_assignment:
        return None

    mapping: dict[str, bool] = {}
    pairs = raw_assignment.split(',')

    for pair in pairs:
        if '=' not in pair:
            return None
        raw_name, raw_value = pair.split('=', 1)
        variable = raw_name.strip()
        value = raw_value.strip().lower()

        if not _VAR_NAME_RE.match(variable):
            return None
        if variable in mapping:
            return None

        if value in ('1', 'true', 't'):
            mapping[variable] = True
        elif value in ('0', 'false', 'f'):
            mapping[variable] = False
        else:
            return None

    if not mapping:
        return None

    return mapping


@dataclass(frozen=True)
class ThreeSATEncoding(object):
    """
    Tape data encoding of a formula and assignment pair, laid out as one
    contiguous clause block per clause, where every clause block holds one
    cell per variable of the formula
    """
    num_clauses: int
    num_variables: int
    literal_states: tuple[TapeCellState, ...]
    assignment_states: tuple[TapeCellState, ...]

    @property
    def num_cells(self) -> int:
        return self.num_clauses * self.num_variables


def encode_3sat(
    parsed_equation: Parsed3SAT, assignment: dict[str, bool]
) -> ThreeSATEncoding | None:
    """
    Encode a formula and assignment pair into clause block tape data
    :param parsed_equation:
    :param assignment:
    :return:
    None if the assignment does not cover all formula variables
    """
    variable_names = parsed_equation.variable_names
    for variable_name in variable_names:
        if variable_name not in assignment:
            return None

    num_variables = len(variable_names)
    num_clauses = len(parsed_equation.clauses)
    variable_positions = {
        variable_name: index
        for index, variable_name in enumerate(variable_names)
    }

    literal_states = [VOID_STATE] * (num_clauses * num_variables)
    assignment_states = [VOID_STATE] * (num_clauses * num_variables)

    for clause_no, clause in enumerate(parsed_equation.clauses):
        block_start = clause_no * num_variables

        for variable_name in variable_names:
            cell_no = block_start + variable_positions[variable_name]
            assignment_states[cell_no] = (
                AT_TRUE if assignment[variable_name] else AT_FALSE
            )

        for literal in clause:
            cell_no = block_start + variable_positions[literal.variable]
            literal_state = LT_NEGATED if literal.negated else LT_POSITIVE
            prev_literal_state = literal_states[cell_no]

            if prev_literal_state == VOID_STATE:
                literal_states[cell_no] = literal_state
            elif prev_literal_state != literal_state:
                # the variable occurs both negated and un-negated
                literal_states[cell_no] = LT_EITHER

    return ThreeSATEncoding(
        num_clauses=num_clauses, num_variables=num_variables,
        literal_states=tuple(literal_states),
        assignment_states=tuple(assignment_states)
    )


class ThreeSATAutomataRunner(object):
    def __init__(self, equation: str, assignment: str):
        """
        3SAT automata instance whose tapes are populated with the clause
        block encoding of the :equation: and :assignment: pair
        :param equation:
        :param assignment:
        """
        self.equation = equation
        self.assignment = assignment
        self.invalid_reason: str | None = None
        self.encoding: ThreeSATEncoding | None = None

        parsed_equation = parse_3sat_equation(equation)
        parsed_assignment = parse_assignment(assignment)

        if parsed_equation is None:
            self.invalid_reason = 'INVALID_EQUATION'
        elif parsed_assignment is None:
            self.invalid_reason = 'INVALID_ASSIGNMENT'
        else:
            encoding = encode_3sat(parsed_equation, parsed_assignment)
            if encoding is None:
                missing_variables = [
                    name for name in parsed_equation.variable_names
                    if name not in parsed_assignment
                ]
                self.invalid_reason = (
                    f'MISSING_ASSIGNMENT:{",".join(missing_variables)}'
                )
            else:
                self.encoding = encoding

        self.parsed_equation = parsed_equation
        self.parsed_assignment = parsed_assignment

        self.builder = ThreeSATAutomataBuilder()
        self.transitions_group = self.builder.build_transitions_group()
        self.state_eq_map = MultiTapeRuleGenerator.generate_equations(
            self.transitions_group
        )
        self.multi_tape_automata = PyMultiTapeAutomata(self.state_eq_map)
        self.multi_tape_automata.init_tapes(
            tape_nos=[
                LITERALS_TAPE, ASSIGNMENTS_TAPE, CLAUSES_TAPE,
                SCAN_TAPE, VERDICT_TAPE
            ]
        )
        self._initialize_tapes()

    @property
    def invalid_input(self) -> bool:
        return self.encoding is None

    def _write_cell(self, position: int, tape_no: TapeNo, state: int):
        self.multi_tape_automata.write_region(
            position=position, end_position=position,
            data=[MultiTapeState(
                tape_no=tape_no, tape_cell_state=TapeCellState(state)
            )]
        )

    def _initialize_tapes(self):
        self._write_cell(VERDICT_POSITION, VERDICT_TAPE, VT_PENDING)

        if self.encoding is None:
            self._write_cell(
                VERDICT_POSITION, CLAUSES_TAPE, CT_INVALID_INPUT
            )
            return

        encoding = self.encoding
        for cell_no in range(encoding.num_cells):
            position = DATA_START_POSITION + cell_no
            literal_state = encoding.literal_states[cell_no]

            if literal_state != VOID_STATE:
                self._write_cell(position, LITERALS_TAPE, literal_state)

            self._write_cell(
                position, ASSIGNMENTS_TAPE,
                encoding.assignment_states[cell_no]
            )
            if cell_no % encoding.num_variables == 0:
                self._write_cell(position, CLAUSES_TAPE, CT_CLAUSE_START)

    def get_required_timesteps(self) -> int:
        """
        :return:
        Number of timesteps (including the initial one) needed for the
        automata to reach a verdict, which is one timestep to start the
        scan, one timestep per remaining clause block cell to move the
        scan accumulator leftwards, and one timestep to write the verdict
        """
        if self.encoding is None:
            return 2

        return self.encoding.num_cells + 2

    def step(self, verbose: bool = False) -> PyProcessStepResult:
        return self.multi_tape_automata.step(verbose=verbose)

    def read_verdict_state(self) -> TapeCellState:
        verdict_tape = self.multi_tape_automata[VERDICT_TAPE]
        return TapeCellState(verdict_tape.read(VERDICT_POSITION))

    def read_scan_state(self) -> tuple[bool, bool] | None:
        """
        :return:
        The (clause_sat, formula_sat) verdicts accumulated so far by the
        scan accumulator, or None when no scan accumulator is on the tapes
        """
        scan_tape = self.multi_tape_automata[SCAN_TAPE]
        min_position, max_position = scan_tape.get_range()

        for position in range(min_position, max_position + 1):
            scan_tape_state = scan_tape.read(position)
            if is_scan_state(scan_tape_state):
                return from_scan_state(scan_tape_state)

        return None

    def read_verdict(self) -> str:
        verdict_state = self.read_verdict_state()
        if verdict_state == VT_SAT:
            return 'SAT'
        if verdict_state == VT_UNSAT:
            return 'UNSAT'
        return 'PENDING'

    def is_satisfiable(self) -> bool:
        return self.read_verdict_state() == VT_SAT

    def run_simulation(
        self,
        num_timesteps: int = BLANK_INT,
        terminal_width: int = BLANK_INT,
        render_start: int = -3,
        render: bool = False,
    ):
        try:
            terminal_size = os.get_terminal_size()
            default_terminal_width = terminal_size.columns - 1
        except OSError:
            default_terminal_width = 100

        if terminal_width == BLANK_INT:
            terminal_width = default_terminal_width
        if num_timesteps == BLANK_INT:
            num_timesteps = self.get_required_timesteps()

        for timestep in range(num_timesteps):
            if timestep > 0:
                self.step(verbose=render)

            if render:
                frame = self.multi_tape_automata.render_tapes(
                    start_position=render_start,
                    length=terminal_width,
                    cell_width=2
                )
                print(f'\nTIMESTEP {timestep}:')
                print(frame.render())
                print(f'scan={self.read_scan_state()}')
                print(f'verdict={self.read_verdict()}')

    def evaluate(self, num_timesteps: int = BLANK_INT) -> str:
        self.run_simulation(num_timesteps=num_timesteps, render=False)
        return self.read_verdict()
