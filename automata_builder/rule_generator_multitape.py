from __future__ import annotations

import copy
import dataclasses

from automata_builder import utils
from automata_builder.utils import PopRestorableList

from result import Result, Ok, Err
from collections import defaultdict
from typing import Sequence

from automata_builder.product_writes_map import ProductWritesMap
from automata_builder.tape_overlaps_fsm import (
    TapeOverlapsFSMState, TapeOverlapsFSM
)
from automata_builder.utils import FreezableSet, FrozenSet
from automata_builder.rule_generator import (
    TapeTransitionsGroup, TapeCellState, TapeNo,
    VOID_STATE, HALT_STATE
)
from automata_builder.tape_overlaps import (
    MultiTapeState, TapeOverlaps, MultiTapeStatesMap,
    FrozenTapeOverlaps, is_product_satisfiable
)
from automata_builder._rust import (
    D, PyMultiTapeProduct, PyMultiTapeExpression,
    A, PyProduct, PyMultiTapeAutomata
)


@dataclasses.dataclass(frozen=True)
class MultiTapeTransition(object):
    input_terms: tuple[D, ...]
    output_state: MultiTapeState
    annotation: str = ''


@dataclasses.dataclass
class MultiTapeTransitionsGroup(object):
    """
    contains a set of transitions for a multi-tape cellular automaton
    defined as a mapping from input states to output state
    map D[] -> (output tape_no, output state)
    """
    transitions: list[MultiTapeTransition] = dataclasses.field(
        default_factory=list
    )
    require_annotation: bool = False

    def __len__(self):
        return len(self.transitions)

    def add_transition(
        self, input_terms: tuple[D, ...],
        output_tape_no: int, output_cell_state: int,
        validate_void: bool = True,
        validate_halt: bool = True,
        annotation: str = ''
    ):
        """
        :param input_terms:
        :param output_tape_no:
        :param output_cell_state:
        :param validate_void:
        If true, check that the input terms do not all have a void state
        :param validate_halt:
        If true, check that the halt state is not within input terms
        :param annotation:
        :return:
        """
        if self.require_annotation and not annotation:
            raise ValueError(f'Annotation expected')

        if validate_void:
            is_all_void = True

            for term in input_terms:
                if term.get_state() != VOID_STATE:
                    is_all_void = False

            if is_all_void:
                raise ValueError(
                    f"Input terms are all void, which is not "
                    f"allowed since it would make the simulation range "
                    f"infinite"
                )
        if validate_halt:
            for term in input_terms:
                if term.get_state() != HALT_STATE:
                    continue

                raise ValueError(
                    f"Input term {term} has halt state, which is not "
                    f"allowed since it has predefined behavior"
                )

        output_state = MultiTapeState(
            tape_no=TapeNo(output_tape_no),
            tape_cell_state=TapeCellState(output_cell_state)
        )
        transition = MultiTapeTransition(
            input_terms=input_terms,
            output_state=output_state,
            annotation=annotation
        )
        self.transitions.append(transition)

    def __or__(
        self, other: MultiTapeTransitionsGroup
    ) -> MultiTapeTransitionsGroup:
        if not isinstance(other, MultiTapeTransitionsGroup):
            raise TypeError(f'unexpected type {type(other)}')

        require_annotation = (
            self.require_annotation or other.require_annotation
        )
        if require_annotation:
            if not self.require_annotation:
                raise ValueError(
                    "Cannot combine transitions while other group "
                    "does not require annotation"
                )
            elif not other.require_annotation:
                raise ValueError(
                    "Cannot combine transitions while own group "
                    "requires annotation"
                )

        combined = self.__class__(require_annotation=require_annotation)
        combined.transitions.extend(copy.deepcopy(self.transitions))
        combined.transitions.extend(copy.deepcopy(other.transitions))
        return combined


class MultiTapeRuleGenerator(object):
    @staticmethod
    def terms_to_product(
        terms: tuple[D, ...], annotation: str
    ) -> PyMultiTapeProduct:
        return PyMultiTapeProduct(
            terms=terms, annotation=annotation
        )

    @staticmethod
    def aggregate_bit_or(expr_list: list[
        PyMultiTapeExpression | PyMultiTapeProduct
    ]) -> PyMultiTapeExpression:
        if not expr_list:
            return PyMultiTapeExpression()

        result = expr_list[0]
        for k in range(1, len(expr_list)):
            result = result | expr_list[k]

        return result.to_py_expression()

    @classmethod
    def generate_equations(
        cls, transitions_group: MultiTapeTransitionsGroup,
        require_annotations: bool = False
    ) -> dict[MultiTapeState, PyMultiTapeExpression]:
        state_eq_terms_map: dict[
            MultiTapeState, list[PyMultiTapeProduct]
        ] = {}

        for transition in transitions_group.transitions:
            input_states = transition.input_terms
            output_state = transition.output_state
            annotation = transition.annotation

            if output_state not in state_eq_terms_map:
                state_eq_terms_map[output_state] = []

            product = cls.terms_to_product(input_states, annotation)
            state_eq_terms_map[output_state].append(product)
            if require_annotations:
                assert product.get_annotation()

        state_eq_map: dict[MultiTapeState, PyMultiTapeExpression] = {
            next_state: cls.aggregate_bit_or(state_eq_terms_map[next_state])
            for next_state in state_eq_terms_map
        }
        for next_state in state_eq_map:
            expr = state_eq_map[next_state]
            flat_products = expr.get_flat_products()

            if not flat_products:
                raise ValueError(
                    f"Output state {next_state} has no products in "
                    f"its expression {expr}"
                )

            for product in flat_products:
                if require_annotations:
                    assert product.get_annotation()

        return state_eq_map


def zigzag_sort_key(num: int | None):
    if num is None:
        return float('inf'), float('inf')

    return abs(num), num < 0


def zigzag_term_sort_key(term: D):
    """
    Sorting key to sort terms by position in zigzag order
    (higher absolute value first, sign of offset second)
    0, -1, 1, -2, 2, -3, 3, ...
    :param term:
    :return:
    """
    return zigzag_sort_key(term.get_position()), term


@dataclasses.dataclass
class OffsetGroupedTerms(object):
    terms: tuple[D, ...]
    offset: int | None = None

    def __hash__(self):
        return hash((self.terms, self.offset))

    def __eq__(self, other: object):
        if not isinstance(other, OffsetGroupedTerms):
            return False

        return (self.terms, self.offset) == (other.terms, other.offset)

    def __bool__(self):
        return bool(self.terms)

    @property
    def is_blank(self):
        return not self.terms

    @classmethod
    def blank(cls):
        return cls(terms=())

    def __post_init__(self):
        if self.offset is None and self.terms:
            raise ValueError(
                f"OffsetGroupedTerms without offset cannot have terms"
            )

        assert list(self.terms) == sorted(self.terms)


@dataclasses.dataclass
class MultiTapeStateTrie(object):
    offset_group: OffsetGroupedTerms | None = None
    # whether any nested trie contains an offset group
    # (i.e., not just a blank group)
    has_nested_offset_group: bool = False

    next_tries: defaultdict[
        MultiTapeState, MultiTapeStateTrie
    ] = dataclasses.field(
        default_factory=lambda: defaultdict(MultiTapeStateTrie)
    )

    @property
    def has_offset_group(self):
        return (
            self.offset_group is not None or
            self.has_nested_offset_group
        )

    def __or__(self, other: MultiTapeStateTrie) -> MultiTapeStateTrie:
        if self.offset_group is None:
            offset_group = other.offset_group
        elif other.offset_group is None:
            offset_group = self.offset_group
        elif self.offset_group == other.offset_group:
            offset_group = self.offset_group
        else:
            raise ValueError(
                f"offset_group mismatch: "
                f"{self.offset_group} vs {other.offset_group}"
            )

        has_nested_offset_group = (
            self.has_nested_offset_group or other.has_nested_offset_group
        )
        next_tries: defaultdict[
            MultiTapeState, MultiTapeStateTrie
        ] = defaultdict(MultiTapeStateTrie)

        for state in self.next_tries:
            next_tries[state] = self.next_tries[state]
        for state in other.next_tries:
            next_tries[state] = next_tries[state] | other.next_tries[state]

        return MultiTapeStateTrie(
            offset_group=offset_group,
            has_nested_offset_group=has_nested_offset_group,
            next_tries=next_tries
        )

    def __and__(self, other: MultiTapeStateTrie) -> MultiTapeStateTrie:
        offset_group = None
        if self.offset_group == other.offset_group:
            offset_group = self.offset_group

        has_nested_offset_group = False
        next_tries: defaultdict[
            MultiTapeState, MultiTapeStateTrie
        ] = defaultdict(MultiTapeStateTrie)

        for state in self.next_tries:
            if state not in other.next_tries:
                continue

            next_trie = self.next_tries[state]
            merged_trie = next_trie & other.next_tries[state]
            if merged_trie.has_offset_group:
                next_tries[state] = merged_trie
                has_nested_offset_group = True

        return MultiTapeStateTrie(
            offset_group=offset_group,
            has_nested_offset_group=has_nested_offset_group,
            next_tries=next_tries
        )

    @classmethod
    def merge(cls, tries: list[MultiTapeStateTrie]) -> MultiTapeStateTrie:
        return cls._merge(tries[::])

    @classmethod
    def _merge(cls, tries: list[MultiTapeStateTrie]) -> MultiTapeStateTrie:
        assert len(tries) > 0
        if len(tries) == 1:
            return tries[0]

        last_trie = tries.pop()
        others_merged = cls._merge(tries)
        return others_merged | last_trie

    def insert_group(self, group: OffsetGroupedTerms):
        self._insert_group(group=group)

    def _insert_group(
        self, group: OffsetGroupedTerms,
        rev_states: list[MultiTapeState] | None = None,
    ):
        if rev_states is None:
            states = [MultiTapeState.from_term(term) for term in group.terms]
            rev_states = states[::-1]
        if not rev_states:
            self.offset_group = group
            return

        self.has_nested_offset_group = True
        next_state = rev_states.pop()
        assert isinstance(next_state, MultiTapeState), next_state
        self.next_tries[next_state]._insert_group(
            group=group, rev_states=rev_states
        )

    def lookup(
        self, states: list[MultiTapeState]
    ) -> OffsetGroupedTerms:
        """
        Finds the smallest covering offset group that contains
        some subset of the input states
        :param states:
        :return:
        """
        rev_sorted_states = PopRestorableList(sorted(states)[::-1])
        return self._lookup(rev_states=rev_sorted_states)

    def lookup_all(
        self, states: list[MultiTapeState]
    ) -> set[OffsetGroupedTerms]:
        """
        Finds all covering offset groups that contain
        some subset of the input states
        :param states:
        :return:
        """
        rev_sorted_states = PopRestorableList(sorted(states)[::-1])
        return self._lookup_all(rev_states=rev_sorted_states)

    def _lookup(
        self, rev_states: PopRestorableList[MultiTapeState]
    ) -> OffsetGroupedTerms:
        """
        Given a list of MultiTapeStates sorted in reverse,
        find the smallest covering offset group that contains
        some subset of the input states
        :param rev_states:
        :return:
        """
        if self.offset_group is not None:
            return self.offset_group

        with rev_states.undo_pops_when_done():
            while rev_states:
                next_state = rev_states.pop()
                if next_state not in self.next_tries:
                    continue

                next_trie = self.next_tries[next_state]
                resolved_group = next_trie._lookup(rev_states)
                if resolved_group.is_blank:
                    continue

                return resolved_group

        return OffsetGroupedTerms.blank()

    def _lookup_all(
        self, rev_states: PopRestorableList[MultiTapeState]
    ) -> set[OffsetGroupedTerms]:
        """
        Given a list of MultiTapeStates sorted in reverse,
        find all covering offset groups that contain
        some subset of the input states
        :param rev_states:
        :return:
        """
        if self.offset_group is not None:
            return { self.offset_group }

        resolved_groups: set[OffsetGroupedTerms] = set()

        with rev_states.undo_pops_when_done():
            while rev_states:
                next_state = rev_states.pop()
                if next_state not in self.next_tries:
                    continue

                next_trie = self.next_tries[next_state]
                sub_resolved_groups = next_trie._lookup_all(rev_states)
                resolved_groups |= sub_resolved_groups

        return resolved_groups

    def get_all(self) -> set[OffsetGroupedTerms]:
        resolved_groups: set[OffsetGroupedTerms] = set()
        if self.offset_group is not None:
            resolved_groups.add(self.offset_group)

        for next_state in self.next_tries:
            next_trie = self.next_tries[next_state]
            resolved_groups |= next_trie.get_all()

        return resolved_groups


@dataclasses.dataclass
class MultiTapeProductTrie(object):
    """
    A trie of product terms nested from smallest to largest term offset
    """
    offset: int | None = None
    end_products: set[PyMultiTapeProduct] = dataclasses.field(
        default_factory=set
    )
    # whether any nested trie contains an end product
    has_nested_products: bool = False
    # map next offset to current groups that lead to a nested
    # MultiTapeProductTrie with said offset
    # TODO: really need to explain what is going on before I forget
    combos_with_offset: defaultdict[
        int | None, MultiTapeStateTrie
    ] = dataclasses.field(
        default_factory=lambda: defaultdict(MultiTapeStateTrie)
    )
    next_groups: defaultdict[
        OffsetGroupedTerms, MultiTapeProductTrie
    ] = dataclasses.field(
        default_factory=lambda: defaultdict(MultiTapeProductTrie)
    )

    @classmethod
    def merge(cls, tries: list[MultiTapeProductTrie]) -> MultiTapeProductTrie:
        return cls._merge(tries[::])

    @classmethod
    def _merge(cls, tries: list[MultiTapeProductTrie]) -> MultiTapeProductTrie:
        assert len(tries) > 0
        if len(tries) == 1:
            return tries[0]

        last_trie = tries.pop()
        others_merged = cls._merge(tries)
        return others_merged | last_trie

    def __bool__(self):
        return bool(self.combos_with_offset) or bool(self.next_groups)

    def __or__(self, other: MultiTapeProductTrie) -> MultiTapeProductTrie:
        if self.offset is None:
            offset = other.offset
        elif other.offset is None:
            offset = self.offset
        elif self.offset == other.offset:
            offset = self.offset
        else:
            raise ValueError(
                f"offset mismatch: {self.offset} vs {other.offset}"
            )

        end_products = self.end_products | other.end_products
        has_nested_products = (
            self.has_nested_products | other.has_nested_products
        )
        combos_with_offset: defaultdict[
            int | None, MultiTapeStateTrie
        ] = defaultdict(MultiTapeStateTrie)
        next_groups: defaultdict[
            OffsetGroupedTerms, MultiTapeProductTrie
        ] = defaultdict(MultiTapeProductTrie)

        for next_offset in self.combos_with_offset:
            combos_with_offset[next_offset] = self.combos_with_offset[
                next_offset
            ]
        for next_offset in other.combos_with_offset:
            combos_with_offset[next_offset] = (
                combos_with_offset[next_offset] |
                other.combos_with_offset[next_offset]
            )

        for offset_group in self.next_groups:
            next_groups[offset_group] = self.next_groups[offset_group]
        for offset_group in other.next_groups:
            next_groups[offset_group] = (
                next_groups[offset_group] | other.next_groups[offset_group]
            )

        return MultiTapeProductTrie(
            offset=offset,
            end_products=end_products,
            has_nested_products=has_nested_products,
            combos_with_offset=combos_with_offset,
            next_groups=next_groups
        )

    def __and__(self, other: MultiTapeProductTrie) -> MultiTapeProductTrie:
        if self.offset == other.offset:
            offset = self.offset
        elif self.offset is None:
            offset = other.offset
        elif other.offset is None:
            offset = self.offset
        else:
            raise ValueError(
                f"offset mismatch: {self.offset} vs {other.offset}"
            )

        end_products = self.end_products & other.end_products
        has_nested_products: bool = False

        combos_with_offset: defaultdict[
            int | None, MultiTapeStateTrie
        ] = defaultdict(MultiTapeStateTrie)
        next_groups: defaultdict[
            OffsetGroupedTerms, MultiTapeProductTrie
        ] = defaultdict(MultiTapeProductTrie)

        for next_offset in self.combos_with_offset:
            if next_offset not in other.combos_with_offset:
                continue

            merged_offset_groups_trie = (
                self.combos_with_offset[next_offset] &
                other.combos_with_offset[next_offset]
            )
            if merged_offset_groups_trie.has_offset_group:
                combos_with_offset[next_offset] = merged_offset_groups_trie

        for group in self.next_groups:
            if group not in other.next_groups:
                continue

            next_trie = self.next_groups[group] & other.next_groups[group]
            if next_trie.has_end_product:
                has_nested_products = True
                next_groups[group] = next_trie

        return MultiTapeProductTrie(
            offset=offset,
            end_products=end_products,
            has_nested_products=has_nested_products,
            combos_with_offset=combos_with_offset,
            next_groups=next_groups
        )

    def merge_next_exclusion(
        self, offset_group: OffsetGroupedTerms,
        trie_exclusion: MultiTapeProductTrie
    ):
        offset = offset_group.offset
        assert offset == trie_exclusion.offset
        if offset is None:
            return

        for next_offset_group in self.next_groups:
            if next_offset_group.offset != trie_exclusion.offset:
                continue

            self.next_groups[next_offset_group] |= trie_exclusion

        if offset not in self.combos_with_offset:
            self.has_nested_products |= trie_exclusion.has_products
            self.combos_with_offset[offset].insert_group(offset_group)
            self.next_groups[offset_group] = trie_exclusion
            pass

    def copy(self) -> MultiTapeProductTrie:
        return MultiTapeProductTrie(
            offset=self.offset,
            end_products=self.end_products.copy(),
            has_nested_products=self.has_nested_products,
            combos_with_offset=self.combos_with_offset.copy(),
            next_groups=self.next_groups.copy()
        )

    @classmethod
    def spawn_root(cls):
        return cls(offset=None, has_nested_products=True)

    def get_next_offsets(self) -> set[int | None]:
        return set(self.combos_with_offset.keys())

    def get_offset_group(
        self, offset: int, states: list[MultiTapeState]
    ) -> OffsetGroupedTerms:
        if offset not in self.combos_with_offset:
            return OffsetGroupedTerms.blank()

        state_trie = self.combos_with_offset[offset]
        return state_trie.lookup(states)

    def advance_exclusions(
        self, source_offset_group: OffsetGroupedTerms,
        merge_from_adjacent_offsets: bool = True,
    ) -> MultiTapeProductTrie:
        assert source_offset_group.offset is not None
        matching_groups = self.match_offset_groups_for(
            offset=source_offset_group.offset, group=source_offset_group
        )
        matching_exclusions: list[MultiTapeProductTrie] = []
        for match_offset_group in matching_groups:
            matching_exclusions.append(self.next(
                group=match_offset_group
            ))

        """
        Consider the following situation:
        matching_offset_groups={
            OffsetGroupedTerms(terms=(D(0,0,0), D(0,1,0)), offset=0), 
            OffsetGroupedTerms(terms=(D(0,1,0), D(0,3,0)), offset=0), 
            OffsetGroupedTerms(terms=(D(0,3,0),), offset=0)
        }
        
        if we only went along 
        OffsetGroupedTerms(terms=(D(0,1,0), D(0,3,0)), then there 
        could still be a pre-existing product matching our 
        current_product along matching offset group at 
        OffsetGroupedTerms(terms=(D(0,3,0),), offset=0); 
        hence the need to merge all product exclusions 
        across matching offset groups
        """
        next_exclusions = MultiTapeProductTrie.merge(
            tries=matching_exclusions
        )

        if merge_from_adjacent_offsets:
            for offset_group in self.next_groups:
                if offset_group.offset == source_offset_group.offset:
                    continue

                other_exclusions = self.next_groups[offset_group]
                next_exclusions.merge_next_exclusion(
                    offset_group=offset_group, trie_exclusion=other_exclusions
                )
                pass

        return next_exclusions

    def match_offset_groups_for(
        self, offset: int, group: OffsetGroupedTerms
    ) -> set[OffsetGroupedTerms]:
        states = sorted([MultiTapeState.from_term(t) for t in group.terms])
        return self._match_offset_groups_for_states(
            offset=offset, states=states
        )

    def _match_offset_groups_for_states(
        self, offset: int, states: list[MultiTapeState]
    ) -> set[OffsetGroupedTerms]:
        if offset not in self.combos_with_offset:
            return {OffsetGroupedTerms.blank()}

        state_trie = self.combos_with_offset[offset]
        resolved_groups = state_trie.lookup_all(states)
        if not resolved_groups:
            return {OffsetGroupedTerms.blank()}

        return resolved_groups

    @property
    def has_products(self) -> bool:
        # whether this trie or any nested trie contains an end product
        return self.has_end_product or self.has_nested_products

    @staticmethod
    def next_zigzag_index(prev_index: int | None = None):
        """  
        Get the next index in a zigzag pattern
        0, -1, 1, -2, 2, -3, 3, ...
        :param prev_index:
        :return:
        """
        if prev_index is None:
            return 0

        if prev_index >= 0:
            # flip from positive to negative and increment (abs value)
            return -prev_index - 1
        else:
            # flip from negative to positive
            return -prev_index

    def next(
        self, group: OffsetGroupedTerms
    ) -> MultiTapeProductTrie:
        if group not in self.next_groups:
            return MultiTapeProductTrie()

        return self.next_groups[group]

    @staticmethod
    def _create_group_path(sorted_terms: list[D]) -> list[OffsetGroupedTerms]:
        """
        :param sorted_terms:
        terms that are assumed to have been sorted by position
        in zigzag order
        :return:
        terms grouped by offset with groups sorted by offset
        in zigzag order
        """
        if not sorted_terms:
            return []

        offset_groups: list[OffsetGroupedTerms] = []
        offset_grouped_terms: list[D] = []
        covered_offsets: set[int | None] = set()
        prev_offset: int | None = None

        def add_group(
            _offset_grouped_terms: list[D],
            _offset: int | None
        ):
            _group = OffsetGroupedTerms(
                terms=tuple(_offset_grouped_terms),
                offset=_offset
            )
            offset_groups.append(_group)

        for k, term in enumerate(sorted_terms):
            offset = term.get_position()
            flush_group = (
                (len(offset_grouped_terms) > 0) and
                (offset != prev_offset)
            )

            if flush_group:
                if offset_grouped_terms:
                    add_group(offset_grouped_terms, prev_offset)

                covered_offsets.add(prev_offset)
                offset_grouped_terms = []
            else:
                assert offset not in covered_offsets

            offset_grouped_terms.append(term)
            prev_offset = offset

        if offset_grouped_terms:
            add_group(offset_grouped_terms, prev_offset)

        return offset_groups

    @property
    def has_end_product(self) -> bool:
        return len(self.end_products) > 0

    @classmethod
    def create_group_path(cls, terms: list[D]) -> list[OffsetGroupedTerms]:
        sorted_terms = cls.build_term_path(terms)
        return cls._create_group_path(sorted_terms)

    def _insert_group_path(
        self, group_path: list[OffsetGroupedTerms],
        product: PyMultiTapeProduct
    ) -> int | None:
        """
        :param group_path:
        terms that are assumed to have been sorted by position
        :return:
        """
        if not group_path:
            self.end_products.add(product)
            return self.offset

        current_group, next_groups = group_path[0], group_path[1:]
        current_offset: int | None = current_group.offset

        if current_group not in self.next_groups:
            next_trie = MultiTapeProductTrie(offset=current_offset)
            self.next_groups[current_group] = next_trie
        else:
            next_trie = self.next_groups[current_group]

        self.combos_with_offset[current_offset].insert_group(current_group)
        next_trie._insert_group_path(next_groups, product=product)
        self.has_nested_products = True
        return self.offset

    def _has_group_path(self, group_path: list[OffsetGroupedTerms]) -> bool:
        if not group_path:
            return self.has_end_product

        current_group, next_groups = group_path[0], group_path[1:]
        if current_group not in self.next_groups:
            return False

        return self.next_groups[current_group]._has_group_path(next_groups)

    def load_matching_products(
        self, product: PyMultiTapeProduct
    ) -> set[PyMultiTapeProduct]:
        terms = product.get_flat_terms()
        return self.load_matching_products_for_terms(terms=terms)

    def load_matching_products_for_terms(
        self, terms: list[D]
    ) -> set[PyMultiTapeProduct]:
        group_path = self.create_group_path(terms)
        return self._load_matching_products(group_path=group_path)

    def _load_matching_products(
        self, group_path: list[OffsetGroupedTerms]
    ) -> set[PyMultiTapeProduct]:
        if not group_path:
            return self.end_products

        all_products: set[PyMultiTapeProduct] = set()
        current_group, next_groups = group_path[0], group_path[1:]
        if current_group.offset is None:
            raise ValueError("Blank groups are not allowed")

        matching_groups = self.match_offset_groups_for(
            offset=current_group.offset, group=current_group
        )
        for matching_group in matching_groups:
            next_trie = self.next_groups[matching_group]
            sub_products = next_trie._load_matching_products(
                group_path=next_groups
            )
            all_products |= sub_products

        return all_products

    @classmethod
    def build_term_path(cls, terms: list[D]) -> list[D]:
        unique_terms = list(set(terms))
        term_path = sorted(unique_terms, key=zigzag_term_sort_key)
        return term_path

    def _insert_term_path(
        self, terms: list[D], product: PyMultiTapeProduct
    ):
        group_path = self.create_group_path(terms)
        self._insert_group_path(group_path, product=product)

    def insert_product(self, product: PyMultiTapeProduct):
        terms = product.get_flat_terms()
        self._insert_term_path(terms, product=product)

    def has_term_path(self, terms: list[D]) -> bool:
        group_path = self.create_group_path(terms)
        return self._has_group_path(group_path)

    def has_product(self, product: PyMultiTapeProduct) -> bool:
        terms = product.get_flat_terms()
        return self.has_term_path(terms)


@dataclasses.dataclass
class MultiTapeStateRemap(object):
    _global_tape_state_remap: dict[MultiTapeState, TapeCellState]
    _rev_global_tape_state_remap: dict[TapeCellState, MultiTapeState]

    @classmethod
    def create_from(
        cls, global_tape_state_remap: dict[MultiTapeState, TapeCellState]
    ) -> MultiTapeStateRemap:
        rev_global_tape_state_remap: dict[TapeCellState, MultiTapeState] = {}
        _global_tape_state_remap = copy.deepcopy(global_tape_state_remap)

        for multi_tape_output in global_tape_state_remap:
            remapped_state = global_tape_state_remap[multi_tape_output]
            rev_global_tape_state_remap[remapped_state] = multi_tape_output

        return MultiTapeStateRemap(
            _global_tape_state_remap=_global_tape_state_remap,
            _rev_global_tape_state_remap=rev_global_tape_state_remap
        )

    def __len__(self) -> int:
        return len(self._global_tape_state_remap)

    def __getitem__(self, item: MultiTapeState) -> TapeCellState:
        return self.to_composed_state(item)

    def to_composed_state(
        self, multi_tape_state: MultiTapeState
    ) -> TapeCellState:
        assert isinstance(multi_tape_state, MultiTapeState), multi_tape_state
        return self._global_tape_state_remap[multi_tape_state]

    def from_composed_state(
        self, composed_state: TapeCellState
    ) -> MultiTapeState:
        return self._rev_global_tape_state_remap[composed_state]


@dataclasses.dataclass
class MultiTapeStatePathRemap(object):
    """
    Contains a remap of
    possible tape cell state combinations
    along the exact same position across all tapes
    to global tape cell state values
    """
    remap_counter_start: TapeCellState

    # TODO: technically only 1 state path can exist here
    void_state_paths: set[tuple[MultiTapeState, ...]] = dataclasses.field(
        default_factory=set
    )
    halt_state_paths: set[tuple[MultiTapeState, ...]] = dataclasses.field(
        default_factory=set
    )
    tape_state_path_remap: dict[
        tuple[MultiTapeState, ...], TapeCellState
    ] = dataclasses.field(
        default_factory=dict
    )
    rev_state_path_remap: dict[
        TapeCellState, tuple[MultiTapeState, ...]
    ] = dataclasses.field(
        default_factory=dict
    )

    def __len__(self):
        return self.num_normal_remaps

    def rev_lookup(
        self, tape_cell_state: TapeCellState
    ) -> Result[tuple[MultiTapeState, ...], TapeCellState]:
        if tape_cell_state == HALT_STATE:
            return Err(tape_cell_state)
        elif tape_cell_state == VOID_STATE:
            assert len(self.void_state_paths) == 1
            return Ok(list(self.void_state_paths)[0])

        return Ok(self.rev_state_path_remap[tape_cell_state])

    def get_all_state_paths(self) -> set[tuple[MultiTapeState, ...]]:
        return (
            set(self.tape_state_path_remap.keys()) |
            self.void_state_paths | self.halt_state_paths
        )

    @property
    def remap_counter_end(self) -> TapeCellState:
        return TapeCellState(
            self.remap_counter_start + self.num_normal_remaps
        )

    @property
    def next_free_counter(self) -> TapeCellState:
        return TapeCellState(
            self.remap_counter_end + TapeCellState(1)
        )

    def remap(self, state_path: tuple[MultiTapeState, ...]) -> TapeCellState:
        """
        Remaps a combination of multi tape cell states to
        a single tape cell state
        :param state_path:
        :return:
        """
        if self.is_halt_path(state_path):
            return HALT_STATE
        if self.is_void_path(state_path):
            return VOID_STATE

        return self.tape_state_path_remap[state_path]

    def remap_from_product_to_term(self, product: PyMultiTapeProduct) -> A:
        terms = product.get_flat_terms()
        positions = set([term.get_position() for term in terms])
        if len(positions) != 1:
            raise Exception(
                f"Cannot remap from product with multiple positions: "
                f"{product}"
            )

        position = positions.pop()
        multi_tape_states = [MultiTapeState.from_term(term) for term in terms]
        remapped_cell_state = self.remap(tuple(multi_tape_states))
        return A(position=position, state=remapped_cell_state)

    def remap_from_product_to_state(
        self, product: PyMultiTapeProduct
    ) -> TapeCellState:
        remapped_term = self.remap_from_product_to_term(product)
        return TapeCellState(remapped_term.get_state())

    def get_all_remap_states(self) -> set[TapeCellState]:
        """
        Get all remapped tape cell state values that
        have been allocated in this remap
        :return:
        """
        remap_states = set(self.tape_state_path_remap.values())

        if self.void_state_paths:
            remap_states.add(VOID_STATE)
        if self.halt_state_paths:
            remap_states.add(HALT_STATE)

        return remap_states

    def __getitem__(self, item: tuple[MultiTapeState, ...]) -> TapeCellState:
        return self.remap(item)

    @property
    def num_normal_remaps(self) -> int:
        return len(self.tape_state_path_remap)

    @property
    def num_void_remaps(self) -> int:
        return len(self.void_state_paths)

    @property
    def num_halt_remaps(self) -> int:
        return len(self.halt_state_paths)

    @staticmethod
    def is_void_path(path: tuple[MultiTapeState, ...]):
        """
        The remapped state is VOID if all the multi-tape cell states
        passed in are void also
        :param path:
        :return:
        """
        for multi_tape_state in path:
            if multi_tape_state.tape_cell_state != VOID_STATE:
                return False

        return True

    @staticmethod
    def is_halt_path(path: tuple[MultiTapeState, ...]):
        """
        The remapped state is HALT if any of the multi-tape cell states
        passed in are HALT also
        :param path:
        :return:
        """
        for multi_tape_state in path:
            if multi_tape_state.tape_cell_state == HALT_STATE:
                return True

        return False

    def insert_overlap_path(
        self, path: tuple[MultiTapeState, ...],
    ) -> Result[TapeCellState, None]:
        if path in self.tape_state_path_remap:
            return Err(None)

        if self.is_halt_path(path):
            self.halt_state_paths.add(path)
            return Ok(HALT_STATE)
        elif self.is_void_path(path):
            self.void_state_paths.add(path)
            return Ok(VOID_STATE)
        else:
            new_tape_state = self.get_next_tape_state()
            self.tape_state_path_remap[path] = new_tape_state
            self.rev_state_path_remap[new_tape_state] = path
            return Ok(new_tape_state)

    def get_next_tape_state(self) -> TapeCellState:
        """
        :return:
        The next available tape cell state that isn't being
        used in a mapping from an existing path
        """
        return TapeCellState(
            self.remap_counter_start + self.num_normal_remaps
        )

    def merge(self, other_remap: MultiTapeStatePathRemap):
        """
        We could probably just merge the dict / sets
        directly with a little more work but whatever
        :param other_remap:
        :return:
        """
        for path in other_remap.tape_state_path_remap:
            self.insert_overlap_path(path)

        for void_path in other_remap.void_state_paths:
            self.insert_overlap_path(void_path)

        for halt_path in other_remap.halt_state_paths:
            self.insert_overlap_path(halt_path)

    @classmethod
    def from_path(
        cls, path: tuple[MultiTapeState, ...],
        remap_counter_start: TapeCellState
    ) -> MultiTapeStatePathRemap:
        remap_states = cls(remap_counter_start=remap_counter_start)
        remap_states.insert_overlap_path(path)
        return remap_states

    def remap_single_tape_terms(
        self, terms: Sequence[A]
    ) -> Result[PyMultiTapeProduct, TapeCellState]:
        sub_products: list[PyMultiTapeProduct] = []
        covered_offsets: set[int] = set()

        for term in terms:
            assert term.get_position() not in covered_offsets
            sub_product_res = self.remap_term_to_multi_tape(input_term=term)
            if sub_product_res.is_err():
                return Err(sub_product_res.unwrap_err())

            sub_product = sub_product_res.unwrap()
            sub_products.append(sub_product)
            covered_offsets.add(term.get_position())

        combined_product = PyMultiTapeProduct.merge(sub_products)
        return Ok(combined_product)

    def remap_term_to_multi_tape(
        self, input_term: A
    ) -> Result[PyMultiTapeProduct, TapeCellState]:
        """
        Resolve a term in the composed automata to a
        multi-tape product with the corresponding tape states
        for each tape in the original multi-tape automata
        :param input_term:
        :return:
        """
        collected_global_terms: list[D] = []
        position = input_term.get_position()
        global_tape_state = TapeCellState(input_term.get_state())
        multi_tape_states_res = self.rev_lookup(
            tape_cell_state=global_tape_state
        )
        if multi_tape_states_res.is_err():
            halt_state = multi_tape_states_res.unwrap_err()
            return Err(halt_state)

        multi_tape_states = multi_tape_states_res.unwrap()
        for multi_tape_state in multi_tape_states:
            tape_no = multi_tape_state.tape_no
            tape_cell_state = multi_tape_state.tape_cell_state
            individual_term = D(
                position=position,
                tape_no=tape_no,
                state=tape_cell_state
            )
            collected_global_terms.append(individual_term)

        multi_tape_product = PyMultiTapeProduct(collected_global_terms)
        return Ok(multi_tape_product)


@dataclasses.dataclass
class TransitionOptimizations(object):
    disappeared_states: set[MultiTapeState]
    new_prod_to_state_map: ProductWritesMap
    whitelist_overlaps: FrozenTapeOverlaps


@dataclasses.dataclass
class ComposeTapesResult(object):
    transitions_group: TapeTransitionsGroup
    state_remap: MultiTapeStatePathRemap

    def get_transition_at(self, index: int) -> tuple[PyProduct, int]:
        return self.transitions_group[index]

    def count_unique_states(self):
        return len(self.state_remap.get_all_remap_states())

    def remap_from_product_to_term(self, product: PyMultiTapeProduct) -> A:
        return self.state_remap.remap_from_product_to_term(product)

    def remap_from_product_to_state(
        self, product: PyMultiTapeProduct
    ) -> TapeCellState:
        return self.state_remap.remap_from_product_to_state(product)

    def remap_prod_to_multi_tape(
        self, input_product: PyProduct
    ) -> Result[PyMultiTapeProduct, TapeCellState]:
        """
        Remaps the input product to a multi-tape product based
        on the state remap
        :param input_product:
        :return:
        """
        input_product_terms = input_product.to_flat_terms()
        collected_global_terms: list[D] = []

        for term in input_product_terms:
            sub_product_res = self.remap_term_to_multi_tape(input_term=term)
            if sub_product_res.is_err():
                return sub_product_res

            sub_product = sub_product_res.unwrap()
            sub_product_terms = sub_product.get_flat_terms()
            collected_global_terms.extend(sub_product_terms)

        multi_tape_product = PyMultiTapeProduct(collected_global_terms)
        return Ok(multi_tape_product)

    def remap_state_to_multi_tape(
        self, tape_cell_state: TapeCellState
    ) -> Result[tuple[MultiTapeState, ...], TapeCellState]:
        """
        Remaps a global tape cell state to the corresponding
        multi-tape cell states for each tape in the original
        multi-tape automata
        :param tape_cell_state:
        :return:
        """
        multi_tape_states_res = self.state_remap.rev_lookup(
            tape_cell_state=tape_cell_state
        )
        if multi_tape_states_res.is_err():
            halt_state = multi_tape_states_res.unwrap_err()
            return Err(halt_state)

        multi_tape_states = multi_tape_states_res.unwrap()
        return Ok(multi_tape_states)

    def remap_term_to_multi_tape(
        self, input_term: A
    ) -> Result[PyMultiTapeProduct, TapeCellState]:
        """
        Resolve a term in the composed automata to a
        multi-tape product with the corresponding tape states
        for each tape in the original multi-tape automata
        :param input_term:
        :return:
        """
        return self.state_remap.remap_term_to_multi_tape(
            input_term=input_term
        )


class MultiTapeBuilder(object):
    def __init__(self, multi_tape_automata: PyMultiTapeAutomata):
        self._automata = multi_tape_automata
        # tape state -> (relative) position -> overlapping tape state
        # (tape_no, state) -> int -> (tape_no, state)
        # and by overlaps I mean (tape_no, state)
        self._initial_overlaps: TapeOverlaps = TapeOverlaps()

        tape_nos = self.get_tape_nos()
        void_overlap_states = set([
            MultiTapeState(tape_no=tape_no, tape_cell_state=VOID_STATE)
            for tape_no in tape_nos
        ])
        # declare that void states can overlap with one another
        self.declare_initial_group_overlaps(void_overlap_states)

    @property
    def leftmost_extent(self) -> int:
        return self._automata.leftmost_extent

    @property
    def rightmost_extent(self) -> int:
        return self._automata.rightmost_extent

    def get_tape_nos(self) -> list[TapeNo]:
        return [
            TapeNo(tape_no) for tape_no in
            self._automata.get_tape_nos()
        ]

    def _get_prod_to_state_map(self) -> ProductWritesMap:
        return ProductWritesMap.from_pairs(
            self._automata.get_prod_to_state_map()
        )

    def declare_initial_group_overlaps(
        self, overlap_states: set[MultiTapeState]
    ):
        """
        Declare that every state in overlap_states
        can overlap with any other state at any relative offset
        in the initial automata tape

        To clarify,
        when I say that a tape state A can overlap with tape state B
        at offset k, I mean that:

        if A is present at some position p on the tape,
        then B can also plausibly be present at position p + k
        at some point in the history of the tape
        :param overlap_states:
        :return:
        """
        tape_nos = self.get_tape_nos()

        for offset in range(self.leftmost_extent, self.rightmost_extent + 1):
            for state in overlap_states:
                state_tape_no = state.tape_no

                for other_state in overlap_states:
                    """
                    every tape state could overlap with any other tape state
                    at any offset within the range of possible offsets
                    covered across all the automata's rules
                    """
                    self._initial_overlaps.insert_direct_overlap(
                        source_state=state, target_state=other_state,
                        offset=offset,
                        # min_offset=self.leftmost_extent,
                        # max_offset=self.rightmost_extent
                    )
                for tape_no in tape_nos:
                    if (tape_no == state_tape_no) and (offset == 0):
                        """
                        a tape state can't overlap directly
                        on the same position with void on the same tape
                        """
                        continue

                    # every tape state can overlap with void at any offset
                    tape_void = MultiTapeState(tape_no, VOID_STATE)
                    self._initial_overlaps.insert_direct_overlap(
                        source_state=state, target_state=tape_void,
                        offset=offset,
                        # min_offset=self.leftmost_extent,
                        # max_offset=self.rightmost_extent
                    )

    @classmethod
    def _build_whitelist_overlaps(
        cls, overlaps_fsm_state: TapeOverlapsFSMState,
        states_written: dict[MultiTapeState, set[PyMultiTapeProduct]],
        verbose: bool = False
    ) -> FrozenTapeOverlaps:
        """
        If for some newly spawned state_written
        that did not exist in the previous tape overlaps,

        and if for some offset e,
        every contributing product to the state_written
        has a translated variant that also writes to the
        same tape as state_written at said offset e,

        then we know that state_written can only overlap
        with the states produced by the translated variant
        products
        """
        def log(*args, **kwargs):
            if verbose:
                print(*args, **kwargs)

        prev_overlaps = overlaps_fsm_state.tape_overlaps
        prod_to_state_map = overlaps_fsm_state.product_writes_map
        # all states that could exist at the start of current time step
        all_prev_overlap_states = prev_overlaps.get_all_states()
        whitelist_overlaps = TapeOverlaps()

        for state_written in states_written:
            if state_written in all_prev_overlap_states:
                continue

            state_written_tape_no = state_written.tape_no
            log(f"NEW SPAWNED STATE {state_written}")
            # set of products that spawned state_written
            writing_products = states_written[state_written]
            """
            maps 
            write_offset
            -> 
            products whose outputs are offset by write_offset 
            relative from the products that wrote state_written
            """
            covering_products: defaultdict[
                int, set[PyMultiTapeProduct]
            ] = defaultdict(set)

            for contributing_product in writing_products:
                translated_prods = prod_to_state_map.get_translated_variants(
                    target_product=contributing_product
                )
                for translated_prod, terms_offset in translated_prods:
                    prod_writes = prod_to_state_map.get_state_writes_for(
                        translated_prod
                    )
                    tapes_written = [state.tape_no for state in prod_writes]
                    if state_written_tape_no not in tapes_written:
                        continue

                    """
                    if for a translated_product the input term positions 
                    that make it up are displaced by term_offset 
                    relative to the input terms of the contributing_product, 
                    then its output will be displaced by 
                    (write_offset := -terms_offset) from the output position 
                    of the contributing_product that spawned state_written
                    """
                    write_offset = -terms_offset
                    covering_products[write_offset].add(translated_prod)

            for write_offset, translated_prods in covering_products.items():
                if write_offset == 0:
                    continue
                if len(translated_prods) != len(writing_products):
                    # translated products don't cover
                    # all products contributing to the state_written
                    continue

                for translated_prod in translated_prods:
                    writes = prod_to_state_map[translated_prod]
                    written_tape_cell_state = writes[state_written_tape_no]
                    target_state = MultiTapeState(
                        tape_no=state_written_tape_no,
                        tape_cell_state=written_tape_cell_state
                    )
                    log(
                        "WHITE_INS",
                        (state_written, target_state, write_offset)
                    )
                    whitelist_overlaps.insert_direct_overlap(
                        source_state=state_written, target_state=target_state,
                        offset=write_offset,
                        # min_offset=None, max_offset=None
                    )

        return whitelist_overlaps.to_frozen()

    @classmethod
    def _create_optimizations(
        cls, start_overlaps_fsm_state: TapeOverlapsFSMState,
        states_written: dict[MultiTapeState, set[PyMultiTapeProduct]],
        verbose: bool = False
    ) -> TransitionOptimizations:
        """
        :param start_overlaps_fsm_state:
        overlaps FSM state at start of time step
        (i.e. previous overlaps FSM state)

        :param states_written:
        Mapping of states -> contributing products
        that were spawned in the current timestep* from said products

        Note that this does not make any claims
        on whether the states written here did not already exist
        in the automata / overlaps at the start of timestep

        :param verbose:
        :return:
        """
        def log(*args, **kwargs):
            if verbose:
                print(*args, **kwargs)

        prev_overlaps = start_overlaps_fsm_state.tape_overlaps
        prev_prod_to_state_map = start_overlaps_fsm_state.product_writes_map
        # all states that could exist at the start of current time step
        all_prev_overlap_states = prev_overlaps.get_all_states()
        # extinct_states: set[MultiTapeState] = set()
        # states that cease to exist in tapes after current time step
        disappeared_states: set[MultiTapeState] = set()
        prod_to_state_map = prev_prod_to_state_map.to_unfrozen()

        for prev_overlap_state in all_prev_overlap_states:
            current_state_attrs = prev_prod_to_state_map.get_state_attributes(
                prev_overlap_state, extant_states=all_prev_overlap_states,
                tape_overlaps=prev_overlaps
            )
            # whether state has no occurrences after the current time step
            no_state_occurrences_post_transition = (
                current_state_attrs.instant_delete and
                prev_overlap_state not in states_written
            )
            if no_state_occurrences_post_transition:
                log(f"DISAPPEARED STATE {prev_overlap_state}")
                disappeared_states.add(prev_overlap_state)

                """
                if not current_state_attrs.writable:
                    # a state is extinct if it will never show up again
                    # in any future time step
                    log("EXTINCT", prev_overlap_state)
                    extinct_states.add(prev_overlap_state)
                    prod_to_state_map.extinct_input_state(prev_overlap_state)
                """

        # TODO: apply overlap_states_at_offsets to tape_overlaps
        # TODO: refactor automata builder to its own repo?
        whitelist_overlaps = cls._build_whitelist_overlaps(
            overlaps_fsm_state=start_overlaps_fsm_state,
            states_written=states_written,
            verbose=verbose
        )

        # remove products that will never be satisfiable after
        # current time step
        state_attrs_map = prod_to_state_map.build_all_state_attrs_map(
            extant_states=None, tape_overlaps=prev_overlaps
        )
        prod_to_state_map.purge_unsatisfiable_products(
            state_attributes_map=state_attrs_map
        )
        return TransitionOptimizations(
            new_prod_to_state_map=prod_to_state_map,
            disappeared_states=disappeared_states,
            whitelist_overlaps=whitelist_overlaps,
        )

    @staticmethod
    def determine_state_written(
        overlaps_fsm_state: TapeOverlapsFSMState,
        verbose: bool = False
    ) -> defaultdict[
        MultiTapeState, set[PyMultiTapeProduct]
    ]:
        """
        Determine the states that would be spawned in the current
        timestep* given the products and tape overlaps
        in the overlaps_fsm_state of the previous timestep
        :param overlaps_fsm_state:
        :param verbose:
        :return:
        """
        def log(*args, **kwargs):
            if verbose:
                print(*args, **kwargs)

        prev_overlaps = overlaps_fsm_state.tape_overlaps
        relevant_input_products = overlaps_fsm_state.relevant_input_products
        prev_prod_to_state_map = overlaps_fsm_state.product_writes_map
        states_written: defaultdict[
            MultiTapeState, set[PyMultiTapeProduct]
        ] = defaultdict(set)

        for product in relevant_input_products:
            if not is_product_satisfiable(product, prev_overlaps):
                log('NO_SAT <<<', product, product.get_annotation())
                continue

            log('IS_SAT >>>', product, product.get_annotation())
            if product not in prev_prod_to_state_map:
                continue

            product_writes = prev_prod_to_state_map[product]
            # print('SATISFIABLE PRODUCT PRE:', product, product_writes)
            # input_terms = product.get_flat_terms()

            for write_tape_no in product_writes:
                output_tape_cell_state = product_writes[write_tape_no]
                output_state = MultiTapeState(
                    tape_no=write_tape_no,
                    tape_cell_state=output_tape_cell_state
                )
                states_written[output_state].add(product)

        return states_written

    def transition_overlaps(
        self, start_overlaps_fsm_state: TapeOverlapsFSMState,
        overlaps_fsm: TapeOverlapsFSM, verbose: bool = False
    ) -> TapeOverlapsFSMState:
        """
        :param start_overlaps_fsm_state:
        :param overlaps_fsm:
        :param verbose:
        :return:
        new tape overlaps, and set of input products that could
        be affected by the new overlaps
        """
        def log(*args, **kwargs):
            if verbose:
                print(*args, **kwargs)

        prev_overlaps = start_overlaps_fsm_state.tape_overlaps
        # relevant_input_products = overlaps_fsm_state.relevant_input_products
        prev_prod_to_state_map = start_overlaps_fsm_state.product_writes_map
        input_state_to_prod_map = (
            prev_prod_to_state_map.build_input_state_to_prod_map()
        )

        prev_overlaps.print_for_states()
        new_relevant_input_products: set[PyMultiTapeProduct] = set()
        """
        Collection of states that were spawned in the 
        current timestep* - note that this does not make any claims 
        on whether the states written here did not already exist 
        in the automata / overlaps at the start of timestep
        """
        overlaps = prev_overlaps.to_unfrozen()
        states_written = self.determine_state_written(
            overlaps_fsm_state=start_overlaps_fsm_state, verbose=verbose
        )
        optimizations = self._create_optimizations(
            start_overlaps_fsm_state=start_overlaps_fsm_state,
            states_written=states_written,
            verbose=verbose
        )

        for output_state in states_written:
            write_tape_no = output_state.tape_no
            output_tape_cell_state = output_state.tape_cell_state
            writing_products = states_written[output_state]

            for product in writing_products:
                input_terms = product.get_flat_terms()
                states_written[output_state].add(product)
                overlaps_updated = False

                for input_term in input_terms:
                    # Insert overlaps between the products' constituent
                    # input states and the output state it writes to
                    input_state = MultiTapeState.from_term(input_term)
                    term_offset_from_output = input_term.get_position()
                    term_offset_from_input = -term_offset_from_output

                    """
                    if not whitelist_overlaps.can_overlap_exist(
                        source_state=input_state, target_state=output_state,
                        offset=term_offset_from_input,
                        default_value=True
                    ):
                        log(
                            "WHITELIST_SKIP",
                            input_state, output_state, term_offset_from_input
                        )
                        raise RuntimeError
                        continue

                    # TODO: check if in whitelist_overlaps first
                    """

                    overlaps_updated |= overlaps.propagate_overlap(
                        source_state=input_state,
                        target_state=output_state,
                        offset=term_offset_from_input,
                        min_offset=self.leftmost_extent,
                        max_offset=self.rightmost_extent
                    )
                    assert start_overlaps_fsm_state in overlaps_fsm

                write_pair = (write_tape_no, output_tape_cell_state)
                if not overlaps_updated:
                    # print("SKIP_WRITE", write_pair)
                    continue

                log("DO_WRITE", write_pair)
                # Get the other products that use the current products'
                # output state as one of their input states, and add it
                # to list of products to check for satisfiability later
                affected_products = input_state_to_prod_map[output_state]
                for affected_product in affected_products:
                    new_relevant_input_products.add(affected_product)

        disappeared_states = optimizations.disappeared_states
        # TODO: refactor to optimizations.apply_to(fsm_state) -> new_fsm_state
        for disappeared_state in disappeared_states:
            overlaps.delete_state(disappeared_state)

        log(f'{states_written=}')
        log(f'{disappeared_states=}')

        whitelist_overlaps = optimizations.whitelist_overlaps
        overlaps.apply_whitelist(
            whitelist_overlaps=whitelist_overlaps, verbose=verbose
        )

        prod_to_state_map = optimizations.new_prod_to_state_map.to_frozen()
        return TapeOverlapsFSMState.create(
            tape_overlaps=overlaps.to_frozen(),
            relevant_input_products=FrozenSet(new_relevant_input_products),
            product_writes_map=prod_to_state_map
        )

    def build_overlaps(self, verbose: bool = True) -> TapeOverlaps:
        """
        Builds a mapping of which tape states can overlap with
        which other tape states at what relative offsets
        :return:
        """
        def log(*args, **kwargs):
            if verbose:
                print(*args, **kwargs)

        # map input products to output tape writes
        prod_to_state_map = self._get_prod_to_state_map()
        relevant_input_products = prod_to_state_map.build_input_products()

        # TODO: infer existing overlaps from the automata as well
        initial_fsm_state = TapeOverlapsFSMState.create(
            tape_overlaps=self._initial_overlaps.to_frozen(),
            relevant_input_products=relevant_input_products,
            product_writes_map=prod_to_state_map
        )
        overlaps_fsm = TapeOverlapsFSM(initial_fsm_state=initial_fsm_state)
        prev_fsm_state: TapeOverlapsFSMState = initial_fsm_state

        assert prev_fsm_state in overlaps_fsm
        # prod_to_state_map.build_state_to_products_map(verbose=True)
        overlaps_fsm_updated = True
        round_no: int = 0

        while overlaps_fsm_updated:
            log(f'NEXT_ROUND: {round_no}\n')
            round_no += 1

            next_fsm_state = self.transition_overlaps(
                start_overlaps_fsm_state=prev_fsm_state,
                overlaps_fsm=overlaps_fsm,
                verbose=verbose
            )
            # print(len(overlaps_fsm._existing_overlaps))
            _, overlaps_fsm_updated = overlaps_fsm.insert(
                state=prev_fsm_state, next_state=next_fsm_state
            )
            log(f'{overlaps_fsm_updated=}')
            prev_fsm_state = next_fsm_state
            assert prev_fsm_state in overlaps_fsm

        if verbose:
            log(f'overlaps FSM has {len(overlaps_fsm)} states')

        merged_overlaps = overlaps_fsm.merge()
        return merged_overlaps

    @classmethod
    def build_product_same_writes_map(
        cls, overlaps: TapeOverlaps,
        current_product_path: list[OffsetGroupedTerms],
        product_exclusions: MultiTapeProductTrie,
        _root_exclusions: MultiTapeProductTrie | None = None,
    ) -> ProductWritesMap:
        """
        Generate a mapping of all possible product combinations
        to an output state that is the same as the previous input state.

        :param _root_exclusions:
        If set, inserted products will be checked against this trie
        for duplicates. For debugging purposes only.
        :param product_exclusions:
        If a built product is in product_exclusions, we will
        exclude it from being added to the returned ProductWritesMap
        :param overlaps:
        Information about what tape states can overlap with what
        other tape states over all relevant position offsets
        :param current_product_path:
        The current partially built product.
        Each item contains the term for each tape for the
        same offset in the product path.
        :return:
        A product writes map where the products generated
        will transition every combination of term states along
        the write position offset to itself,
        (so no change from input to output)
        """
        product_writes_map = ProductWritesMap()
        if product_exclusions.has_end_product:
            """
            current product path is covered by a pre-existing product, 
            so we don't need to build it
            """
            return product_writes_map

        if not product_exclusions.has_nested_products:
            """
            current product path is not covered by a pre-existing product 
            in any subcase, so we can build and insert it 
            """
            flat_terms: list[D] = []
            for group in current_product_path:
                flat_terms.extend(group.terms)

            current_product = PyMultiTapeProduct(flat_terms)
            if _root_exclusions is not None:
                matching_products = _root_exclusions.load_matching_products(
                    product=current_product
                )
                assert not matching_products
                """
                if matching_products:
                    return ProductWritesMap()
                """

            product_writes_map.insert_neutral_product(current_product)
            return product_writes_map

        # TODO: implement overlaps FSM optimization
        states_by_tape_map = overlaps.group_states_by_tape()
        tape_nos = sorted(states_by_tape_map.keys())
        combos = list(utils.cartesian_product([
            sorted(list(states_by_tape_map[tape_no]))
            for tape_no in tape_nos
        ]))

        if current_product_path:
            next_offsets = product_exclusions.get_next_offsets()
        else:
            next_offsets = [0]

        for next_offset in next_offsets:
            if next_offset is None:
                continue

            for combo in combos:
                flat_terms = [state.to_term(next_offset) for state in combo]
                offset_group = OffsetGroupedTerms(
                    offset=next_offset, terms=tuple(flat_terms),
                )
                next_exclusions = product_exclusions.advance_exclusions(
                    source_offset_group=offset_group
                )
                current_product_path.append(offset_group)
                sub_products = cls.build_product_same_writes_map(
                    overlaps=overlaps,
                    current_product_path=current_product_path,
                    product_exclusions=next_exclusions,
                    _root_exclusions=_root_exclusions
                )
                product_writes_map.merge(sub_products)
                current_product_path.pop()

        return product_writes_map

    @classmethod
    def build_remap_states(
        cls, tape_nos: list[TapeNo],
        multi_tape_states_map: MultiTapeStatesMap,
        tape_overlaps: TapeOverlaps,
        overlap_state_path: Sequence[MultiTapeState] = (),
        tape_no_index: int = 0,
        remap_counter_start: TapeCellState = TapeCellState(2),
    ) -> MultiTapeStatePathRemap:
        """
        We want to remap all combinations of individual tape states
        that can overlap over each other directly along the same position
        at offset=0 across all tapes to global tape state numbers

        TODO: not sure if its the best to set a default counter start
            and have MultiTapeStatePathRemap merge shift conflicting remaps
        TODO: if we have all the variant states of a tape, skip the tape

        :param tape_no_index:
        index of the current tape we are building the remap
        for in the tape_nos list
        :param tape_nos:
        list of tapes to iterate over for tape state combination generation
        :param tape_overlaps:
        :param multi_tape_states_map:
        TapeNo -> set[TapeCellState]
        for each individual tape with tape no TapeNo,
        contains what tape cell states exist for that particular tape
        :param overlap_state_path:
        The currently built combination of tape states, or None
        None is used as a stand-in for every possible state for the
        particular tape at tape_no_index
        :param remap_counter_start:
        :return:
        MultiTapeStatePathRemap instance,
        which is a wrapper for combinations of individual tape states
        to global tape state numbers.
        """
        # counter state cannot collide with void (0) and halt (1) states
        assert remap_counter_start >= 2

        if tape_no_index >= len(tape_nos):
            # TODO: handle void / halt edge cases
            # print("INSERT_PATH", overlap_state_path)
            return MultiTapeStatePathRemap.from_path(
                path=tuple(overlap_state_path),
                remap_counter_start=remap_counter_start
            )

        collated_tape_state_remap = MultiTapeStatePathRemap(
            remap_counter_start=remap_counter_start
        )
        tape_no = tape_nos[tape_no_index]
        # states we are building combinations for in current tape
        next_tape_cell_states_set = multi_tape_states_map[tape_no]
        next_tape_cell_states = list(sorted(next_tape_cell_states_set))

        # what other states can overlap directly on top of
        # the last state in the overlap_state_path
        _overlap_state_path: list[MultiTapeState] = []
        if not isinstance(overlap_state_path, list):
            _overlap_state_path = list(overlap_state_path)
        else:
            _overlap_state_path = overlap_state_path

        if not _overlap_state_path:
            # Use all available states fur the current tape
            # as the overlap path is empty / just started
            next_state_overlaps = tape_overlaps.get_states_for_tape(tape_no)
        else:
            # get the overlapping states for prev_tape_state
            prev_tape_state = _overlap_state_path[-1]
            next_state_overlaps: FreezableSet[MultiTapeState] = (
                tape_overlaps.get_overlaps(prev_tape_state)[0]
            )

        for next_tape_cell_state in next_tape_cell_states:
            next_tape_state = MultiTapeState(
                tape_no=tape_no, tape_cell_state=next_tape_cell_state
            )
            if next_tape_state not in next_state_overlaps:
                # print("SKIP_STATE_1", _overlap_state_path, next_tape_state)
                continue

            # print("PUSH", _overlap_state_path, next_tape_state)
            _overlap_state_path.append(next_tape_state)
            sub_tape_state_path_remap = cls.build_remap_states(
                tape_no_index=tape_no_index + 1,
                tape_nos=tape_nos,
                overlap_state_path=_overlap_state_path,
                multi_tape_states_map=multi_tape_states_map,
                tape_overlaps=tape_overlaps,
                remap_counter_start=remap_counter_start,
            )
            collated_tape_state_remap.merge(sub_tape_state_path_remap)
            # print("POP", _overlap_state_path)
            _overlap_state_path.pop()

        return collated_tape_state_remap

    @classmethod
    def build_global_state_path_remap(
        cls, product_writes_map: ProductWritesMap,
        overlaps: TapeOverlaps
    ) -> MultiTapeStatePathRemap:
        """
        remap individual tape states to a global combined tape state
        :param overlaps:
        mapping for which tape states can overlap with which other
        tape states over all relevant relative offsets
        :param product_writes_map:
        mapping containing what output writes are emitted by the
        input products in product_writes_map
        :return:
        """
        # contains which tape cell states can exist in each tape
        multi_tape_states_map = MultiTapeStatesMap()

        for product in product_writes_map:
            product_writes = product_writes_map[product]
            # insert product output terms into multi_tape_states_map
            for tape_no in product_writes:
                tape_cell_state = product_writes[tape_no]
                multi_tape_states_map.insert(tape_no, tape_cell_state)

            product_terms = product.get_flat_terms()
            # insert product input terms into multi_tape_states_map
            for product_term in product_terms:
                term_state = MultiTapeState.from_term(product_term)
                tape_no = term_state.tape_no
                tape_cell_state = term_state.tape_cell_state
                multi_tape_states_map.insert(tape_no, tape_cell_state)

        tape_nos = multi_tape_states_map.get_tape_nos()
        global_tape_state_remap = cls.build_remap_states(
            tape_no_index=0, tape_nos=tape_nos,
            multi_tape_states_map=multi_tape_states_map,
            tape_overlaps=overlaps
        )
        return global_tape_state_remap

    @classmethod
    def get_terms_at_output_pos(
        cls, terms: Sequence[A]
    ) -> Sequence[A]:
        terms_at_output_pos: list[A] = []

        for term in terms:
            if term.get_position() == 0:
                terms_at_output_pos.append(term)

        return terms_at_output_pos

    @staticmethod
    def _reassign_state_path(
        input_state_path: tuple[MultiTapeState, ...],
        product_outputs: dict[TapeNo, TapeCellState],
    ) -> tuple[MultiTapeState, ...]:
        """
        Reassigns the tape cell states in input_state_path
        to the corresponding output tape cell states in product_outputs
        :param input_state_path:
        :param product_outputs:
        :return:
        """
        reassigned_state_path: list[MultiTapeState] = []

        for state in input_state_path:
            tape_no = state.tape_no

            if tape_no in product_outputs:
                new_tape_cell_state = product_outputs[tape_no]
                reassigned_state = MultiTapeState(
                    tape_no=tape_no, tape_cell_state=new_tape_cell_state
                )
                reassigned_state_path.append(reassigned_state)
            else:
                reassigned_state_path.append(state)

        return tuple(reassigned_state_path)

    @classmethod
    def get_matching_prods_for_single_tape_terms(
        cls, global_state_path_remap: MultiTapeStatePathRemap,
        preexisting_products: MultiTapeProductTrie,
        terms: Sequence[A]
    ) -> set[PyMultiTapeProduct]:
        multi_tape_product = global_state_path_remap.remap_single_tape_terms(
            terms=terms
        ).unwrap()
        matching_products = preexisting_products.load_matching_products(
            product=multi_tape_product
        )
        return matching_products

    def build_transitions_for_product(
        self, multi_tape_product: PyMultiTapeProduct,
        product_writes_map: ProductWritesMap,
        all_tape_states_per_tape: MultiTapeStatesMap,
        global_overlaps: TapeOverlaps,
        global_state_path_remap: MultiTapeStatePathRemap,
        preexisting_products: MultiTapeProductTrie
    ) -> TapeTransitionsGroup:
        """
        For every position that is covered by the current product,
        we want to know which states could be present in the product
        terms at that position across all individual tapes,
        and then determine all fully formed term combinations that
        could satisfy the multi_tape_product
        """
        transitions_group = TapeTransitionsGroup.spawn_new(None)
        all_tape_nos = sorted(self.get_tape_nos())
        product_terms = multi_tape_product.get_flat_terms()
        # tape writes that the multi_tape_product produces as output
        product_writes = product_writes_map[multi_tape_product]
        product_term_positions_set: set[int] = set()
        """
        map position_offset -> tape_no -> choice of possible tape states 
        that are required to be present at the aforementioned 
        (offset, tape_no) in order for the current product input 
        terms to be satisfied
        
        if for a given (position_offset, tape_no) there is no 
        entry in product_state_whitelists, then that means that any 
        tape state of tape tape_no can be assigned at 
        output position offset position_offset while still satisfying 
        the product's input terms
        """
        # TODO: ^ wow this is a mouthful
        product_state_whitelists: defaultdict[
            int, MultiTapeStatesMap
        ] = defaultdict(MultiTapeStatesMap)

        for product_term in product_terms:
            term_offset = product_term.get_position()
            product_term_positions_set.add(term_offset)
            term_state = MultiTapeState.from_term(product_term)
            # tape_no -> set of possible tape cell states at current pos
            product_state_whitelists[term_offset].insert(
                tape_no=term_state.tape_no, state=term_state
            )

        """
        maps offset (from output) to possible tape states 
        that can exist at said offset such that the product 
        inputs are satisfied.
        
        This (offset_tape_states_map) is different from 
        product_state_whitelists in that:
        
        product_state_whitelists only contains tape states that are 
        explicitly present in the product terms, whereas
        offset_tape_states_map contains all tape states that can
        exist at the given offset so long that the product's
        input terms still remain satisfied
        """
        offset_tape_states_map: defaultdict[
            int, MultiTapeStatesMap
        ] = defaultdict(MultiTapeStatesMap)

        for term_offset in product_state_whitelists:
            # what product term states can occur at each tape
            # for terms at the current term_offset (from product write)
            offset_states_whitelist: MultiTapeStatesMap = (
                product_state_whitelists[term_offset]
            )
            for tape_no in all_tape_states_per_tape:
                if tape_no in offset_states_whitelist:
                    continue

                """
                If there aren't any constraints on the tape cell states 
                that can exist on a particular tape_no imposed by the 
                product terms at the current term_offset, then the set 
                of states that can exist on that tape_no at the 
                current term_offset is just the set of all tape cell 
                states that can exist on that tape in general
                """
                assert tape_no not in offset_states_whitelist
                offset_states_whitelist[tape_no] = (
                    all_tape_states_per_tape[tape_no]
                )

            offset_tape_states_map[term_offset] = (
                offset_states_whitelist
            )

        # get input state combinations at offset 0
        # (relative to output position)
        input_zero_whitelist = product_state_whitelists[0]
        # remapped_global_state_set: set[TapeCellState] = set()
        """
        possible tape states that can exist for each tape 
        that exists, along the output write position for the 
        current product, right *after* output has been written 
        """
        post_output_whitelist = copy.deepcopy(input_zero_whitelist)

        for output_tape_no in product_writes:
            """
            When we spit out output tape_cell_states, we have to 
            consider the possible tape cell state values for tapes 
            that weren't explicitly written to, and remap all 
            possible combinations of unwritten tape states and 
            output tape states to a global tape state 
            """
            output_tape_cell_state = product_writes[output_tape_no]
            """
            Immediately after writing, the current tape state
            would only have the output tape state
            """
            post_output_whitelist[output_tape_no] = {
                output_tape_cell_state
            }

        remap_counter_start = TapeCellState(2)
        offset_combos_map: dict[int, MultiTapeStatePathRemap] = {}
        for term_offset in product_state_whitelists:
            offset_states_whitelist = offset_tape_states_map[term_offset]
            offset_input_combos = self.build_remap_states(
                tape_nos=all_tape_nos,
                multi_tape_states_map=offset_states_whitelist,
                tape_overlaps=global_overlaps,
                remap_counter_start=remap_counter_start
            )
            remap_counter_start = offset_input_combos.next_free_counter
            offset_combos_map[term_offset] = offset_input_combos

        product_term_positions = sorted(list(product_term_positions_set))
        assert 0 in product_term_positions
        """
        Each list item contains the set of possible remapped terms 
        that the corresponding term offset could contain
        """
        product_pos_combos: list[tuple[A, ...]] = []

        for product_term_position in product_term_positions:
            offset_input_combos = offset_combos_map[product_term_position]
            position_combos = offset_input_combos.get_all_state_paths()
            position_remapped_terms: list[A] = []

            for state_path in position_combos:
                remapped_cell_state = global_state_path_remap[state_path]
                remapped_term = A(
                    position=product_term_position,
                    state=remapped_cell_state
                )
                if remapped_term in position_remapped_terms:
                    continue

                position_remapped_terms.append(remapped_term)

            product_pos_combos.append(tuple(position_remapped_terms))

        """
        Get every specific combination of term states that could satisfy 
        the current product's input terms
        """
        specific_combos = utils.cartesian_product(product_pos_combos)

        for remapped_product_input_terms in specific_combos:
            current_product_writes = copy.deepcopy(product_writes)
            matching_products = self.get_matching_prods_for_single_tape_terms(
                global_state_path_remap=global_state_path_remap,
                preexisting_products=preexisting_products,
                terms=remapped_product_input_terms
            )
            for matching_product in matching_products:
                matching_product_writes = product_writes_map[matching_product]
                for product_write in matching_product_writes.items():
                    write_tape_no, write_tape_cell_state = product_write
                    existing_write_cell_state = current_product_writes.get(
                        write_tape_no, write_tape_cell_state
                    )
                    if existing_write_cell_state != write_tape_cell_state:
                        raise ValueError(
                            f'Conflicting writes for tape {write_tape_no=}: '
                            f'{existing_write_cell_state=} vs '
                            f'{write_tape_cell_state=} in product ' 
                            f'{multi_tape_product=} vs {matching_product=}'
                        )

                    current_product_writes[write_tape_no] = (
                        write_tape_cell_state
                    )

            input_terms_at_output_pos: Sequence[A] = (
                self.get_terms_at_output_pos(remapped_product_input_terms)
            )
            if len(input_terms_at_output_pos) != 1:
                raise ValueError(
                    f'There should only be one term at output position '
                    f'within {remapped_product_input_terms}'
                )

            input_term_at_output_pos = input_terms_at_output_pos[0]
            input_tape_cell_state_at_output_pos = TapeCellState(
                input_term_at_output_pos.get_state()
            )
            input_path_at_output_pos_res = global_state_path_remap.rev_lookup(
                tape_cell_state=input_tape_cell_state_at_output_pos
            )

            remapped_output_state: TapeCellState = HALT_STATE
            if input_path_at_output_pos_res.is_ok():
                input_state_path = input_path_at_output_pos_res.unwrap()
                output_state_path = self._reassign_state_path(
                    input_state_path=input_state_path,
                    product_outputs=current_product_writes
                )
                remapped_output_state = global_state_path_remap[
                    output_state_path
                ]

            annotation = multi_tape_product.get_annotation()
            if not annotation:
                annotation = str(multi_tape_product)

            transitions_group.add_transition(
                input_terms=tuple(remapped_product_input_terms),
                output_state=remapped_output_state,
                annotation=annotation,
                ban_halt_state=True
            )

        return transitions_group

    def compose_tapes(self) -> ComposeTapesResult:
        """
        Combine a multi-tape automata into a single tape automata
        TODO: reorder existing products for comparison with generated ones
        :return:
        """
        global_overlaps = self.build_overlaps()
        # TODO assert that void state can overlap with itself at any offset
        # get all tape states that can exist in each tape
        all_tape_states_per_tape: MultiTapeStatesMap = (
            global_overlaps.create_whitelist_for_offset()
        )
        preexisting_products = MultiTapeProductTrie.spawn_root()
        preexisting_writes_map = self._get_prod_to_state_map()
        for multi_tape_product in preexisting_writes_map:
            preexisting_products.insert_product(multi_tape_product)

        """
        Generate rules for all possible term combinations
        that could exist given the state overlaps passed in,
        excluding pre-existing products as they already have explicit 
        output write rule(s).
        
        The products generated here will transition every combination 
        of term states along the write position offset to itself, 
        (so no change from input to output) 
        """
        self_writes_map = self.build_product_same_writes_map(
            overlaps=global_overlaps, current_product_path=[],
            product_exclusions=MultiTapeProductTrie.spawn_root()
        )
        covering_product_writes_map = self.build_product_same_writes_map(
            overlaps=global_overlaps, current_product_path=[],
            product_exclusions=preexisting_products
        )
        product_writes_map = ProductWritesMap()
        product_writes_map.merge(preexisting_writes_map)
        product_writes_map.merge(covering_product_writes_map)

        # remap individual tape states to a global combined tape state
        global_state_path_remap = self.build_global_state_path_remap(
            product_writes_map=self_writes_map,
            overlaps=global_overlaps
        )
        # input-output pairs for the final combined automata
        global_transitions_group = TapeTransitionsGroup(
            num_states=None, transitions=[]
        )

        for multi_tape_product in product_writes_map:
            """
            print(
                f'TRANSITIONS_FOR: {multi_tape_product} '
                f'{multi_tape_product.get_annotation()}'
            )
            """
            product_transitions_group = self.build_transitions_for_product(
                multi_tape_product=multi_tape_product,
                product_writes_map=product_writes_map,
                all_tape_states_per_tape=all_tape_states_per_tape,
                global_overlaps=global_overlaps,
                global_state_path_remap=global_state_path_remap,
                preexisting_products=preexisting_products
            )
            global_transitions_group.merge(product_transitions_group)

        return ComposeTapesResult(
            transitions_group=global_transitions_group,
            state_remap=global_state_path_remap
        )
