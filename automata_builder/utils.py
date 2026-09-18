from __future__ import annotations

import copy
import typing

from collections.abc import Iterable
from abc import ABCMeta, abstractmethod
from collections import defaultdict
from typing import TypeVar, Iterator, Tuple, Sequence, Generic, Callable
from dataclasses import is_dataclass
from automata_builder._rust import D, A, PyMultiTapeProduct, PyProduct

T = TypeVar('T')
U = TypeVar('U')


def zip_preserve_types(
    a: Sequence[T], b: Sequence[U]
) -> Iterator[Tuple[T, U]]:
    if len(a) != len(b):
        raise ValueError("Input sequences must have the same length")

    for k in range(len(a)):
        yield a[k], b[k]


def cartesian_product(
    sequences: Sequence[Sequence[T]]
) -> Iterator[Tuple[T, ...]]:
    if not sequences:
        yield ()
        return

    first, *rest = sequences
    for item in first:
        for rest_items in cartesian_product(rest):
            yield item, *rest_items


def is_frozen_dataclass_instance(obj: object) -> bool:
    if not is_dataclass(obj) or isinstance(obj, type):
        return False

    # __dataclass_params__ exists on dataclass types
    return bool(getattr(type(obj), "__dataclass_params__").frozen)


K = TypeVar("K")
V = TypeVar("V")


class Freezable(metaclass=ABCMeta):
    def __init__(self) -> None:
        self._frozen: bool = False

    @property
    def is_frozen(self) -> bool:
        return self._frozen

    @staticmethod
    def is_allowed_value(value: typing.Any) -> bool:
        if isinstance(value, (int, float, str, bool)):
            return True
        elif isinstance(value, (A, D, PyMultiTapeProduct, PyProduct)):
            # TODO: find a way to check if maturin class is immutable instead?
            return True
        elif is_frozen_dataclass_instance(value):
            return True
        elif isinstance(value, Freezable):
            return True

        return False

    def freeze(self) -> bool:
        if self._frozen:
            return False

        self._freeze()
        self._frozen = True
        return True

    @abstractmethod
    def _freeze(self) -> None:
        raise NotImplementedError

    def encode(self):
        return self._encode()

    @abstractmethod
    def _encode(self) -> tuple:
        raise NotImplementedError

    @classmethod
    def decode(cls, data: tuple):
        return cls._decode(data)

    @classmethod
    @abstractmethod
    def _decode(cls, data: tuple) -> Freezable:
        raise NotImplementedError

    @abstractmethod
    def to_unfrozen(self):
        raise NotImplementedError

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, type(self)):
            return False

        return self.encode() == other.encode()

    def __hash__(self) -> int:
        classname = self.__class__.__name__
        if not self._frozen:
            raise ValueError(f"Cannot hash {classname} when not frozen")

        return hash(self.encode())


class FreezableDict(Freezable, Generic[K, V]):
    def __init__(
        self, initial_data: typing.Optional[dict[K, V]] = None
    ) -> None:
        super().__init__()
        self._data: dict[K, V] = {}

        if initial_data is not None:
            for key, value in initial_data.items():
                self._data[key] = value

    def _freeze(self) -> None:
        for value in self._data.values():
            if isinstance(value, Freezable):
                value.freeze()

    def __delitem__(self, key: K):
        if self._frozen:
            raise ValueError(
                f"Cannot delete from frozen {self.__class__.__name__}"
            )

        del self._data[key]

    def __getitem__(self, key: K) -> V:
        classname = self.__class__.__name__
        if self._frozen and key not in self._data:
            raise KeyError(
                f"Cannot create missing key in frozen {classname}"
            )

        return self._data[key]

    def __setitem__(self, key: K, value: V) -> None:
        classname = self.__class__.__name__
        if self._frozen:
            raise ValueError(f"Cannot modify frozen {classname}")

        if not self.is_allowed_value(value):
            raise ValueError(f"Value {value} is not allowed")

        self._data[key] = value

    def __contains__(self, key: object) -> bool:
        return key in self._data

    def to_dict(self):
        return copy.deepcopy(self._data)

    def get(self, key: K, default: V | None = None) -> V | None:
        return self._data.get(key, default)

    def items(self):
        return self._data.items()

    def keys(self):
        return self._data.keys()

    def values(self):
        return self._data.values()

    def pop(self, key: K) -> V:
        if self._frozen:
            raise ValueError(
                f"Cannot pop from frozen {self.__class__.__name__}"
            )

        return self._data.pop(key)

    def __iter__(self) -> Iterator[K]:
        return iter(self._data)

    def __len__(self) -> int:
        return len(self._data)

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}({dict(self._data)!r})"

    def _encode(self) -> tuple[tuple[K, V], ...]:
        keys = sorted(list(self.keys()))
        return tuple([
            (key, self._data[key]) for key in keys
        ])

    @classmethod
    def _decode(
        cls, data: tuple[tuple[K, V], ...]
    ) -> FrozenDict[K, V]:
        instance = FreezableDict()
        for key, value in data:
            instance[key] = value

        return instance.to_frozen()

    def to_default_dict(
        self
    ) -> FreezableDefaultDict[K, V]:
        default_dict = FreezableDefaultDict(
            default_factory=lambda: None,
            initial_data=self._data
        )
        return default_dict

    def to_frozen(self) -> FrozenDict[K, V]:
        return FrozenDict(initial_data=self._data)

    def to_unfrozen(self) -> FreezableDict[K, V]:
        unfrozen_data = {}
        for key, value in self._data.items():
            if isinstance(value, Freezable):
                unfrozen_data[key] = value.to_unfrozen()
            else:
                unfrozen_data[key] = value

        return FreezableDict(initial_data=unfrozen_data)


class FrozenDict(FreezableDict[K, V]):
    def __init__(
        self, initial_data: typing.Optional[dict[K, V]] = None
    ) -> None:
        super().__init__(initial_data)
        self.freeze()

    def to_frozen(self) -> FrozenDict[K, V]:
        return self

    def to_unfrozen(self) -> FreezableDict[K, V]:
        return super().to_unfrozen()


class FreezableDefaultDict(FreezableDict[K, V]):
    def __init__(
        self, default_factory: Callable[[], V],
        initial_data: typing.Optional[dict[K, V]] = None
    ) -> None:
        super().__init__(initial_data=initial_data)
        self._default_factory = default_factory
        self._data: defaultdict[K, V] = defaultdict(default_factory, self._data)

    def __getitem__(self, key: K) -> V:
        classname = self.__class__.__name__
        if self._frozen and key not in self._data:
            raise KeyError(
                f"Cannot create missing key in frozen {classname}"
            )

        return self._data[key]

    @classmethod
    def _decode(
        cls, data: tuple[tuple[K, V], ...]
    ) -> FrozenDict[K, V]:
        instance = FreezableDict()
        for key, value in data:
            instance[key] = value

        return instance.to_frozen()

    def to_frozen(self) -> FrozenDefaultDict[K, V]:
        return FrozenDefaultDict(
            default_factory=self._default_factory,
            initial_data=self._data
        )

    def to_unfrozen(self) -> FreezableDefaultDict[K, V]:
        unfrozen_data = {}
        for key, value in self._data.items():
            if isinstance(value, Freezable):
                unfrozen_data[key] = value.to_unfrozen()
            else:
                unfrozen_data[key] = value

        return FreezableDefaultDict(
            default_factory=self._default_factory,
            initial_data=unfrozen_data
        )

    def to_freezable_dict(self):
        if self._frozen:
            raise ValueError("cannot convert to freezable dict when frozen")

        freezable_dict = FreezableDict(initial_data=self._data)
        return freezable_dict

    def to_frozen_dict(self):
        if not self._frozen:
            raise ValueError("cannot convert to frozen dict when not frozen")

        frozen_dict = FrozenDict(initial_data=self._data)
        return frozen_dict


class FrozenDefaultDict(FreezableDefaultDict[K, V]):
    def __init__(
        self, default_factory: Callable[[], V],
        initial_data: typing.Optional[dict[K, V]] = None
    ) -> None:
        super().__init__(
            default_factory=default_factory,
            initial_data=initial_data
        )
        self.freeze()


class FreezableSet(FreezableDict[V, V], Generic[V]):
    def __init__(self, args: typing.Collection[V] = ()) -> None:
        Freezable.__init__(self)
        self._data: set[V] = set()
        for arg in args:
            self.add(arg)

    def __iter__(self) -> Iterator[V]:
        return iter(self._data)

    def __repr__(self):
        classname = self.__class__.__name__
        return f"{classname}({self._data!r})"

    def remove(self, value: V):
        if self._frozen:
            raise ValueError(
                f"Cannot delete from frozen {self.__class__.__name__}"
            )

        self._data.remove(value)

    def clone_data(self) -> set[V]:
        return copy.copy(self._data)

    def add(self, other: V) -> None:
        if self._frozen:
            raise ValueError("Cannot add to frozen set")

        if not self.is_allowed_value(other):
            raise ValueError(f"Value {other} is not allowed")

        self._data.add(other)

    def _freeze(self) -> None:
        for value in self._data:
            if isinstance(value, Freezable):
                value.freeze()

    def _encode(self) -> tuple[V, ...]:
        return tuple(list(self._data))

    @classmethod
    def _decode(cls, data: tuple[V, ...]) -> typing.Self:
        return cls(data)

    def to_frozen(self) -> FrozenSet[V]:
        frozen_set = FrozenSet(self._data)
        return frozen_set

    def to_unfrozen(self):
        unfrozen_elements = []
        for element in self._data:
            if isinstance(element, Freezable):
                unfrozen_elements.append(element.to_unfrozen())
            else:
                unfrozen_elements.append(element)

        unfrozen_set = FreezableSet(unfrozen_elements)
        return unfrozen_set


class FrozenSet(FreezableSet[V]):
    def __init__(self, args: typing.Collection[V] = ()) -> None:
        super().__init__(args)
        self.freeze()

    def to_unfrozen(self) -> FreezableSet[V]:
        raise ValueError("Cannot be unfrozen")


class RestoringList(Generic[V]):
    def __init__(self, items: typing.Iterable[T] = ()) -> None:
        self._items: list[T] = list(items)
        self._context_popped_items: list[tuple[int, T]] | None = None

    def __enter__(self) -> RestoringList[T]:
        if self._context_popped_items is not None:
            raise RuntimeError("RestoringList context is already entered")

        self._context_popped_items = []
        return self

    def __exit__(self, exc_type: object, exc: object, tb: object) -> bool:
        if self._context_popped_items is None:
            raise RuntimeError("RestoringList context was not entered")

        popped_items = self._context_popped_items
        self._context_popped_items = None

        # Restore in reverse pop order so the list returns to its prior state.
        for index, value in reversed(popped_items):
            self._items.insert(index, value)

        return False  # do not suppress exceptions

    def pop(self, index: int = -1) -> T:
        value = self._items.pop(index)

        if self._context_popped_items is not None:
            # Normalize the negative index to the original
            # positive position.
            if index < 0:
                index += len(self._items) + 1

            self._context_popped_items.append((index, value))

        return value

    def append(self, value: T) -> None:
        self._items.append(value)

    def extend(self, values: Iterable[T]) -> None:
        self._items.extend(values)

    def insert(self, index: int, value: T) -> None:
        self._items.insert(index, value)

    def remove(self, value: T) -> None:
        index = self._items.index(value)
        self.pop(index)

    def clear(self) -> None:
        while self._items:
            self.pop()

    @typing.overload
    def __getitem__(self, index: int) -> T: ...
    @typing.overload
    def __getitem__(self, index: slice) -> list[T]: ...
    def __getitem__(self, index: int | slice) -> T | list[T]:
        return self._items[index]

    @typing.overload
    def __setitem__(self, index: int, value: T) -> None: ...
    @typing.overload
    def __setitem__(self, index: slice, value: Iterable[T]) -> None: ...
    def __setitem__(self, index: int | slice, value: T | Iterable[T]) -> None:
        self._items[index] = value  # type: ignore[index, assignment]

    def __delitem__(self, index: int | slice) -> None:
        if isinstance(index, slice):
            # Do deletion from right to left so earlier indexes remain valid.
            indexes = range(*index.indices(len(self._items)))
            for i in reversed(list(indexes)):
                self.pop(i)
        else:
            self.pop(index)

    def __len__(self) -> int:
        return len(self._items)

    def __iter__(self) -> Iterator[T]:
        return iter(self._items)

    def __contains__(self, value: object) -> bool:
        return value in self._items

    def __repr__(self) -> str:
        return repr(self._items)
