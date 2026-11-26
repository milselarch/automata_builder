from __future__ import annotations

import os
import re

from dataclasses import dataclass
from typing import Callable, Final

from automata_builder._rust import (
    D, PyMultiTapeAutomata, PyProcessStepResult, PyMultiTapeTransitionsGroup
)

from automata_builder.rule_generator import BLANK_INT
from automata_builder.rule_generator_multitape import (
    MultiTapeState,
    TapeCellState,
    TapeNo,
    VOID_STATE,
)

LITERALS_TAPE: Final[TapeNo] = TapeNo(0)
ASSIGNMENTS_TAPE: Final[TapeNo] = TapeNo(1)
CLAUSES_TAPE: Final[TapeNo] = TapeNo(2)
SCAN_TAPE: Final[TapeNo] = TapeNo(3)
VERDICT_TAPE: Final[TapeNo] = TapeNo(4)
CONFLICT_TAPE: Final[TapeNo] = TapeNo(5)
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

ASSIGNMENT CONSISTENCY CHECK (CONFLICT_TAPE)
============================================
Replicating the assignment vector once per clause block means the same
variable value is written down :num_clauses: times, so the tape could in
principle carry a different value for the same variable in two different
blocks (this is also what an assignment that gives the same variable two
conflicting values has to be encoded as). The automata must therefore
verify the replication itself instead of trusting whoever wrote the tape.

That check cannot be done on ASSIGNMENTS_TAPE directly: the copies of a
variable within one clause block are :num_variables: cells apart, and a
ruleset whose window may not grow with the input cannot compare two cells
at an unknown distance from one another. The copies are therefore also
laid out in tape order on CONFLICT_TAPE, one contiguous group per
variable, with one cell per clause block, all of which hold the value the
ASSIGNMENTS_TAPE carries for that variable in that block:

    CONFLICT_TAPE = [group(x1)][group(x2)]...[group(xn)]
    group(xv)     = [value of xv in block 0][value of xv in block 1]...

The first cell of every group is flagged as a group start, so two
neighbouring cells belong to the same variable if and only if the one on
the right is not a group start. That makes the consistency check local:
any two neighbours of the same group that disagree overwrite their cell
with CF_CONFLICT, and CF_CONFLICT then spreads leftwards one cell per
timestep. In parallel a consistency sweep starts on the rightmost cell of
the tape and also travels leftwards, marking every cell it passes as
CF_SETTLED, and it stops as soon as it runs into a conflict marker (which
travels ahead of it, so a conflict marker always reaches the verdict cell
first). The verdict cell is only resolved once one of the two waves gets
there, which is what makes the pending verdict wait for the check to
finish: CF_SETTLED means the replication was consistent and lets the scan
verdict through, CF_CONFLICT instead resolves it to VT_CONFLICTING
(reported as UNSAT).

Since a variable's group has at least as many cells as it has occurrences
in the input, an assignment that names the same variable twice with
different values puts the two values in different cells of the same
group, so it is rejected by the rules rather than by the parser.
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
            - bits[0] => whether all scanned clauses are satisfied

The spent cells keep the formula verdict the scan accumulated before it
moved on, so that the verdict cell can be resolved once the assignment
consistency check has swept over the whole tape as well, and not merely
while the scan accumulator happens to still be sitting on the leftmost
cell

Note that no cell state may be encoded as HALT_STATE (0b1), which is
reserved by the multi-tape framework, hence the offset bits everywhere
"""
ST_SPENT_FALSE: Final[TapeCellState] = TapeCellState(0b010)
ST_SPENT_TRUE: Final[TapeCellState] = TapeCellState(0b011)
ST_SPENT_STATES: Final[tuple[TapeCellState, ...]] = (
    ST_SPENT_FALSE, ST_SPENT_TRUE
)

VT_PENDING: Final[TapeCellState] = TapeCellState(0b10)
VT_UNSAT: Final[TapeCellState] = TapeCellState(0b11)
VT_SAT: Final[TapeCellState] = TapeCellState(0b100)
VT_CONFLICTING: Final[TapeCellState] = TapeCellState(0b101)
"""the assignment the automata was given contradicts itself"""

CF_FALSE: Final[TapeCellState] = TapeCellState(0b10)
CF_TRUE: Final[TapeCellState] = TapeCellState(0b11)
CF_FALSE_START: Final[TapeCellState] = TapeCellState(0b100)
CF_TRUE_START: Final[TapeCellState] = TapeCellState(0b101)
CF_CONFLICT: Final[TapeCellState] = TapeCellState(0b110)
"""spread leftwards once a variable was given two different values"""
CF_SETTLED: Final[TapeCellState] = TapeCellState(0b111)
"""the consistency sweep has reached this cell without finding a conflict"""

CF_VALUE_STATES: Final[tuple[TapeCellState, ...]] = (
    CF_FALSE, CF_TRUE, CF_FALSE_START, CF_TRUE_START
)
CF_GROUP_START_STATES: Final[tuple[TapeCellState, ...]] = (
    CF_FALSE_START, CF_TRUE_START
)

LITERAL_STATES: Final[tuple[TapeCellState, ...]] = (
    VOID_STATE, LT_POSITIVE, LT_NEGATED, LT_EITHER
)
ASSIGNMENT_STATES: Final[tuple[TapeCellState, ...]] = (AT_FALSE, AT_TRUE)
CLAUSE_STATES: Final[tuple[TapeCellState, ...]] = (VOID_STATE, CT_CLAUSE_START)
CONFLICT_STATES: Final[tuple[TapeCellState, ...]] = (
    VOID_STATE,) + CF_VALUE_STATES + (CF_CONFLICT, CF_SETTLED)

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

CF_MID: Final[Callable[[int], D]] = prefill_tape(MID, CONFLICT_TAPE)
CF_RIGHT: Final[Callable[[int], D]] = prefill_tape(RIGHT, CONFLICT_TAPE)


def conflict_cell_state(assigned: bool, is_group_start: bool) -> int:
    """
    Encodes one cell of the assignment replication laid out on the
    conflict tape

    :param assigned: value the cell claims the variable has in its block
    :param is_group_start: whether the cell starts its variable's group
    :return:
    """
    if is_group_start:
        return CF_TRUE_START if assigned else CF_FALSE_START

    return CF_TRUE if assigned else CF_FALSE


def is_conflict_value_state(cell_state: int) -> bool:
    """
    :param cell_state:
    :return:
    whether the conflict tape cell holds a variable value
    """
    return cell_state in CF_VALUE_STATES


def conflict_cell_value(cell_state: int) -> bool:
    """
    :param cell_state: conflict tape cell state holding a variable value
    :return:
    the truth value the cell claims the variable has
    """
    assert is_conflict_value_state(cell_state), cell_state
    return cell_state in (CF_TRUE, CF_TRUE_START)


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
        self, transitions_group: PyMultiTapeTransitionsGroup | None = None
    ) -> PyMultiTapeTransitionsGroup:
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
            _transitions_group = PyMultiTapeTransitionsGroup(
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
        # they are neither rescanned nor used to start a new scan, and keep
        # the formula verdict the accumulator had reached there
        for clause_sat in (False, True):
            for formula_sat in (False, True):
                _transitions_group.add_transition(
                    input_terms=(ST_MID(scan_state(
                        clause_sat=clause_sat, formula_sat=formula_sat
                    )),),
                    output_tape_no=SCAN_TAPE,
                    output_cell_state=(
                        ST_SPENT_TRUE if formula_sat else ST_SPENT_FALSE
                    ),
                    annotation=(
                        f'SCAN_SPEND_{int(clause_sat)}{int(formula_sat)}'
                    )
                )

        return _transitions_group

    @classmethod
    def build_conflict_transitions_group(
        cls, transitions_group: PyMultiTapeTransitionsGroup | None = None
    ) -> PyMultiTapeTransitionsGroup:
        """
        Builds the rules that verify that the assignment replication laid
        out on the conflict tape gives every variable one single value.

        Two neighbouring conflict tape cells belong to the same variable
        exactly when the cell on the right does not start a group, so a
        disagreement between two such cells is detectable from a two cell
        window, without knowing how many variables or clauses there are:
        such a cell is overwritten with a conflict marker, which then
        spreads leftwards one cell per timestep until it reaches the
        verdict cell.

        The remaining rules run the consistency sweep, which starts on the
        rightmost cell that carries assignment data and also travels
        leftwards one cell per timestep, marking the cells it passes as
        settled. A conflict marker always travels ahead of the sweep, and
        the sweep stops instead of overwriting it, so the verdict cell is
        told either that the replication was consistent or that it was
        not - and never that it has not been checked yet.

        :param transitions_group:
        :return:
        """
        if transitions_group is not None:
            _transitions_group = transitions_group
        else:
            _transitions_group = PyMultiTapeTransitionsGroup(
                require_annotation=True
            )

        for mid_state in CF_VALUE_STATES:
            for right_state in (CF_FALSE, CF_TRUE):
                # the cell on the right does not start a group, so both
                # cells describe the same variable and have to agree
                if conflict_cell_value(mid_state) == conflict_cell_value(
                    right_state
                ):
                    continue

                _transitions_group.add_transition(
                    input_terms=(
                        CF_MID(mid_state), CF_RIGHT(right_state)
                    ),
                    output_tape_no=CONFLICT_TAPE,
                    output_cell_state=CF_CONFLICT,
                    annotation=(
                        f'CONFLICT_DETECT_{mid_state}_{right_state}'
                    )
                )

        # carry the conflict marker towards the verdict cell, which sits
        # left of the leftmost cell of the leftmost group: through the
        # cells that still hold a variable value, and then into the verdict
        # cell itself, which is the only cell that has neither a value nor
        # a verdict yet, so that both waves stop there instead of running
        # on into the empty tape forever
        for mid_state in CF_VALUE_STATES:
            _transitions_group.add_transition(
                input_terms=(CF_MID(mid_state), CF_RIGHT(CF_CONFLICT)),
                output_tape_no=CONFLICT_TAPE,
                output_cell_state=CF_CONFLICT,
                annotation=f'CONFLICT_PROPAGATE_LEFT_{mid_state}'
            )

        _transitions_group.add_transition(
            input_terms=(
                CF_MID(VOID_STATE), CF_RIGHT(CF_CONFLICT),
                VT_MID(VT_PENDING), AT_MID(VOID_STATE)
            ),
            output_tape_no=CONFLICT_TAPE,
            output_cell_state=CF_CONFLICT,
            annotation='CONFLICT_REACH_VERDICT'
        )

        # the consistency sweep starts on the rightmost cell that carries
        # assignment data and travels leftwards one cell per timestep,
        # marking the cells it passes as settled so that the verdict cell
        # can tell whether the whole replication has been checked already
        for mid_state in CF_VALUE_STATES:
            _transitions_group.add_transition(
                input_terms=(CF_MID(mid_state), CF_RIGHT(VOID_STATE)),
                output_tape_no=CONFLICT_TAPE,
                output_cell_state=CF_SETTLED,
                annotation=f'CONFLICT_SWEEP_START_{mid_state}'
            )

        for mid_state in CF_VALUE_STATES:
            # a cell that already holds a conflict marker stops the sweep,
            # so that the marker is the one that reaches the verdict cell
            _transitions_group.add_transition(
                input_terms=(CF_MID(mid_state), CF_RIGHT(CF_SETTLED)),
                output_tape_no=CONFLICT_TAPE,
                output_cell_state=CF_SETTLED,
                annotation=f'CONFLICT_SWEEP_LEFT_{mid_state}'
            )

        _transitions_group.add_transition(
            input_terms=(
                CF_MID(VOID_STATE), CF_RIGHT(CF_SETTLED),
                VT_MID(VT_PENDING), AT_MID(VOID_STATE)
            ),
            output_tape_no=CONFLICT_TAPE,
            output_cell_state=CF_SETTLED,
            annotation='CONFLICT_SWEEP_REACH_VERDICT'
        )

        return _transitions_group

    @classmethod
    def build_verdict_transitions_group(
        cls, transitions_group: PyMultiTapeTransitionsGroup | None = None
    ) -> PyMultiTapeTransitionsGroup:
        """
        Builds the rules that resolve the pending verdict cell once both
        the scan accumulator has run past the leftmost clause block cell
        and the assignment consistency sweep has run past it, as well as
        the rules that reject input that could not be encoded and input
        whose assignment contradicts itself.

        :param transitions_group:
        :return:
        """
        if transitions_group is not None:
            _transitions_group = transitions_group
        else:
            _transitions_group = PyMultiTapeTransitionsGroup(
                require_annotation=True
            )

        for formula_sat in (False, True):
            # the scan is complete once the accumulator has been spent on
            # the leftmost clause block cell, i.e. the cell without
            # assignment data on the side the accumulator was travelling
            # towards, and the replication is checked once the sweep has
            # marked the verdict cell as settled; a conflict marker instead
            # stops the sweep, so that it always wins over this rule
            _transitions_group.add_transition(
                input_terms=(
                    VT_MID(VT_PENDING), AT_MID(VOID_STATE),
                    ST_RIGHT(
                        ST_SPENT_TRUE if formula_sat else ST_SPENT_FALSE
                    ),
                    CF_MID(CF_SETTLED)
                ),
                output_tape_no=VERDICT_TAPE,
                output_cell_state=VT_SAT if formula_sat else VT_UNSAT,
                annotation=f'VERDICT_{int(formula_sat)}'
            )

        # a variable that was given two different values cannot be
        # evaluated at all, so it overrides whatever the scan computed
        _transitions_group.add_transition(
            input_terms=(VT_MID(VT_PENDING), CF_MID(CF_CONFLICT)),
            output_tape_no=VERDICT_TAPE,
            output_cell_state=VT_CONFLICTING,
            annotation='VERDICT_CONFLICTING_ASSIGNMENT'
        )

        _transitions_group.add_transition(
            input_terms=(VT_MID(VT_PENDING), CT_MID(CT_INVALID_INPUT)),
            output_tape_no=VERDICT_TAPE,
            output_cell_state=VT_UNSAT,
            annotation='VERDICT_INVALID_INPUT'
        )
        return _transitions_group

    def build_transitions_group(self) -> PyMultiTapeTransitionsGroup:
        transitions_group = self.build_scan_transitions_group()
        transitions_group = self.build_conflict_transitions_group(
            transitions_group=transitions_group
        )
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


@dataclass(frozen=True)
class ParsedAssignment(object):
    """
    Every (variable, value) pair an assignment text consists of, in the
    order they were given.

    A variable is allowed to be named more than once: rejecting that at
    parse time would hide the contradiction from the automata, which is
    what has to decide whether the assignment is usable at all.
    """
    pairs: tuple[tuple[str, bool], ...]

    @property
    def variable_names(self) -> tuple[str, ...]:
        return tuple(sorted({variable for variable, _ in self.pairs}))

    def occurrences(self, variable: str) -> tuple[bool, ...]:
        """
        :param variable:
        :return:
        every value given for the variable, in the order they were given
        """
        values = tuple(
            value for name, value in self.pairs if name == variable
        )
        assert values, variable
        return values

    def is_conflicting(self, variable: str) -> bool:
        """
        :param variable:
        :return:
        whether the variable was given two different values
        """
        return len(set(self.occurrences(variable))) > 1

    @property
    def conflicting_variables(self) -> tuple[str, ...]:
        return tuple(
            variable for variable in self.variable_names
            if self.is_conflicting(variable)
        )

    def __contains__(self, variable: str) -> bool:
        return any(name == variable for name, _ in self.pairs)

    def __getitem__(self, variable: str) -> bool:
        return self.occurrences(variable)[0]


def parse_assignment(assignment: str) -> ParsedAssignment | None:
    """
    Parse assignment text:
    x1=1,x2=0,x3=true

    Naming the same variable twice is accepted here, because deciding
    whether the resulting assignment is usable is the job of the automata
    rules that check the assignment replication, not of the parser.
    """
    raw_assignment = assignment.strip()
    if not raw_assignment:
        return None

    parsed_pairs: list[tuple[str, bool]] = []
    pairs = raw_assignment.split(',')

    for pair in pairs:
        if '=' not in pair:
            return None
        raw_name, raw_value = pair.split('=', 1)
        variable = raw_name.strip()
        value = raw_value.strip().lower()

        if not _VAR_NAME_RE.match(variable):
            return None

        if value in ('1', 'true', 't'):
            parsed_pairs.append((variable, True))
        elif value in ('0', 'false', 'f'):
            parsed_pairs.append((variable, False))
        else:
            return None

    if not parsed_pairs:
        return None

    return ParsedAssignment(pairs=tuple(parsed_pairs))


@dataclass(frozen=True)
class ThreeSATEncoding(object):
    """
    Tape data encoding of a formula and assignment pair, laid out as one
    contiguous clause block per clause, where every clause block holds one
    cell per variable of the formula, plus the same assignment replication
    laid out with one contiguous group per variable on the conflict tape
    """
    num_clauses: int
    num_variables: int
    literal_states: tuple[TapeCellState, ...]
    assignment_states: tuple[TapeCellState, ...]
    conflict_states: tuple[TapeCellState, ...]
    conflicting_variables: tuple[str, ...] = ()

    @property
    def num_cells(self) -> int:
        return self.num_clauses * self.num_variables

    @property
    def num_conflict_cells(self) -> int:
        return len(self.conflict_states)



def encode_3sat(
    parsed_equation: Parsed3SAT, assignment: ParsedAssignment
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

    # a variable that was given more than one value has no single value to
    # put in a cell, so its occurrences are spread over the clause blocks
    # instead, which is what tells the consistency check that they disagree
    occurrences = {
        variable_name: assignment.occurrences(variable_name)
        for variable_name in variable_names
    }

    literal_states = [VOID_STATE] * (num_clauses * num_variables)
    assignment_states = [VOID_STATE] * (num_clauses * num_variables)
    conflict_states: list[TapeCellState] = []
    conflicting_variables = tuple(
        variable_name for variable_name in variable_names
        if assignment.is_conflicting(variable_name)
    )

    for clause_no, clause in enumerate(parsed_equation.clauses):
        block_start = clause_no * num_variables

        for variable_name in variable_names:
            cell_no = block_start + variable_positions[variable_name]
            values = occurrences[variable_name]
            assignment_states[cell_no] = (
                AT_TRUE if values[clause_no % len(values)] else AT_FALSE
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

    for variable_name in variable_names:
        values = occurrences[variable_name]

        # one cell per clause block, so that a value that disagrees with
        # the value of the same variable in another block has a neighbour
        # to disagree with; naming a variable more often than there are
        # clauses must not let an occurrence escape the check
        group_size = max(num_clauses, len(values))
        for occurrence_no in range(group_size):
            conflict_states.append(conflict_cell_state(
                assigned=values[occurrence_no % len(values)],
                is_group_start=occurrence_no == 0
            ))

    return ThreeSATEncoding(
        num_clauses=num_clauses, num_variables=num_variables,
        literal_states=tuple(literal_states),
        assignment_states=tuple(assignment_states),
        conflict_states=tuple(conflict_states),
        conflicting_variables=conflicting_variables
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
        self.state_eq_map = self.transitions_group.generate_equations()
        self.multi_tape_automata = PyMultiTapeAutomata(self.transitions_group)
        self.multi_tape_automata.init_tapes(
            tape_nos=[
                LITERALS_TAPE, ASSIGNMENTS_TAPE, CLAUSES_TAPE,
                SCAN_TAPE, VERDICT_TAPE, CONFLICT_TAPE
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

        # the assignment replication again, but grouped per variable so
        # that copies of the same variable end up next to each other
        for cell_no, conflict_state in enumerate(encoding.conflict_states):
            self._write_cell(
                DATA_START_POSITION + cell_no, CONFLICT_TAPE, conflict_state
            )

    def get_required_timesteps(self) -> int:
        """
        :return:
        Number of timesteps (including the initial one) needed for the
        automata to reach a verdict.

        The scan takes one timestep to start, one timestep per remaining
        clause block cell to move the accumulator leftwards and one to
        spend the cell it ends up on. The consistency sweep starts on the
        rightmost cell at the same time as the scan and moves leftwards
        one cell per timestep, so it needs one timestep per conflict tape
        cell to reach the verdict cell. Whichever of the two finishes
        last decides the verdict one timestep later, and since a variable
        group holds at least one cell per clause block the sweep is
        normally the slower of the two.
        """
        if self.encoding is None:
            return 2

        return max(
            self.encoding.num_cells + 3,
            self.encoding.num_conflict_cells + 3
        )

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

    def read_conflict_state(
        self, position: int = DATA_START_POSITION
    ) -> TapeCellState:
        """
        :param position:
        :return:
        the conflict tape cell state at the position
        """
        conflict_tape = self.multi_tape_automata[CONFLICT_TAPE]
        return TapeCellState(conflict_tape.read(position))

    def read_verdict(self) -> str:
        verdict_state = self.read_verdict_state()
        if verdict_state == VT_SAT:
            return 'SAT'
        if verdict_state in (VT_UNSAT, VT_CONFLICTING):
            return 'UNSAT'
        return 'PENDING'

    @property
    def conflicting_assignment(self) -> bool:
        """
        :return:
        whether the automata rejected the assignment because it gives some
        variable two different values, which is decided by the transition
        rules and not by the parser
        """
        return self.read_verdict_state() == VT_CONFLICTING

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
