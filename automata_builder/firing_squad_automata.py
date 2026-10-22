import os

from automata_builder import _rust

from typing import Final

from automata_builder._rust import PySingleTapeAutomata
from automata_builder.rule_generator import (
    TapeCellState, TapeTransitionsGroup, RuleGenerator, BLANK_INT
)

X: Final[TapeCellState] = TapeCellState(0)  # void state
H: Final[TapeCellState] = TapeCellState(1)  # invalid / halt state

L: Final[TapeCellState] = TapeCellState(2)  # initial data state
A: Final[TapeCellState] = TapeCellState(3)
B: Final[TapeCellState] = TapeCellState(4)
C: Final[TapeCellState] = TapeCellState(5)
G: Final[TapeCellState] = TapeCellState(6)  # general state
F: Final[TapeCellState] = TapeCellState(7)  # firing state

# Maps current_state -> left_state -> right_state -> new_state
RULE_MATRICES: Final[dict[
    TapeCellState, dict[
        TapeCellState, dict[TapeCellState, TapeCellState]
    ]
]] = {
    L: {
        X: {X: H, L: L, A: H, B: H, C: H, G: H},
        L: {X: L, L: L, A: H, B: L, C: L, G: L},
        A: {X: C, L: G, A: L, B: L, C: L, G: C},
        B: {X: L, L: L, A: L, B: L, C: L, G: L},
        C: {X: G, L: A, A: L, B: L, C: L, G: G},
        G: {X: A, L: C, A: L, B: L, C: L, G: A},
    },
    A: {
        X: {X: H, L: H, A: F, B: H, C: G, G: H},
        L: {X: H, L: H, A: A, B: L, C: G, G: H},
        A: {X: F, L: A, A: A, B: B, C: C, G: B},
        B: {X: C, L: G, A: H, B: G, C: C, G: C},
        C: {X: H, L: A, A: A, B: H, C: H, G: H},
        G: {X: C, L: H, A: H, B: H, C: C, G: C},
    },
    B: {
        X: {X: H, L: H, A: H, B: H, C: H, G: H},
        L: {X: H, L: H, A: G, B: B, C: L, G: B},
        A: {X: H, L: G, A: B, B: B, C: L, G: H},
        B: {X: H, L: G, A: A, B: B, C: C, G: B},
        C: {X: L, L: L, A: A, B: H, C: H, G: L},
        G: {X: G, L: C, A: C, B: H, C: B, G: G},
    },
    C: {
        X: {X: H, L: H, A: H, B: H, C: H, G: H},
        L: {X: H, L: C, A: A, B: G, C: C, G: G},
        A: {X: B, L: B, A: H, B: B, C: H, G: B},
        B: {X: G, L: C, A: H, B: H, C: C, G: G},
        C: {X: H, L: C, A: A, B: B, C: C, G: B},
        G: {X: B, L: B, A: H, B: B, C: H, G: B},
    },
    G: {
        # we differ from the paper here in that
        # if there's just a general state surrounded with void
        # (so no soldiers), we transition to the firing state ([X][X} -> F)
        X: {X: F, L: A, A: H, B: G, C: G, G: F},
        L: {X: H, L: H, A: G, B: G, C: G, G: H},
        A: {X: H, L: B, A: H, B: G, C: G, G: H},
        B: {X: G, L: B, A: H, B: G, C: G, G: G},
        C: {X: A, L: A, A: H, B: G, C: G, G: A},
        G: {X: F, L: B, A: H, B: G, C: G, G: F},
    }
}


class FiringSquadAutomataRunner(object):
    def __init__(
        self, initial_write_start: int = 0,
        initial_write_end: int = 20,
    ):
        assert initial_write_end >= initial_write_start
        transitions_group = self.build_transitions_group()
        self.transitions_group = transitions_group
        self.state_eq_map = RuleGenerator.generate_equations(
            transitions_group=transitions_group
        )
        self.initial_write_start = initial_write_start
        self.initial_write_end = initial_write_end
        self.automata = PySingleTapeAutomata(
            state_eq_map=self.state_eq_map
        )
        self.automata.write_region(
            position=self.initial_write_start,
            end_position=self.initial_write_start,
            data=[TapeCellState(G)]
        )
        self.automata.write_region(
            position=self.initial_write_start+1,
            end_position=self.initial_write_end,
            data=[TapeCellState(L)]
        )

    def get_minimal_data_region(self) -> list[int]:
        return self.automata.get_minimal_data_region()

    @staticmethod
    def build_transitions_group() -> TapeTransitionsGroup:
        transitions_group = TapeTransitionsGroup(num_states=None)

        for middle_state, left_dict in RULE_MATRICES.items():
            for left_state, right_dict in left_dict.items():
                for right_state, new_middle_state in right_dict.items():
                    input_terms = (
                        _rust.A(position=-1, state=left_state),
                        _rust.A(position=0, state=middle_state),
                        _rust.A(position=1, state=right_state),
                    )
                    transitions_group.add_transition(
                        input_terms=input_terms,
                        output_state=new_middle_state
                    )

        return transitions_group

    def step(self, verbose: bool = False):
        self.automata.step(verbose=verbose)

    def run_simulation(
        self, num_timesteps: int = 30, terminal_width: int = BLANK_INT,
        render_start: int = -5, render: bool = True
    ):
        try:
            terminal_size = os.get_terminal_size()
            default_terminal_width = terminal_size.columns - 1
        except OSError:
            default_terminal_width = 100

        if terminal_width == BLANK_INT:
            terminal_width = default_terminal_width

        for timestep in range(num_timesteps):
            # print(f'{terminal_width=}')
            if timestep > 0:
                self.step(verbose=render)

            if render:
                render_frame = self.automata.render_tape(
                    start_position=render_start, length=terminal_width,
                    cell_width=2
                )
                # print(render_frame.get_dimensions())
                print(f'\nTIMESTEP {timestep}:')
                print(render_frame.render())
