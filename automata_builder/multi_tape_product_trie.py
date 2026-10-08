from __future__ import annotations

import dataclasses

from collections import defaultdict
from automata_builder._rust import PyMultiTapeProduct, D


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
    _product: PyMultiTapeProduct | None = None

    def __hash__(self):
        return hash((self.terms, self.offset))

    def __eq__(self, other: object):
        if not isinstance(other, OffsetGroupedTerms):
            return False

        return (self.terms, self.offset) == (other.terms, other.offset)

    def __bool__(self):
        return bool(self.terms)

    def to_product(self) -> PyMultiTapeProduct:
        if self._product is not None:
            return self._product

        return PyMultiTapeProduct(terms=self.terms)

    def satisfiable_with(self, other: OffsetGroupedTerms) -> bool:
        if self.offset != other.offset:
            return False

        return self.to_product().is_satisfiable_with(
            other.to_product()
        )

    @property
    def is_blank(self):
        return not self.terms

    @classmethod
    def blank(cls):
        return cls(terms=())

    def __post_init__(self):
        classname = self.__class__.__name__
        if self.offset is None and self.terms:
            raise ValueError(
                f"{classname} without offset cannot have terms"
            )

        assert list(self.terms) == sorted(self.terms)

        for term in self.terms:
            assert term.get_position() == self.offset

        if self.terms:
            self._product = self.to_product()


def offset_group_path_to_product(
        offset_group_path: list[OffsetGroupedTerms]
) -> PyMultiTapeProduct:
    flat_terms: list[D] = []
    for group in offset_group_path:
        flat_terms.extend(group.terms)

    product = PyMultiTapeProduct(flat_terms)
    return product


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
    # TODO: this could just be a omputable property i think
    has_nested_products: bool = False
    next_groups: defaultdict[
        OffsetGroupedTerms, MultiTapeProductTrie
    ] = dataclasses.field(
        default_factory=lambda: defaultdict(MultiTapeProductTrie)
    )

    def remove_end_product(self, product: PyMultiTapeProduct) -> bool:
        if product in self.end_products:
            self.end_products.remove(product)
            return True
        else:
            return False

    @classmethod
    def merge(cls, tries: list[MultiTapeProductTrie]) -> MultiTapeProductTrie:
        return cls._merge(tries[::])

    @classmethod
    def _merge(cls, tries: list[MultiTapeProductTrie]) -> MultiTapeProductTrie:
        assert len(tries) > 0
        if len(tries) == 1:
            return tries[0].copy()

        last_trie = tries.pop()
        others_merged = cls._merge(tries)
        return others_merged | last_trie

    def __bool__(self):
        return bool(self.next_groups)

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
        next_groups: defaultdict[
            OffsetGroupedTerms, MultiTapeProductTrie
        ] = defaultdict(MultiTapeProductTrie)

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

        next_groups: defaultdict[
            OffsetGroupedTerms, MultiTapeProductTrie
        ] = defaultdict(MultiTapeProductTrie)

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
            next_groups=next_groups
        )

    def insert(
            self, offset_group: OffsetGroupedTerms,
            trie_exclusion: MultiTapeProductTrie
    ):
        """
        Insert an offset group -> trie exclusion into own trie exclusions.
        :param offset_group:
        :param trie_exclusion:
        :return:
        """
        offset = offset_group.offset
        assert offset == trie_exclusion.offset
        if offset is None:
            return

        self.has_nested_products |= trie_exclusion.has_products

        for next_offset_group in self.next_groups:
            if next_offset_group.offset == offset:
                continue

            self.next_groups[next_offset_group] |= trie_exclusion

        if offset_group not in self.next_groups:
            self.next_groups[offset_group] = trie_exclusion
        else:
            self.next_groups[offset_group] |= trie_exclusion

    def copy(self) -> MultiTapeProductTrie:
        return MultiTapeProductTrie(
            offset=self.offset,
            end_products=self.end_products.copy(),
            has_nested_products=self.has_nested_products,
            next_groups=self.next_groups.copy()
        )

    @classmethod
    def spawn_root(cls):
        return cls(offset=None, has_nested_products=True)

    def get_next_offsets(self) -> set[int | None]:
        offsets = set()

        for offset_group in self.next_groups:
            offsets.add(offset_group.offset)

        return offsets

    def without_nested_offsets(self, offsets: set[int]):
        if not offsets:
            return self

        has_nested_products = False
        next_groups: defaultdict[
            OffsetGroupedTerms, MultiTapeProductTrie
        ] = defaultdict(MultiTapeProductTrie)

        for offset_group in self.next_groups:
            if offset_group.offset in offsets:
                continue

            next_groups[offset_group] = self.next_groups[
                offset_group
            ].without_nested_offsets(offsets)
            has_nested_products |= self.next_groups[offset_group].has_products

        return MultiTapeProductTrie(
            offset=self.offset,
            end_products=self.end_products.copy(),
            has_nested_products=has_nested_products,
            next_groups=next_groups
        )

    def advance_exclusions(
            self, source_offset_group: OffsetGroupedTerms,
            merge_from_adjacent_offsets: bool = True,
    ) -> MultiTapeProductTrie:
        assert source_offset_group.offset is not None
        matching_groups = self.match_offset_groups_for(
            target_group=source_offset_group
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

                adjacent_exclusions = self.next_groups[offset_group]
                next_exclusions.insert(
                    offset_group=offset_group,
                    trie_exclusion=adjacent_exclusions
                )

        next_exclusions = next_exclusions.without_nested_offsets(
            offsets={source_offset_group.offset}
        )

        return next_exclusions

    def match_offset_groups_for(
            self, target_group: OffsetGroupedTerms,
            insert_blank: bool = True,
    ) -> set[OffsetGroupedTerms]:
        matching_groups: set[OffsetGroupedTerms] = set()

        for offset_group in self.next_groups:
            if offset_group.satisfiable_with(target_group):
                matching_groups.add(offset_group)

        if insert_blank and not matching_groups:
            return {OffsetGroupedTerms.blank()}

        return matching_groups

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
    def group_path_from_product(
            cls, product: PyMultiTapeProduct
    ) -> list[OffsetGroupedTerms]:
        return cls.create_group_path(product.get_flat_terms())

    @classmethod
    def create_group_path(cls, terms: list[D]) -> list[OffsetGroupedTerms]:
        sorted_terms = cls.build_term_path(terms)
        return cls._create_group_path(sorted_terms)

    def _insert_group_path(
            self, group_path: list[OffsetGroupedTerms],
            product: PyMultiTapeProduct
    ) -> MultiTapeProductTrie:
        """
        :param group_path:
        terms that are assumed to have been sorted by position
        :return:
        """
        if not group_path:
            self.end_products.add(product)
            return self

        current_group, next_groups = group_path[0], group_path[1:]
        current_offset: int | None = current_group.offset

        if current_group not in self.next_groups:
            next_trie = MultiTapeProductTrie(offset=current_offset)
            self.next_groups[current_group] = next_trie
        else:
            next_trie = self.next_groups[current_group]

        next_trie._insert_group_path(next_groups, product=product)
        self.has_nested_products = True
        return next_trie

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
        all_products: set[PyMultiTapeProduct] = self.end_products.copy()
        if not group_path:
            return all_products

        current_group, next_groups = group_path[0], group_path[1:]
        if current_group.offset is None:
            raise ValueError("Blank groups are not allowed")

        matching_groups = self.match_offset_groups_for(
            target_group=current_group
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
    ) -> MultiTapeProductTrie:
        group_path = self.create_group_path(terms)
        return self._insert_group_path(group_path, product=product)

    def insert_product(
            self, product: PyMultiTapeProduct
    ) -> MultiTapeProductTrie:
        terms = product.get_flat_terms()
        return self._insert_term_path(terms, product=product)

    def has_term_path(self, terms: list[D]) -> bool:
        group_path = self.create_group_path(terms)
        return self._has_group_path(group_path)

    def has_product(self, product: PyMultiTapeProduct) -> bool:
        terms = product.get_flat_terms()
        return self.has_term_path(terms)

    def search_all_nested_products(self) -> set[PyMultiTapeProduct]:
        all_products: set[PyMultiTapeProduct] = set()

        for next_group in self.next_groups:
            sub_products = self.next_groups[next_group].search_all_products()
            all_products |= sub_products

        return all_products

    def search_all_products(self) -> set[PyMultiTapeProduct]:
        all_products: set[PyMultiTapeProduct] = self.end_products.copy()

        for next_group in self.next_groups:
            sub_products = self.next_groups[next_group].search_all_products()
            all_products |= sub_products

        return all_products

    def search_all_offsets(self) -> set[int]:
        if self.offset is None:
            all_offsets: set[int] = set()
        else:
            all_offsets: set[int] = {self.offset}

        for next_group in self.next_groups:
            sub_offsets = self.next_groups[next_group].search_all_offsets()
            all_offsets |= sub_offsets

        return all_offsets
