"""
Demonstrates that the assignment consistency check is done by the transition
rules and not by the encoder: the tape of an automata built from a perfectly
consistent assignment is tampered with, and the rules then reject it.

For the equation below there are two clause blocks of three variables, so the
group of x2 on the CONFLICT tape occupies positions 2 and 3, and both cells
hold whatever the assignment says x2 is (false). Overwriting the second cell
with the opposite value makes the two cells of one group disagree, which is
exactly what an assignment naming x2 with two different values looks like.
"""
from automata_builder.sat_automata import (
    CF_TRUE,
    CONFLICT_TAPE,
    VT_CONFLICTING,
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
print('assignment given to the runner is consistent:',
      runner.encoding.conflicting_variables == ())

# manually set up a conflict on the CONFLICT tape: position 2 = "x2 is false
# in block 0", position 3 = "x2 is true in block 1"
runner.multi_tape_automata.write_region(
    position=3, end_position=3,
    data=[
        MultiTapeState(
            tape_no=CONFLICT_TAPE,
            tape_cell_state=TapeCellState(int(CF_TRUE))
        ),
    ]
)
runner.run_simulation(render=True)

verdict_state = runner.read_verdict_state()
print(f'\nverdict={runner.read_verdict()}')
print(f'verdict_state={int(verdict_state)} '
      f'(VT_CONFLICTING={int(VT_CONFLICTING)})')
print(f'conflicting_assignment={runner.conflicting_assignment}')
