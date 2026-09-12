from __future__ import annotations

import dataclasses
import typing
import numpy as np

from typing import Final, Self
from automata_builder._rust import A, PyExpression, PyProduct


class TapeNo(int):
    pass


class TapeCellState(int):
    pass


BLANK_INT: Final[int] = -1
VOID_STATE: Final[TapeCellState] = TapeCellState(0b0)
HALT_STATE: Final[TapeCellState] = TapeCellState(0b1)


def is_halt_state(term: A) -> bool:
    return term.get_state() == HALT_STATE


@dataclasses.dataclass
class AutomataTransitionsGroup(object):
    """
    contains a set of transitions for a cellular automaton
    defined as a mapping from input states to output state
    map A[] -> output state
    """
    num_states: int | None = None
    transitions_map: dict[tuple[A, ...], int] = dataclasses.field(
        default_factory=dict
    )
    transitions: list[
        tuple[
            tuple[A, ...],
            int
        ]
    ] = dataclasses.field(
        default_factory=list
    )

    def __len__(self):
        return len(self.transitions)

    def __getitem__(self, index: int) -> tuple[PyProduct, int]:
        input_terms, output_state = self.transitions[index]
        input_product = PyProduct(input_terms)
        return input_product, output_state

    def get_all_states(self):
        all_states = {0}

        for transition in self.transitions:
            input_terms, output_state = transition
            for term in input_terms:
                all_states.add(term.get_state())

            all_states.add(output_state)

        return all_states

    @classmethod
    def spawn_new(cls, num_states: int | None) -> AutomataTransitionsGroup:
        return cls(num_states=num_states, transitions=[])

    def add_transition(
        self, input_terms: tuple[A, ...], output_state: int,
        ban_halt_state: bool = False
    ) -> bool:
        transition_entry = (input_terms, output_state)
        if input_terms in self.transitions_map:
            if self.transitions_map[input_terms] == output_state:
                return False

            raise ValueError(
                f'Conflicting transition for input terms {input_terms}: '
                f'{output_state} vs {self.transitions_map[input_terms]}'
            )

        _num_states: int | float = float('inf')
        if self.num_states is not None:
            _num_states = self.num_states

        assert 0 <= output_state < _num_states

        for term in input_terms:
            assert isinstance(term, A)
            state = term.get_state()
            assert 0 <= state < _num_states

            if ban_halt_state and is_halt_state(term):
                raise ValueError(
                    f'Cannot add transition with halt state term: {term}'
                )

        self.transitions.append(transition_entry)
        self.transitions_map[input_terms] = output_state
        return True

    def merge(
        self, other: AutomataTransitionsGroup
    ) -> Self:
        for input_terms, output_state in other.transitions:
            self.add_transition(
                input_terms=input_terms, output_state=output_state
            )

        return self


@dataclasses.dataclass
class AutomataRuleSet(object):
    expansion_map: dict[int, PyExpression]
    flat_term_offsets: tuple[int, ...]
    base_num_products: int
    base_terms_per_product: int
    num_flat_terms: int
    num_states: int

    def get_expression_for_state(self, state: int) -> PyExpression:
        assert 0 <= state < self.num_states
        return self.expansion_map[state]

    def get_num_products(self, timesteps: int) -> int:
        return self.base_num_products ** (
            (-1 + self.base_terms_per_product ** timesteps) //
            (-1 + self.base_terms_per_product)
        )

    def get_num_terms(self, timesteps: int) -> int:
        return self.base_terms_per_product ** timesteps

    def get_position_matrices(self, expansion_index: int) -> np.ndarray:
        assert 0 <= expansion_index < self.num_flat_terms
        position_matrix = np.zeros(
            (self.num_states, self.num_states), dtype=np.uint
        )

        states = sorted(list(self.expansion_map.keys()))
        for next_state in states:
            expr = self.expansion_map[next_state]
            flat_terms: list[A] = expr.get_flat_terms()
            assert len(flat_terms) == self.num_flat_terms
            term = flat_terms[expansion_index]
            start_state = term.get_state()
            start_position = term.get_position()
            position_matrix[start_state, next_state] = start_position

        return position_matrix

    def get_position_offset(self, expansion_index: int) -> int:
        return self.flat_term_offsets[expansion_index]

    def resolve_state_hist(
        self, initial_state: int, expansion_path: list[int]
    ) -> list[int]:
        state = initial_state
        state_hist: list[int] = [state]

        for expand_index in expansion_path:
            expand_equation = self.expansion_map[state]
            flat_expand_terms = expand_equation.get_flat_terms()
            target_term: A = flat_expand_terms[expand_index]
            state = target_term.get_state()
            state_hist.append(state)

        return state_hist

    def resolve_position_hist(
        self, initial_position: int, expansion_path: list[int]
    ) -> list[int]:
        position = initial_position
        position_hist: list[int] = [position]

        for expand_index in expansion_path:
            step_offset = self.get_position_offset(expand_index)
            position += step_offset
            position_hist.append(position)

        return position_hist


class RuleGenerator(object):
    @staticmethod
    def tuple_to_product(terms: tuple[A, ...]) -> PyProduct:
        product = terms[0].to_py_product()
        if len(terms) == 1:
            return product

        for term in terms[1:]:
            product = product.multiply_by_term(term)

        assert isinstance(product, PyProduct)
        return product

    @staticmethod
    def aggregate_bit_or(expr_list: list[
        typing.Union[PyExpression, PyProduct]
    ]) -> PyExpression:
        if not expr_list:
            return PyExpression()

        result = expr_list[0]
        for k in range(1, len(expr_list)):
            result = result | expr_list[k]

        return result.to_py_expression()

    @classmethod
    def to_ruleset(
        cls, transitions_group: AutomataTransitionsGroup,
        require_consistent_flat_term_offsets: bool = True,
        verbose: bool = False
    ) -> AutomataRuleSet:
        equations = cls.generate_equations(
            transitions_group, pad_product_length=True,
            pad_expr_length=True, verbose=verbose
        )
        max_flat_terms = 0
        base_num_products = 0
        base_terms_per_product = 0
        base_flat_term_offsets = []

        for state in sorted(equations.keys()):
            assert isinstance(equations[state], PyExpression)
            flat_terms = equations[state].get_flat_terms()
            flat_term_offsets = [term.get_position() for term in flat_terms]

            if not base_flat_term_offsets:
                base_flat_term_offsets = flat_term_offsets
            elif require_consistent_flat_term_offsets:
                assert flat_term_offsets == base_flat_term_offsets, (
                    f'Inconsistent flat term offsets for state {state}: '
                    f'{flat_term_offsets} != {base_flat_term_offsets}'
                )

            assert isinstance(flat_terms, list)
            num_flat_terms = len(flat_terms)
            if max_flat_terms != 0:
                assert num_flat_terms == max_flat_terms

            base_num_products = len(equations[state])
            max_flat_terms = max(max_flat_terms, num_flat_terms)

            for product in equations[state]:
                assert isinstance(product, PyProduct)
                base_terms_per_product = product.get_num_terms()

                for term in product:
                    assert isinstance(term, A)

        assert max_flat_terms > 0
        return AutomataRuleSet(
            num_states=transitions_group.num_states,
            flat_term_offsets=tuple(base_flat_term_offsets),
            expansion_map=equations,
            num_flat_terms=max_flat_terms,
            base_num_products=base_num_products,
            base_terms_per_product=base_terms_per_product
        )

    @classmethod
    def generate_equations(
        cls, transitions_group: AutomataTransitionsGroup,
        pad_product_length: bool = True,
        pad_expr_length: bool = True,
        verbose: bool = False
    ) -> dict[int, PyExpression]:
        """
        generates a mapping from state to expression
        :param transitions_group:
        :param pad_product_length:
        whether to pad the products to the same length
        :param pad_expr_length:
        whether to pad the expressions to the same length
        :param verbose:
        :return:
        """
        def log(*args, **kwargs):
            if verbose:
                print(*args, **kwargs)

        state_eq_terms_map: dict[int, list[PyProduct]] = {}
        for transition in transitions_group.transitions:
            input_states, output_state = transition
            if output_state not in state_eq_terms_map:
                state_eq_terms_map[output_state] = []

            product = cls.tuple_to_product(input_states)
            state_eq_terms_map[output_state].append(product)

        if pad_product_length:
            # ensure that all products have the same length
            max_product_length = 0
            for next_state in state_eq_terms_map:
                for product in state_eq_terms_map[next_state]:
                    log(f'Product for state {next_state}: {product}')
                    log(type(product))

                max_product_length = max([
                    len(product) for product in state_eq_terms_map[next_state]
                ])

            log(f'Padding products to length {max_product_length}')
            assert max_product_length > 0

            for next_state in state_eq_terms_map:
                state_products = state_eq_terms_map[next_state]

                for prod_idx in range(len(state_products)):
                    product = state_products[prod_idx]
                    start_product_length = len(product)
                    pad_length = max_product_length - start_product_length
                    end_term = product[start_product_length - 1]
                    new_product = product

                    for _ in range(pad_length):
                        new_product = new_product.multiply_by_term(end_term)

                    assert len(new_product) == max_product_length
                    state_products[prod_idx] = new_product

        state_eq_map: dict[int, PyExpression] = {
            next_state: cls.aggregate_bit_or(state_eq_terms_map[next_state])
            for next_state in state_eq_terms_map
        }

        if pad_expr_length:
            # ensure that all expressions have the same length
            # i.e. same number of products
            max_expr_length = max([
                len(state_eq_map[next_state]) for next_state in state_eq_map
            ])
            for next_state in state_eq_map:
                start_expr_length = len(state_eq_map[next_state])
                pad_length = max_expr_length - start_expr_length
                end_product = state_eq_map[next_state][start_expr_length - 1]

                for _ in range(pad_length):
                    state_eq_map[next_state] |= end_product

        sorted_states = sorted(list(state_eq_map.keys()))
        log(f'{sorted_states=}')

        for next_state in sorted_states:
            log(f'{next_state} -> {state_eq_map[next_state]}')

        if transitions_group.num_states is not None:
            for state in range(transitions_group.num_states):
                err = f'State {state} missing in ruleset'
                assert state in state_eq_terms_map, err

        return state_eq_map


if __name__ == "__main__":
    transitions = AutomataTransitionsGroup(
        transitions=[
            ((A(0, 1),), 1),
            ((A(1, 1), A(1, 0)), 0)
        ],
        num_states=2
    )
    ruleset = RuleGenerator.to_ruleset(
        transitions, verbose=True
    )
    print('RULESET', ruleset)
