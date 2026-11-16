from automata_builder.sat_automata import (
    ASSIGNMENTS_TAPE,
    AT_FALSE,
    AT_TRUE,
    CLAUSES_TAPE,
    CONFLICT_TAPE,
    CF_CONFLICTING,
    CT_CLAUSE_START,
    LITERALS_TAPE,
    LT_EITHER,
    VT_CONFLICTING,
    VT_SAT,
    VT_UNSAT,
    ThreeSATAutomataBuilder,
    ThreeSATAutomataRunner,
)
from automata_builder.rule_generator_multitape import (
    MultiTapeState,
    TapeCellState,
)

runner = ThreeSATAutomataRunner(
    equation='(x1|x2|x3)&(~x1|~x2|x3)',
    assignment='x1=1,x2=0,x3=0',
)
# Manually set up a conflict on the CONFLICT tape:
# position 0 = AT_TRUE, position 1 = AT_FALSE
# This simulates x1 being assigned true in one block and false in another.
runner.multi_tape_automata.write_region(
    position=0, end_position=1,
    data=[
        MultiTapeState(
            tape_no=CONFLICT_TAPE,
            tape_cell_state=TapeCellState(int(AT_TRUE))
        ),
        MultiTapeState(
            tape_no=CONFLICT_TAPE,
            tape_cell_state=TapeCellState(int(AT_FALSE))
        ),
    ]
)
runner.run_simulation(render=True)
