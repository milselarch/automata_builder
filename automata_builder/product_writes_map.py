from __future__ import annotations

import copy
import typing
import dataclasses

from collections import defaultdict

from automata_builder._rust import PyMultiTapeProduct, D
from automata_builder.rule_generator import TapeCellState, TapeNo, VOID_STATE
from automata_builder.tape_overlaps import MultiTapeState, TapeOverlaps
from automata_builder.utils import (
    FreezableDefaultDict, FreezableDict, Freezable, FrozenDict, FreezableSet
)


@dataclasses.dataclass
class MultiTapeStateAttributes(object):
    """
    :param writable:
    Whether there are products that can produce the target state
    :param deletable:
    Whether there are products that can transition away from
    the target state
    :param instant_delete:
    whether all instances of target_state are immediately deleted
    this happens if there is a rule target_state -> other_state
    """
    writable: bool
    deletable: bool
    instant_delete: bool


class ProductWritesMap(Freezable):
    """
    map product -> tape_no -> output tape cell state
    TODO: rewrite all of this in rust
    """
    def __init__(self, prod_to_state_map: FreezableDefaultDict[
        PyMultiTapeProduct, FreezableDict[TapeNo, TapeCellState]
    ] | None = None):
        super().__init__()
        self._prod_to_state_map: FreezableDefaultDict[
            PyMultiTapeProduct, FreezableDict[TapeNo, TapeCellState]
        ] = FreezableDefaultDict(
            default_factory=FreezableDict
        )

        if prod_to_state_map is not None:
            self._prod_to_state_map = prod_to_state_map

    def _freeze(self) -> None:
        self._prod_to_state_map.freeze()

    def _encode(self) -> tuple:
        return self._prod_to_state_map.encode()

    @classmethod
    def _decode(cls, data: tuple) -> typing.Self:
        return cls(FreezableDefaultDict.decode(data))

    @classmethod
    def from_pairs(
        cls, pairs: list[tuple[PyMultiTapeProduct, dict[int, int]]]
    ):
        prod_to_state_map: FreezableDefaultDict[
            PyMultiTapeProduct, FreezableDict[TapeNo, TapeCellState]
        ] = FreezableDefaultDict(
            default_factory=FreezableDict
        )

        for product, writes_dict in pairs:
            for tape_no, tape_cell_state in writes_dict.items():
                prod_to_state_map[product][TapeNo(tape_no)] = (
                    TapeCellState(tape_cell_state)
                )

        return ProductWritesMap(prod_to_state_map=prod_to_state_map)

    def get_translated_variants(
        self, target_product: PyMultiTapeProduct,
        offset_whitelist: set[int] | None = None
    ) -> set[tuple[PyMultiTapeProduct, int]]:
        """
        :param target_product:
        :param offset_whitelist:
        :return:
        All products which have the same form as the input product
        but where all the input term offsets are shifted by some
        constant offset (i.e. translational variants)

        e.g.
        D(0,1,1)*D(1,2,1) and D(1,1,1)*D(2,2,1) are translational variants
        (second product is shifted by +1 offset)
        """
        if target_product not in self._prod_to_state_map:
            raise KeyError(
                f"Product {target_product} is not in "
                f"{self._prod_to_state_map}"
            )

        translational_variants: set[tuple[PyMultiTapeProduct, int]] = set()
        input_terms = sorted(target_product.get_flat_terms())
        all_products = self._prod_to_state_map.keys()

        for target_product in all_products:
            target_terms = sorted(target_product.get_flat_terms())
            if len(input_terms) != len(target_terms):
                continue

            offset_diffs = set()
            for input_term, target_term in zip(input_terms, target_terms):
                input_offset = input_term.get_position()
                target_offset = target_term.get_position()
                offset_diff = target_offset - input_offset
                offset_diffs.add(offset_diff)

            if len(offset_diffs) != 1:
                continue

            offset_diff = offset_diffs.pop()
            if offset_whitelist and offset_diff not in offset_whitelist:
                continue

            translational_variants.add(
                (target_product, offset_diff)
            )

        return translational_variants

    def extinct_input_state(self, state: MultiTapeState):
        """
        Remove all input products that contain the input state
        :param state:
        :return:
        """
        if self._frozen:
            raise ValueError("Cannot modify when frozen")

        input_products = list(self._prod_to_state_map.keys())

        for product in input_products:
            input_terms = product.get_flat_terms()

            for input_term in input_terms:
                input_multi_tape_state = MultiTapeState.from_term(input_term)
                if product not in self._prod_to_state_map:
                    continue

                if input_multi_tape_state == state:
                    del self._prod_to_state_map[product]

    def purge_unsatisfiable_products(
        self, state_attributes_map: FrozenDict[
            MultiTapeState, MultiTapeStateAttributes
        ]
    ) -> set[PyMultiTapeProduct]:
        """
        Remove all input products that are unsatisfiable
        :return:
        """
        if self._frozen:
            raise ValueError("Cannot modify when frozen")

        all_products = self._prod_to_state_map.keys()
        products_to_delete: set[PyMultiTapeProduct] = set()

        for product in all_products:
            becomes_unsatisfiable = self.does_product_becomes_unsatisfiable(
                product=product, state_attributes_map=state_attributes_map
            )
            if not becomes_unsatisfiable:
                continue

            products_to_delete.add(product)
            translated_products_res = self.get_translated_variants(
                target_product=product
            )
            for translated_product, _ in translated_products_res:
                products_to_delete.add(translated_product)

        for product in products_to_delete:
            print("DELETED_PRODUCT", product, product.get_annotation())
            del self._prod_to_state_map[product]

        return products_to_delete

    def to_unfrozen(self):
        return self.__class__(
            prod_to_state_map=self._prod_to_state_map.to_unfrozen()
        )

    def to_frozen(self) -> FrozenProductWritesMap:
        return FrozenProductWritesMap(
            prod_to_state_map=self._prod_to_state_map.to_frozen()
        )

    def __len__(self) -> int:
        return len(self._prod_to_state_map)

    def __iter__(self) -> typing.Iterator[PyMultiTapeProduct]:
        return iter(self._prod_to_state_map.keys())

    def __delitem__(self, product: PyMultiTapeProduct) -> None:
        if self._frozen:
            raise ValueError("Cannot remove product when frozen")

        del self._prod_to_state_map[product]

    def items(self):
        return self._prod_to_state_map.items()

    def does_product_becomes_unsatisfiable(
        self, product: PyMultiTapeProduct,
        state_attributes_map: FrozenDict[
            MultiTapeState, MultiTapeStateAttributes
        ]
    ) -> bool:
        if product not in self._prod_to_state_map:
            raise KeyError(
                f"Product {product} is not in {self._prod_to_state_map}"
            )

        if product.get_annotation() == 'EXP_REDUCE_START':
            print("TOMATO")

        has_transition_to_unsatisfiability = False
        has_input_terms_along_output_offset = False
        product_writes = self._prod_to_state_map[product]

        input_terms = product.get_flat_terms()
        for input_term in input_terms:
            input_multi_tape_state = MultiTapeState.from_term(input_term)
            input_term_attrs = state_attributes_map[input_multi_tape_state]
            input_term_offset = input_term.get_position()

            if input_term_offset != 0:
                if input_term_attrs.writable:
                    """
                    If any of the input product's terms can be spawned
                    from somewhere else, and said term does not lie along
                    the output position, then at some point its possible that
                    the state will be spawned and make the product
                    satisfiable
                    """
                    return False

                continue

            assert input_term_offset == 0
            has_input_terms_along_output_offset = True
            input_term_tape_no = TapeNo(input_term.get_tape_no())
            # input_term_tape_cell_state = input_term.get_cell_state()
            if input_term_tape_no not in product_writes:
                # output does not write to the same tape as input term
                # -> we don't care to check for unsatisfiability in that case
                continue

            output_tape_cell_state = product_writes[input_term_tape_no]
            output_multi_tape_state = MultiTapeState(
                tape_no=input_term_tape_no,
                tape_cell_state=output_tape_cell_state
            )

            if output_multi_tape_state not in state_attributes_map:
                # TODO: when is this possible
                continue

            output_term_attrs = state_attributes_map[output_multi_tape_state]
            input_is_unwritable = not input_term_attrs.writable
            output_term_is_undeletable = not output_term_attrs.deletable
            in_out_neq = input_multi_tape_state != output_multi_tape_state
            """
            if:
            1. all input terms outside write position are unwritable,
               (if input_term_offset != 0: if input_term_attrs.writable:)
            2. some input along offset 0 must be unwritable, 
            3. and that input must be replaced with an undeletable state, 
            4. and the replaced state is different from the input state,
            -> then we can be sure that the product will no longer be used
               moving forward
            
            Property #1 & #2 would mean that new instances of such products  
            will not form from elsewhere (i.e. other positions). 
            
            Property #3 & #4 would mean that all current combinations of 
            terms that satisfy the product transition away from 
            satisfying it, and remain so permanently  
            """
            has_transition_to_unsatisfiability |= (
                input_is_unwritable and
                output_term_is_undeletable and
                in_out_neq
            )

        if (1, 8) in product_writes.items():
            print("POTATO")

        if not has_input_terms_along_output_offset:
            """
            all input terms are offset away from output, and
            none of them can be spawned moving forward
            """
            return True

        return has_transition_to_unsatisfiability

    def build_input_products(self) -> FreezableSet[PyMultiTapeProduct]:
        relevant_input_products = FreezableSet()
        for product in self._prod_to_state_map:
            relevant_input_products.add(product)

        return relevant_input_products

    def get_state_writes_for(
        self, product: PyMultiTapeProduct
    ) -> list[MultiTapeState]:
        writes_map = self._prod_to_state_map[product]
        tape_state_writes: list[MultiTapeState] = []

        for tape_no in writes_map:
            tape_cell_state = writes_map[tape_no]
            tape_state = MultiTapeState(
                tape_no=tape_no, tape_cell_state=tape_cell_state
            )
            tape_state_writes.append(tape_state)

        return tape_state_writes

    def build_all_state_attrs_map(
        self, extant_states: set[MultiTapeState] | None,
        tape_overlaps: TapeOverlaps
    ) -> FrozenDict[
        MultiTapeState, MultiTapeStateAttributes
    ]:
        all_states = self.get_states_set()
        state_attributes_map: dict[
            MultiTapeState, MultiTapeStateAttributes
        ] = {}

        for state in all_states:
            state_attributes = self.get_state_attributes(
                state, extant_states=extant_states,
                tape_overlaps=tape_overlaps
            )
            state_attributes_map[state] = state_attributes

        return FrozenDict(state_attributes_map)

    def get_state_attributes(
        self, source_state: MultiTapeState,
        extant_states: set[MultiTapeState] | None,
        tape_overlaps: TapeOverlaps
    ) -> MultiTapeStateAttributes:
        """
        :param tape_overlaps:
        :param source_state:
        State to get attributes for
        :param extant_states:
        Tape states that we declare exist in the cellular automata
        at the time step we are getting attributes for

        If None, then the generated state attributes will be applicable
        throughout all future timesteps of the automata, otherwise
        it will only be applicable for the time step we are getting
        attributes for
        :return:
        """
        source_tape_cell_state = source_state.tape_cell_state
        transitioned_states = self.get_same_tape_states_transitioned_to(
            source_state=source_state, tape_overlaps=tape_overlaps
        )
        """
        :writable:
        Whether there are products that can produce the target_state
        :deletable:
        Whether there are products that can transition away from 
        the target_state
        """
        writable, deletable = False, False

        for product in self._prod_to_state_map:
            if extant_states is not None:
                """
                Check if the input states required for the product 
                all exist in extant_states, if not the product 
                is unsatisfiable
                """
                is_product_unsatisfiable = False
                for input_term in product.get_flat_terms():
                    multi_tape_state = MultiTapeState.from_term(input_term)
                    if multi_tape_state not in extant_states:
                        is_product_unsatisfiable = True
                        break

                if is_product_unsatisfiable:
                    continue

            writes_map = self._prod_to_state_map[product]
            source_state_written = False

            for tape_no in writes_map:
                tape_cell_state = writes_map[tape_no]
                write_state = MultiTapeState(
                    tape_no=tape_no, tape_cell_state=tape_cell_state
                )
                if write_state == source_state:
                    """
                    output writes to the same tape and same state as
                    target_state, so it is being "created" per-se
                    """
                    source_state_written = True

            # whether product writes to same tape as target_state
            # and the written TapeCellState is different from target_state
            writes_away_from_target_state = False
            # whether the product transitions cells
            # with input target_state to output target_state
            # (idempotency is relative to target_state only)
            is_idempotent_transition = False

            for input_term in product.get_flat_terms():
                if input_term.get_position() != 0:
                    continue
                if input_term.get_tape_no() != source_state.tape_no:
                    continue

                tape_cell_state = input_term.get_cell_state()
                if tape_cell_state != source_state.tape_cell_state:
                    continue

                output_tape_cell_state = writes_map.get(
                    source_state.tape_no, source_state.tape_cell_state
                )
                if source_state.tape_cell_state != output_tape_cell_state:
                    """
                    Our input product contains source_state
                    along that output position (offset 0) and the
                    output writes to the same tape, but to a different state,
                    so the original source_state is deleted
                    """
                    writes_away_from_target_state = True
                else:
                    is_idempotent_transition = True

            if len(product) == 1:
                input_term = product.get_flat_terms()[0]
                assert input_term.get_cell_state() != VOID_STATE, (
                    f"VOID STATE CANNOT AUTO TRANSITION AWAY - {product}"
                )

            writable |= source_state_written and not is_idempotent_transition
            deletable |= writes_away_from_target_state

        # TODO: include this without deleting necessary state overlaps
        instant_delete = (
            source_tape_cell_state not in transitioned_states
            and source_tape_cell_state != VOID_STATE
            and not writable
        )
        if instant_delete:
            print(f"INST_DELETE - {source_state} {writable=} {deletable=}")

        return MultiTapeStateAttributes(
            writable=writable, deletable=deletable,
            instant_delete=instant_delete
        )

    def build_state_to_products_map(
        self, verbose: bool = False
    ) -> defaultdict[MultiTapeState, set[PyMultiTapeProduct]]:
        """
        maps states -> products that produce them in their output terms
        :param verbose:
        :return:
        """
        state_to_products_map: defaultdict[
            MultiTapeState, set[PyMultiTapeProduct]
        ] = defaultdict(set)

        for product in self._prod_to_state_map:
            writes_map = self._prod_to_state_map[product]

            for tape_no in writes_map:
                tape_cell_state = writes_map[tape_no]
                tape_state = MultiTapeState(
                    tape_no=tape_no, tape_cell_state=tape_cell_state
                )
                state_to_products_map[tape_state].add(product)

        if verbose:
            states = sorted(state_to_products_map.keys())

            for state in states:
                print(f'Products that produce {state=}')

                production_products = state_to_products_map[state]
                for product in production_products:
                    print(f'- {product}')

        return state_to_products_map

    def build_input_state_to_prod_map(
        self, verbose: bool = False
    ) -> defaultdict[
        MultiTapeState, set[PyMultiTapeProduct]
    ]:
        """
        maps state -> products that contain it in their input terms
        :param verbose:
        :return:
        """
        # map state -> products that contain it in their input terms
        input_state_to_prod_map: defaultdict[
            MultiTapeState, set[PyMultiTapeProduct]
        ] = defaultdict(set)

        for product in self._prod_to_state_map:
            input_terms = product.get_flat_terms()

            for input_term in input_terms:
                input_state = MultiTapeState.from_term(input_term)
                input_state_to_prod_map[input_state].add(product)

        if verbose:
            for input_state in input_state_to_prod_map:
                print(f'Input products for {input_state}')

                products = input_state_to_prod_map[input_state]
                for product in products:
                    print(f'- {product}')

            print('')

        return input_state_to_prod_map

    def get_states_set(self) -> set[MultiTapeState]:
        states_set: set[MultiTapeState] = set()

        for tape_product in self._prod_to_state_map:
            product_terms = tape_product.get_flat_terms()

            for term in product_terms:
                tape_no, tape_cell_state = term.get_state()
                states_set.add(MultiTapeState(
                    tape_no=TapeNo(tape_no),
                    tape_cell_state=TapeCellState(tape_cell_state)
                ))

        return states_set

    def keys(self):
        return self._prod_to_state_map.keys()

    def values(self):
        return self._prod_to_state_map.values()

    def __getitem__(self, item: PyMultiTapeProduct):
        return copy.copy(self._prod_to_state_map[item])

    @classmethod
    def get_zero_terms_from_path(cls, product_path: list[D]) -> list[D]:
        zero_terms = []

        for term in product_path:
            if term.get_position() == 0:
                zero_terms.append(term)

        return zero_terms

    @classmethod
    def get_zero_terms_from_product(
        cls, product: PyMultiTapeProduct
    ) -> list[D]:
        return cls.get_zero_terms_from_path(
            product_path=product.get_flat_terms()
        )

    def insert_neutral_product(self, product: PyMultiTapeProduct):
        """
        Insert a product whose output is idempotent.
        I.e., products where their outputs rewrite the input terms
        at an offset = 0 to be at the same state as the input terms
        :param product:
        :return:
        """
        offset_zero_terms = self.get_zero_terms_from_product(product)

        for zero_term in offset_zero_terms:
            zero_state = MultiTapeState.from_term(zero_term)
            self.insert(product=product, tape_output=zero_state)

    def merge(self, other_writes_map: ProductWritesMap):
        for other_product in other_writes_map:
            other_product_writes = other_writes_map[other_product]

            for tape_no in other_product_writes:
                tape_cell_state = other_product_writes[tape_no]
                tape_output = MultiTapeState(tape_no, tape_cell_state)
                self.insert(product=other_product, tape_output=tape_output)

    def insert(
        self, product: PyMultiTapeProduct, tape_output: MultiTapeState
    ):
        write_tape_no = tape_output.tape_no
        write_tape_cell_state = tape_output.tape_cell_state
        self._insert(
            product=product, write_tape_no=write_tape_no,
            write_tape_cell_state=write_tape_cell_state
        )

    def _insert(
        self, product: PyMultiTapeProduct, write_tape_no: TapeNo,
        write_tape_cell_state: TapeCellState
    ):
        writes_map = self._prod_to_state_map[product]
        existing_tape_write_state = writes_map.get(
            write_tape_no, write_tape_cell_state
        )
        if existing_tape_write_state != write_tape_cell_state:
            raise ValueError(
                f"Conflicting output states for {product=} "
                f"on tape {write_tape_no}: "
                f"{existing_tape_write_state} vs "
                f"{write_tape_cell_state}"
            )

        writes_map[write_tape_no] = write_tape_cell_state

    @staticmethod
    def build_flat_offset_path_combos(
        source_state: MultiTapeState, tape_overlaps: TapeOverlaps
    ) -> list[tuple[D, ...]]:
        if source_state not in tape_overlaps:
            return []

        def build_combos(
            _offset: int, _tape_nos: list[TapeNo],
            _target_states_by_tape: defaultdict[TapeNo, set[TapeCellState]],
            tape_index: int = 0
        ) -> list[tuple[D, ...]]:
            if tape_index >= len(_tape_nos):
                return [()]

            tape_no = _tape_nos[tape_index]
            _combos: list[tuple[D, ...]] = []

            for target_tape_cell_state in _target_states_by_tape[tape_no]:
                target_term = D(
                    position=_offset, tape_no=tape_no,
                    state=target_tape_cell_state
                )
                sub_combos = build_combos(
                    tape_index=tape_index + 1,
                    _tape_nos=_tape_nos,
                    _target_states_by_tape=_target_states_by_tape,
                    _offset=_offset
                )
                for sub_combo in sub_combos:
                    _combos.append(sub_combo + (target_term,))

            return _combos

        offset_overlaps = tape_overlaps[source_state]
        combos: list[tuple[D, ...]] = []

        for offset in offset_overlaps:
            target_states = offset_overlaps[offset]
            target_states_list: list[MultiTapeState] = sorted(
                list(target_states), key=lambda t: t.tape_no
            )
            tape_nos = sorted(set([
                target_state.tape_no for target_state in target_states_list
            ]))
            target_states_by_tape: defaultdict[
                TapeNo, set[TapeCellState]
            ] = defaultdict(set)

            for target_state in target_states_list:
                target_states_by_tape[target_state.tape_no].add(
                    target_state.tape_cell_state
                )

            offset_combos = build_combos(
                _offset=offset, _tape_nos=tape_nos,
                _target_states_by_tape=target_states_by_tape,
            )
            combos.extend(offset_combos)

        return combos

    def get_same_tape_states_transitioned_to(
        self, source_state: MultiTapeState, tape_overlaps: TapeOverlaps
    ) -> set[TapeCellState]:
        """
        Get all the states that the source_state will transition to
        (i.e. what the state could be at the same position after a timestep)
        :param source_state:
        :param tape_overlaps:
        :return:
        """
        if source_state not in tape_overlaps:
            return {source_state.tape_cell_state}

        source_state_tape_no = source_state.tape_no
        source_tape_cell_state = source_state.tape_cell_state
        transitioned_states: set[TapeCellState] = set()
        source_term = D(
            position=0, tape_no=source_state.tape_no,
            state=source_state.tape_cell_state
        )

        for product, prod_writes_map in self._prod_to_state_map.items():
            input_terms = product.get_flat_terms()
            product_is_satisfiable = True

            for input_term in input_terms:
                input_multi_tape_state = MultiTapeState.from_term(input_term)
                if input_multi_tape_state not in tape_overlaps:
                    product_is_satisfiable = False
                    break

            if not product_is_satisfiable:
                continue
            if source_term not in input_terms:
                continue

            # TODO: check if there are identical products
            #  that write to this tape
            if source_state_tape_no not in prod_writes_map:
                continue

            same_tape_write_state = prod_writes_map[source_state_tape_no]
            if same_tape_write_state in transitioned_states:
                continue

            transitioned_states.add(same_tape_write_state)

        """
        Basically if none of the input products are applicable to 
        some combination of states that can exist in tape_overlaps, 
        then its possible the source_state will stay the same / 
        transition to the same state
        """
        has_path_transition_to_same_state: bool = False
        offset_path_combos = self.build_flat_offset_path_combos(
            source_state, tape_overlaps=tape_overlaps
        )
        for offset_path_combo in offset_path_combos:
            """
            Whether there is a product that satisfies
            the combination of states in offset_path_combo
            """
            has_satisfiable_product_transitioning_to_same_state = False

            for product, prod_writes_map in self._prod_to_state_map.items():
                """
                We say that the product "covers" the combo if 
                the combination of states satisfies the product's input terms
                """
                product_covers_combo = True

                input_terms = product.get_flat_terms()
                input_tapes = set([c.get_tape_no() for c in input_terms])
                if source_term not in input_terms:
                    continue

                written_state_for_source_tape = prod_writes_map.get(
                    source_state_tape_no,  source_tape_cell_state
                )
                transitions_to_same_source_state = (
                    written_state_for_source_tape == source_tape_cell_state
                )

                for term in offset_path_combo:
                    if term.get_tape_no() not in input_tapes:
                        """
                        If the product doesn't have any input terms 
                        with the same tape_no as the current path combo term, 
                        then the product is satisfiable regardless of the term
                        """
                        continue
                    if term not in input_terms:
                        product_covers_combo = False
                        break

                if product_covers_combo and transitions_to_same_source_state:
                    has_satisfiable_product_transitioning_to_same_state = True
                    break

            if has_satisfiable_product_transitioning_to_same_state:
                has_path_transition_to_same_state = True
                break

        if has_path_transition_to_same_state:
            transitioned_states.add(source_tape_cell_state)
        if not transitioned_states:
            transitioned_states.add(source_tape_cell_state)

        if source_tape_cell_state not in transitioned_states:
            print("NO PATH TO SAME STATE", source_state, transitioned_states)
            pass

        return transitioned_states


class FrozenProductWritesMap(ProductWritesMap):
    def __init__(self, prod_to_state_map: FreezableDefaultDict[
        PyMultiTapeProduct, FreezableDict[TapeNo, TapeCellState]
    ] | None = None):
        if prod_to_state_map and not prod_to_state_map.is_frozen:
            raise ValueError(
                "FrozenProductWritesMap only works with "
                "frozen prod_to_state_map"
            )

        super().__init__(prod_to_state_map=prod_to_state_map)
        self.freeze()

    def to_unfrozen(self) -> ProductWritesMap:
        return ProductWritesMap(
            prod_to_state_map=self._prod_to_state_map.to_unfrozen()
        )
