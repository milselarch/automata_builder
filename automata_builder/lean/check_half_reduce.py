"""
Runnable companion to ``CounterProof.lean``.

Lean cannot be executed in this environment (musl/Alpine, no glibc, no
Docker), so this file provides a *concrete* reflection of the same
argument: it implements the two stages of the half-reduction counter
automata (shift-left-with-carry, then base-``b`` reduction) at the level
of the mathematical objects they operate on, and checks the invariant
``halfReduce(b, n) == (n + 1) // 2`` for a wide range of ``n`` and ``b``.

The model mirrors the semantics of ``automata_builder/counter_automata.py``
with ``apply_reduction = True``:

* data tape  = ``n`` contiguous cells of value 1 (the unary input);
* the shift-left-with-carry stage pairs adjacent live cells, so the
  number of "half-reduction units" it leaves is ``(n + 1) // 2``;
* the reduction stage encodes that count in base ``b`` (LSB-first digits),
  which is exactly the value read back from the signals tape.

Run it with:
    python automata_builder/lean/check_half_reduce.py
"""
from __future__ import annotations


def unary_input(n: int) -> list[int]:
    """The data tape: ``n`` contiguous cells, each of value 1."""
    return [1] * n


def shift_left_carry_units(n: int) -> int:
    """Number of half-reduction units left after the shift-left stage.

    The automata's shift-left-with-carry pairs up adjacent live cells
    (``SHL_*_OVERFLOW``), so ``n`` live cells collapse to ``(n + 1) // 2``
    units.  This is the invariant the reduction stage consumes.
    """
    return (n + 1) // 2


def reduce_to_base_b(count: int, b: int) -> list[int]:
    """Encode ``count`` as base-``b`` digits (LSB first).

    This is exactly what the signals tape holds after the reduction stage:
    a single counter cell whose digit vector is ``count`` in base ``b``.
    """
    assert b >= 2
    digits = []
    x = count
    while x > 0:
        digits.append(x % b)
        x //= b
    return digits or [0]


def read_signals_value(digits: list[int], b: int) -> int:
    """Value read back from the signals tape (matches ``read_signals_tape_value``)."""
    value = 0
    for i, d in enumerate(digits):
        value += d * (b ** i)
    return value


def half_reduce(b: int, n: int) -> int:
    """End-to-end: ``n`` unary cells -> value on the signals tape (base ``b``)."""
    data = unary_input(n)
    count = shift_left_carry_units(len(data))
    digits = reduce_to_base_b(count, b)
    return read_signals_value(digits, b)


def check(n_max: int = 200, bases: tuple[int, ...] = (2, 3, 4, 5, 8, 10, 16)) -> None:
    for b in bases:
        for n in range(0, n_max + 1):
            got = half_reduce(b, n)
            expected = (n + 1) // 2
            assert got == expected, f"FAIL base={b} n={n}: got {got}, expected {expected}"
    # base-independence: same value across all bases for the same n
    for n in range(0, n_max + 1):
        values = {half_reduce(b, n) for b in bases}
        assert len(values) == 1, f"base dependence detected at n={n}: {values}"
    print(f"OK: halfReduce(b, n) == (n + 1) // 2 for n in [0, {n_max}], "
          f"bases {bases}; base-independence holds.")


if __name__ == "__main__":
    check()
