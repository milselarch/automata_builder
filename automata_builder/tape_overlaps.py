from __future__ import annotations

import copy
import dataclasses
import typing

from collections import defaultdict
from typing import Iterator, Sequence
from automata_builder._rust import D, PyMultiTapeProduct

from automata_builder.rule_generator import (
    TapeCellState, TapeNo
)
from automata_builder.utils import (
    FreezableDefaultDict, FreezableSet, Freezable
)


@dataclasses.dataclass(frozen=True)
class MultiTapeState(object):
    """
    This represents the state of a cell in a specific tape of
    a multi-tape automaton
    Also this is technically the same as D, but without term position
    """
    tape_no: TapeNo
    tape_cell_state: TapeCellState

    def __hash__(self):
        return hash((self.tape_no, self.tape_cell_state))

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, MultiTapeState):
            return False

        return (
            self.tape_no == other.tape_no and
            self.tape_cell_state == other.tape_cell_state
        )

    def __gt__(self, other: MultiTapeState):
        if self.tape_no != other.tape_no:
            return self.tape_no > other.tape_no

        return self.tape_cell_state > other.tape_cell_state

    def to_tuple(self) -> tuple[int, int]:
        return int(self.tape_no), int(self.tape_cell_state)

    def to_str(self) -> str:
        tape_no = int(self.tape_no)
        tape_cell_state = int(self.tape_cell_state)
        str_state = f'T{tape_no}:{tape_cell_state}'
        return str_state

    def to_term(self, offset: int = 0) -> D:
        return D(
            position=offset,
            tape_no=self.tape_no, state=self.tape_cell_state
        )

    def has_tape_mutual_exclusion(self, other: MultiTapeState) -> bool:
        """
        Check if this state has tape mutual exclusion with the other state
        i.e. they are on the same tape but have different cell states
        :param other:
        :return:
        """
        return (
            self.tape_no == other.tape_no and
            self.tape_cell_state != other.tape_cell_state
        )

    @classmethod
    def from_term(cls, term: D):
        tape_no, tape_cell_state = term.get_state()
        return cls(
            tape_no=TapeNo(tape_no),
            tape_cell_state=TapeCellState(tape_cell_state)
        )


class MultiTapeStatesMap(object):
    """
    Intended to represent what tape states can occur at each
    tape from any possible source state at a given offset
    """
    def __init__(
        self, whitelist: defaultdict[TapeNo, set[TapeCellState]] | None = None
    ):
        if whitelist is None:
            _whitelist: defaultdict[
                TapeNo, set[TapeCellState]
            ] = defaultdict(set)
        else:
            _whitelist = whitelist

        self._whitelist: defaultdict[TapeNo, set[TapeCellState]] = _whitelist

    def __iter__(self) -> Iterator[TapeNo]:
        return iter(self._whitelist)

    def get_tape_nos(self) -> list[TapeNo]:
        return sorted(list(self._whitelist.keys()))

    def insert(self, tape_no: TapeNo, state: TapeCellState | MultiTapeState):
        if isinstance(state, MultiTapeState):
            assert tape_no == state.tape_no
            tape_cell_state: TapeCellState = state.tape_cell_state
        else:
            assert isinstance(state, TapeCellState)
            tape_cell_state: TapeCellState = state

        self._whitelist[tape_no].add(tape_cell_state)

    def __getitem__(self, item: TapeNo) -> set[TapeCellState]:
        return copy.copy(self._whitelist[item])

    def __setitem__(self, key: TapeNo, value: set[TapeCellState]):
        self._whitelist[key] = copy.copy(value)


InnerOverlaps = FreezableDefaultDict[
    MultiTapeState,
    FreezableDefaultDict[int, FreezableSet[MultiTapeState]]
]


class TapeOverlaps(Freezable):
    """
    We say that a tape state A can overlap with tape state B at offset k if
    in the history of the automata it is possible that:
    1. A is present at some position p on the tape and
    2. B to be present at position p + k.

    This structure stores all possible state overlaps

    Note that all overlaps are symmetrical, since if tape state A
    can overlap with tape state B at offset k,
    then tape state B can overlap with tape state A at offset -k
    """
    def __init__(self, overlaps: InnerOverlaps | None = None):
        """
        A: MultiTapeState -> B: int -> C: set[MultiTapeState]

        For a given source state A,
        the mapping gives all target states B that can
        be present at offset k from A, for all possible offsets k.
        """
        super().__init__()
        if overlaps is None:
            self._overlaps: InnerOverlaps = FreezableDefaultDict(
                lambda: FreezableDefaultDict(FreezableSet)
            )
        else:
            self._overlaps = overlaps

    def _freeze(self) -> None:
        self._overlaps.freeze()

    def _encode(self) -> tuple:
        return self._overlaps.encode()

    def __iter__(self) -> Iterator[MultiTapeState]:
        return iter(self._overlaps)

    def __getitem__(
        self, item: MultiTapeState
    ) -> FreezableDefaultDict[int, FreezableSet[MultiTapeState]]:
        # get the overlaps for a given source state
        return self._overlaps[item]

    @classmethod
    def _decode(cls, data: tuple) -> typing.Self:
        return cls(FreezableDefaultDict.decode(data))

    def __contains__(self, item: MultiTapeState):
        if not isinstance(item, MultiTapeState):
            raise TypeError(f'not {MultiTapeState.__name__}')

        return item in self._overlaps

    def __bool__(self) -> bool:
        return bool(self._overlaps)

    def __len__(self) -> int:
        return len(self._overlaps)

    def delete_state(self, state: MultiTapeState):
        if self._frozen:
            raise ValueError("Can't delete from frozen tape overlaps")

        if state in self._overlaps:
            del self._overlaps[state]

        for _, source_overlaps in self._overlaps.items():
            for _, offset_overlaps in source_overlaps.items():
                if state in offset_overlaps:
                    offset_overlaps.remove(state)

    def freeze(self) -> bool:
        self._frozen = True
        return self._overlaps.freeze()

    def freeze_copy(self) -> FrozenTapeOverlaps:
        return self.to_frozen()

    def encode(self):
        return self._overlaps.encode()

    def items(self):
        return self._overlaps.items()

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, TapeOverlaps):
            return False

        if self._overlaps.keys() != other._overlaps.keys():
            return False

        return self.encode() == other.encode()

    def __or__(self, other: object):
        if not isinstance(other, self.__class__):
            raise TypeError(
                f"Unsupported operand type(s) for |: "
                f"{other.__class__.__name__}"
            )

    @classmethod
    def merge(
        cls, input_tape_overlaps: Sequence[TapeOverlaps]
    ) -> TapeOverlaps:
        new_overlaps = cls()

        for tape_overlaps in input_tape_overlaps:
            for source_state, source_overlaps in tape_overlaps.items():
                for offset, offset_overlaps in source_overlaps.items():
                    for target_state in offset_overlaps:
                        new_overlaps.insert_direct_overlap(
                            source_state=source_state,
                            offset=offset, target_state=target_state
                        )

        return new_overlaps

    def get_cell_states_for_tape(self, tape_no: TapeNo) -> set[TapeCellState]:
        return set([
            state.tape_cell_state for state in
            self.get_states_for_tape(tape_no=tape_no)
        ])

    def group_states_by_tape(self) -> dict[TapeNo, set[MultiTapeState]]:
        tape_states: dict[TapeNo, set[MultiTapeState]] = defaultdict(set)

        for source_state in self._overlaps:
            tape_states[source_state.tape_no].add(source_state)

        return tape_states

    def get_states_for_tape(
        self, tape_no: TapeNo
    ) -> FreezableSet[MultiTapeState]:
        tape_states: set[MultiTapeState] = set()

        for source_state in self._overlaps:
            if source_state.tape_no == tape_no:
                tape_states.add(source_state)

        return FreezableSet(tape_states)

    def get_all_states(self) -> set[MultiTapeState]:
        return set(self._overlaps.keys())

    def visualize_for_states(
        self, source_states: set[MultiTapeState]
    ) -> list[str]:
        max_prefix_length = 0

        for source_state in source_states:
            tree_prefix = source_state.to_str()
            max_prefix_length = max(max_prefix_length, len(tree_prefix))

        lines = []
        source_states_seq = sorted(list(source_states))

        for source_state in source_states_seq:
            source_state_lines = self.visualize_for_state(
                source_state, prefix_length=max_prefix_length
            )
            lines.extend(source_state_lines)
            lines.append('')

        return lines

    def print_for_states(
        self, source_states: set[MultiTapeState] | None = None
    ) -> None:
        if source_states is None:
            _source_states = set(self._overlaps.keys())
        else:
            _source_states = set(source_states)

        lines = self.visualize_for_states(_source_states)
        print("\n".join(lines))

    @staticmethod
    def list_for_offset(
        target_states_set: set[MultiTapeState]
    ) -> str:
        # print(f"{target_states_set=}")
        sorted_target_states = sorted(list(target_states_set))
        tape_states_map: defaultdict[
            TapeNo, list[TapeCellState]
        ] = defaultdict(list)

        for sorted_target_state in sorted_target_states:
            tape_no = sorted_target_state.tape_no
            tape_cell_state = sorted_target_state.tape_cell_state
            tape_states_map[tape_no].append(tape_cell_state)

        tape_nos = sorted(list(tape_states_map.keys()))
        tape_chunks = []

        for tape_no in tape_nos:
            tape_cell_states = tape_states_map[tape_no]
            tape_cell_states_str = '|'.join([str(x) for x in tape_cell_states])
            tape_chunk = f'T{tape_no}:' + tape_cell_states_str
            tape_chunks.append(tape_chunk)

        target_states_str = ' '.join(tape_chunks)
        # print(f'{target_states_str=}')
        # print('')
        return target_states_str

    def visualize_for_state(
        self, source_state: MultiTapeState, prefix_length: int = 0
    ) -> list[str]:
        # print(source_state)
        tree_prefix = source_state.to_str()
        if prefix_length > len(tree_prefix):
            tree_prefix += ' ' * (prefix_length - len(tree_prefix))

        overlap_map = self._overlaps[source_state]
        overlap_offsets = set(overlap_map.keys())
        min_offset: int = 0
        max_offset: int = 0

        if overlap_offsets:
            min_offset = min(min(overlap_offsets), 0)
            max_offset = max(max(overlap_offsets), 0)

        offset_pad_length = max(len(str(min_offset)), len(str(max_offset)))
        mid_pre_line = tree_prefix + ' —'
        lines = []

        for offset in range(min_offset, max_offset + 1):
            if offset == 0:
                pre_line = mid_pre_line
            else:
                pre_line = " " * len(mid_pre_line)

            target_states_set: set[MultiTapeState] = (
                overlap_map[offset].clone_data()
            )
            if (offset != 0) and not target_states_set:
                # we want to not skip if offset=0 because
                # the offset=0 is also the one showing the tape state
                # lines.append(pre_line + f'|—')
                continue

            target_states_str = self.list_for_offset(target_states_set)

            if offset < 0:
                offset_str = str(offset)
            else:
                offset_str = f'+{offset}'

            if offset == min_offset:
                branch_char = '┌'
            elif offset == max_offset:
                branch_char = '└'
            else:
                branch_char = '|'

            offset_pad_chars = (offset_pad_length - len(offset_str)) * ' '
            offset_padded_str = offset_pad_chars + offset_str
            lines.append(
                pre_line +
                f'{branch_char}— {offset_padded_str}'
                f' —> {target_states_str}'
            )

        return lines

    def __repr__(self):
        return f'{self.__class__.__name__}(overlaps={self._overlaps})'

    def get_overlaps_for_offset(
        self, source_state: MultiTapeState, offset: int
    ) -> FreezableSet[MultiTapeState]:
        return self._overlaps[source_state][offset]

    def get_overlaps(
        self, source_state: MultiTapeState
    ) -> FreezableDefaultDict[int, FreezableSet[MultiTapeState]]:
        return copy.deepcopy(self._overlaps[source_state])

    def propagate_overlap(
        self, source_state: MultiTapeState, target_state: MultiTapeState,
        offset: int, min_offset: int, max_offset: int
    ) -> bool:
        source_updated = self.insert_overlaps_for(
            source_state=source_state, target_state=target_state,
            offset=offset, min_offset=min_offset, max_offset=max_offset
        )
        self.validate_mutual_overlaps_for(source_state=source_state)
        self.validate_symmetric_overlaps_for(source_state=source_state)

        target_updated = self.insert_overlaps_for(
            source_state=target_state, target_state=source_state,
            offset=-offset, min_offset=min_offset, max_offset=max_offset
        )
        self.validate_mutual_overlaps_for(source_state=source_state)
        self.validate_symmetric_overlaps_for(source_state=source_state)
        return source_updated or target_updated
        # return source_updated

    def validate_mutual_overlaps_for(
        self, source_state: MultiTapeState
    ) -> None:
        """
        Check that there are no overlaps with tape mutual exclusion
        i.e. if two states are on the same tape and same position
        but have different cell states, then they can't overlap
        :param source_state:
        :return:
        """
        source_overlaps_map = self._overlaps[source_state]
        direct_overlap_states = source_overlaps_map[0]  # direct overlap

        for overlap_state in direct_overlap_states:
            has_tape_mutual_exclusion = (
                source_state.tape_no == overlap_state.tape_no and
                source_state.tape_cell_state != overlap_state.tape_cell_state
            )
            if has_tape_mutual_exclusion:
                raise ValueError(
                    f"Invalid overlap between {source_state} and "
                    f"{overlap_state} at offset 0 due to tape mutual "
                    f"exclusion"
                )

    def validate_symmetric_overlaps_for(
        self, source_state: MultiTapeState
    ) -> None:
        """
        Every overlap should be symmetric:
        so if A overlaps with B at offset k,
        then B should also overlap with A at offset -k
        """
        source_overlaps_map = self._overlaps[source_state]

        for offset in source_overlaps_map:
            target_states = source_overlaps_map[offset]

            for target_state in target_states:
                target_overlaps_map = self._overlaps[target_state]
                symmetric_overlap_states = target_overlaps_map[-offset]

                if source_state not in symmetric_overlap_states:
                    raise ValueError(
                        f"Invalid asymmetric overlap: {source_state} overlaps "
                        f"with {target_state} at offset {offset}, but "
                        f"{target_state} does not overlap with "
                        f"{source_state} at offset {-offset}"
                    )

    def insert_overlaps_for(
        self, source_state: MultiTapeState, target_state: MultiTapeState,
        offset: int, min_offset: int | None, max_offset: int | None
    ) -> bool:
        """
        :param source_state:
        :param target_state:
        :param offset:
        offset of target_state FROM source_state
        :param min_offset:
        :param max_offset:
        :return:
        """
        source_cell_state = source_state.tape_cell_state
        # target_cell_state = target_state.tape_cell_state
        source_overlaps_map = self._overlaps[source_state]
        target_overlaps_map = self._overlaps[target_state]
        target_overlap_offsets = list(target_overlaps_map.keys())
        overlaps_inserted = False

        for target_state_offset in target_overlap_offsets:
            """
            If state A overlaps with state B at offset k,
            and state B overlaps with state C at offset m,
            then state A also overlaps with state C at offset k + m,
            
            So what we are doing here is to add all relevant states C 
            to the overlaps of state A at relevant (shifted) offsets 
            accordingly
            """
            source_state_offset = offset + target_state_offset
            # target_state_offset_overlap_inserted = False
            if (min_offset is not None) and source_state_offset < min_offset:
                continue
            if (max_offset is not None) and source_state_offset > max_offset:
                continue

            target_overlap_states = target_overlaps_map[target_state_offset]
            source_overlap_states = source_overlaps_map[source_state_offset]
            prev_target_overlap_states = copy.copy(target_overlap_states)
            # prev_source_overlap_states = copy.copy(source_overlap_states)

            for target_overlap_state in prev_target_overlap_states:
                # overlaps_snapshot = copy.deepcopy(self)\
                if target_overlap_state in source_overlap_states:
                    continue

                target_overlap_cell_state = (
                    target_overlap_state.tape_cell_state
                )
                has_tape_mutual_exclusion = (
                    source_state_offset == 0 and
                    target_overlap_cell_state != source_cell_state and
                    target_overlap_state.tape_no == source_state.tape_no
                )
                if has_tape_mutual_exclusion:
                    # if the states have same-tape mutual exclusion,
                    # then they can't overlap
                    continue

                # new_source_overlap_states.add(target_overlap_state)
                # source_overlap_states.add(target_overlap_state)
                overlaps_inserted |= self.insert_direct_overlap(
                    source_state=source_state,
                    target_state=target_overlap_state,
                    offset=source_state_offset
                )

                assert target_overlap_state in source_overlaps_map[
                    source_state_offset
                ]
                # target_state_offset_overlap_inserted = True
                """
                The target_overlap_state also would now overlap with 
                source_state at an offset of -offset
                """
                overlaps_inserted |= self.insert_direct_overlap(
                    source_state=target_overlap_state,
                    target_state=source_state,
                    offset=-offset
                )

                # TODO: insert source_state = source_overlap_state
                #  at target_offset?
                # TODO: loop source_overlap_state also
                """
                overlaps_inserted |= self.insert_direct_overlap(
                    source_state=source_state,
                    target_state=target_overlap_state,
                    offset=source_state_offset
                )
                """
                self.validate_mutual_overlaps_for(source_state)
                self.validate_symmetric_overlaps_for(source_state)

        has_tape_mutual_exclusion = (
            source_state.tape_no == target_state.tape_no and
            source_state.tape_cell_state != target_state.tape_cell_state and
            offset == 0
        )

        if not has_tape_mutual_exclusion:
            """
            if two multi tape states are on the same tape and 
            have different tape cell states then they must 
            necessarily never be able to overlap with one another directly 
            (i.e. with an overlap offset=0)
            """
            overlaps_inserted |= self.insert_direct_overlap(
                source_state=source_state, target_state=target_state,
                offset=offset
            )
            # self._overlaps[source_state][offset].add(target_state)
            # self._overlaps[target_state][-offset].add(source_state)
            self.validate_mutual_overlaps_for(source_state)
            # return False

        self.validate_mutual_overlaps_for(source_state)
        self.validate_symmetric_overlaps_for(source_state)
        return overlaps_inserted

    def insert_direct_overlap(
        self, source_state: MultiTapeState, target_state: MultiTapeState,
        offset: int
    ) -> bool:
        """
        If state A overlaps with state B at offset k,
        then state B also overlaps with state A at offset -k

        :param source_state:
        :param target_state:
        :param offset:
        offset of term with state_b FROM state_a
        one has to conscientious about direction
        :return:
        """
        if self._frozen:
            raise ValueError("Can't insert direct overlap when frozen")

        has_tape_mutual_exclusion = (
            source_state.tape_no == target_state.tape_no and
            source_state.tape_cell_state != target_state.tape_cell_state and
            offset == 0
        )
        if has_tape_mutual_exclusion:
            # states have same-tape mutual exclusion, so can't overlap
            return False

        source_overlaps = self._overlaps[source_state][offset]
        target_overlaps = self._overlaps[target_state][-offset]

        if target_state in source_overlaps:
            # overlap already exists
            return False

        source_overlaps.add(target_state)
        # assert source_state not in target_overlaps
        target_overlaps.add(source_state)
        self.validate_mutual_overlaps_for(source_state=source_state)
        self.validate_symmetric_overlaps_for(source_state=source_state)
        return True

    def remove_direct_overlap(
        self, source_state: MultiTapeState, target_state: MultiTapeState,
        offset: int
    ):
        if self._frozen:
            raise ValueError("Can't remove direct overlap when frozen")

        source_overlaps = self._overlaps[source_state][offset]
        target_overlaps = self._overlaps[target_state][-offset]

        if target_state in source_overlaps:
            source_overlaps.remove(target_state)
        if source_state not in target_overlaps:
            target_overlaps.remove(source_state)

        self.validate_mutual_overlaps_for(source_state=source_state)
        self.validate_symmetric_overlaps_for(source_state=source_state)
        return True

    def remove_overlaps_for(
        self, source_state: MultiTapeState, target_state: MultiTapeState,
        offset: int
    ) -> bool:
        """
        :param source_state:
        :param target_state:
        :param offset:
        offset of target_state FROM source_state
        :return:
        """
        source_overlaps_map = self._overlaps[source_state]
        target_overlaps_map = self._overlaps[target_state]
        source_overlap_states = source_overlaps_map[offset]
        target_overlap_states = target_overlaps_map[-offset]

        if target_state not in source_overlap_states:
            # overlap doesn't exist
            return False

        source_overlap_states.remove(target_state)
        target_overlap_states.remove(source_state)
        self.validate_mutual_overlaps_for(source_state=source_state)
        self.validate_symmetric_overlaps_for(source_state=source_state)
        return True

    def can_overlap_exist(
        self, source_state: MultiTapeState,
        target_state: MultiTapeState, offset: int,
        default_value: bool = False
    ) -> bool:
        """
        :param source_state:
        :param target_state:
        :param offset:
        offset of term with target_state FROM term with source_state
        :param default_value:
        :return:
        """
        if source_state not in self._overlaps:
            return default_value

        source_overlaps = self._overlaps[source_state]
        if offset not in source_overlaps:
            return default_value

        offset_overlaps = source_overlaps[offset]
        return target_state in offset_overlaps

    def create_whitelist_for_offset(
        self, offset: int = 0
    ) -> MultiTapeStatesMap:
        """
        For a given offset, create a whitelist of what tape states can
        exist at each tape for that offset

        If we leave offset at zero, this conveniently returns all the possible
        input tape states that can exist grouped by tape no.

        :param offset:
        :return:
        """
        whitelist = MultiTapeStatesMap()

        for source_state in self._overlaps:
            # source_tape_no = source_state.tape_no
            # source_tape_cell_state = source_state.tape_cell_state
            state_overlaps = self._overlaps[source_state]
            offset_state_overlaps = state_overlaps[offset]

            for offset_state_overlap in offset_state_overlaps:
                whitelist.insert(
                    tape_no=offset_state_overlap.tape_no,
                    state=offset_state_overlap
                )

        return whitelist

    def to_unfrozen(self):
        return self.__class__(
            overlaps=self._overlaps.to_unfrozen()
        )

    def to_frozen(self) -> FrozenTapeOverlaps:
        return FrozenTapeOverlaps(
            overlaps=self._overlaps.to_frozen()
        )

    def fits_within_whitelist(
        self, whitelist_overlaps: TapeOverlaps,
        verbose: bool = True
    ):
        def log(*args, **kwargs):
            if verbose:
                print(*args, **kwargs)

        for source_state in self:
            if source_state not in whitelist_overlaps:
                # TODO: explain skip here
                continue

            whitelisted_state_overlaps = whitelist_overlaps[source_state]
            source_state_overlaps = self[source_state]

            for offset in whitelisted_state_overlaps:
                offset_states = source_state_overlaps[offset].to_frozen()
                whitelisted_offset_states = whitelisted_state_overlaps[offset]
                whitelisted_offset_tapes = set([
                    state.tape_no for state in whitelisted_offset_states
                ])

                for target_state in offset_states:
                    if target_state in whitelisted_offset_states:
                        continue
                    elif target_state.tape_no not in whitelisted_offset_tapes:
                        # TODO: explain why we skip here
                        continue

                    log("ESCAPES", (source_state, target_state, offset))
                    return False

        return True

    def apply_whitelist(
        self, whitelist_overlaps: TapeOverlaps, verbose: bool = False
    ):
        """
        if there is overlap := (source_state, target_state, offset) in self
        but not in whitelist_overlaps, and whitelist_overlaps contains
        (source_state, states with tape_no = target_state.tape_no, offset)
        overlaps, then we remove said overlap.

        :param whitelist_overlaps:
        :param verbose:
        :return:
        """
        def log(*args, **kwargs):
            if verbose:
                print(*args, **kwargs)

        for source_state in self:
            if source_state not in whitelist_overlaps:
                """
                This means that whitelist_overlaps makes no claims
                on what states are whitelisted to coexist with source_state
                """
                continue

            whitelisted_state_overlaps = whitelist_overlaps[source_state]
            source_state_overlaps = self[source_state]

            for offset in whitelisted_state_overlaps:
                offset_states = source_state_overlaps[offset].to_frozen()
                whitelisted_offset_states = whitelisted_state_overlaps[offset]
                whitelisted_offset_tapes = set([
                    state.tape_no for state in whitelisted_offset_states
                ])

                for target_state in offset_states:
                    if target_state in whitelisted_offset_states:
                        continue
                    elif target_state.tape_no not in whitelisted_offset_tapes:
                        """
                        Any state in the automata *has* to overlap with some
                        other state on the same tape, otherwise it would be
                        impossible for that state to exist in the automata
                        at all.

                        So if we don't specify any target_state that
                        source_state can overlap with, we take it that the
                        overlaps makes no claims about what states can
                        overlap with source_state at that offset.
                        """
                        continue

                    # raise RuntimeError()
                    log(
                        f'REMOVE_OVERLAPS',
                        (source_state, target_state, offset)
                    )
                    self.remove_overlaps_for(
                        source_state=source_state,
                        target_state=target_state,
                        offset=offset
                    )


class FrozenTapeOverlaps(TapeOverlaps):
    def __init__(self, overlaps: InnerOverlaps):
        if not overlaps.is_frozen:
            raise ValueError("Tape overlaps need to be frozen")

        super().__init__(overlaps)
        self.freeze()

    def to_unfrozen(self) -> TapeOverlaps:
        return TapeOverlaps(
            overlaps=self._overlaps.to_unfrozen()
        )

    def __hash__(self) -> int:
        return hash(self._overlaps)


def is_product_satisfiable(
    product: PyMultiTapeProduct, overlaps: TapeOverlaps,
    default_if_term_missing: bool = False
) -> bool:
    """
    Check if the given product is satisfiable based on the
    overlaps that exist in the automata
    :param overlaps:
    :param product:
    :param default_if_term_missing:
    :return:
    """
    terms = product.get_flat_terms()

    if len(terms) == 1:
        # if overlaps contain the one term, then product is satisfiable
        tape_state = MultiTapeState.from_term(terms[0])
        return tape_state in overlaps

    for k in range(len(terms)-1):
        term_a, term_b = terms[k], terms[k+1]

        tape_no_a, tape_state_a = term_a.get_state()
        offset_a = term_a.get_position()
        output_state_a = MultiTapeState(
            tape_no=TapeNo(tape_no_a),
            tape_cell_state=TapeCellState(tape_state_a)
        )

        tape_no_b, tape_state_b = term_b.get_state()
        offset_b = term_b.get_position()
        output_state_b = MultiTapeState(
            tape_no=TapeNo(tape_no_b),
            tape_cell_state=TapeCellState(tape_state_b)
        )

        relative_offset = offset_b - offset_a
        overlap_exists = overlaps.can_overlap_exist(
            source_state=output_state_a, target_state=output_state_b,
            offset=relative_offset, default_value=default_if_term_missing
        )
        if not overlap_exists:
            """
            output_state_b cannot possibly be found at a position offset 
            of relative_offset from output_state_a
            """
            return False

    return True

