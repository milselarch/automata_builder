import dataclasses

from automata_builder._rust import PyMultiTapeProduct

from automata_builder.product_writes_map import FrozenProductWritesMap, ProductWritesMap
from automata_builder.tape_overlaps import FrozenTapeOverlaps, TapeOverlaps
from automata_builder.utils import FrozenSet, FreezableSet


@dataclasses.dataclass(frozen=True)
class TapeOverlapsFSMState(object):
    _tape_overlaps: FrozenTapeOverlaps
    _relevant_input_products: FrozenSet[PyMultiTapeProduct]
    _product_writes_map: FrozenProductWritesMap

    @property
    def tape_overlaps(self) -> FrozenTapeOverlaps:
        return self._tape_overlaps

    @property
    def relevant_input_products(self) -> FrozenSet[PyMultiTapeProduct]:
        return self._relevant_input_products

    @property
    def product_writes_map(self) -> FrozenProductWritesMap:
        return self._product_writes_map

    def __post_init__(self) -> None:
        if not self._tape_overlaps.is_frozen:
            raise ValueError("Tape overlaps needs to be frozen")
        if not self._relevant_input_products.is_frozen:
            raise ValueError("Input products need to be frozen")

    @classmethod
    def create(
        cls, tape_overlaps: TapeOverlaps,
        relevant_input_products: FreezableSet[PyMultiTapeProduct],
        product_writes_map: ProductWritesMap
    ):
        return cls(
            _tape_overlaps=tape_overlaps.to_frozen(),
            _relevant_input_products=relevant_input_products.to_frozen(),
            _product_writes_map=product_writes_map.to_frozen()
        )


class TapeOverlapsFSM(object):
    def __init__(self, initial_fsm_state: TapeOverlapsFSMState):
        self._initial_fsm_state = initial_fsm_state
        self._existing_fsm_states: set[TapeOverlapsFSMState] = {
            self._initial_fsm_state
        }
        self._next_fsm_states: dict[
            TapeOverlapsFSMState, TapeOverlapsFSMState
        ] = {}

    def __len__(self):
        return len(self._next_fsm_states)

    def __contains__(self, other):
        if not isinstance(other, TapeOverlapsFSMState):
            return False

        return other in self._existing_fsm_states

    def insert(
            self, state: TapeOverlapsFSMState,
            next_state: TapeOverlapsFSMState
    ) -> tuple[TapeOverlapsFSMState, bool]:
        """
        :param state:
        :param next_state:
        :return:
        frozen copy of inserted next_fsm_state, and whether
        the inserted fsm state are newly inserted into the FSM
        (i.e. they didn't already exist)
        """
        if state not in self._existing_fsm_states:
            raise ValueError(
                f'Overlaps FSM state "{state}" does not exist'
            )

        if state not in self._next_fsm_states:
            self._next_fsm_states[state] = next_state
            self._existing_fsm_states.add(next_state)
            assert next_state in self
            return next_state, True
        else:
            existing_next_fsm_state = self._next_fsm_states[state]

            if existing_next_fsm_state != next_state:
                raise ValueError(
                    f"Conflicting next state for {state=}: "
                    f"{existing_next_fsm_state=} vs {next_state=}"
                )
            return existing_next_fsm_state, False

    def merge(self) -> TapeOverlaps:
        return TapeOverlaps.merge([
            fsm_state.tape_overlaps
            for fsm_state in self._existing_fsm_states
        ])
