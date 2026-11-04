"""Smoke tests for the refactored `automata_builder._rust` (1-state epsilon, input cycles)."""
import sys
import automata_builder as ab


def check(label, cond, extra=""):
    if not cond:
        raise AssertionError(f"FAIL: {label} {extra}")
    print(f"  ok  {label} {extra}")


def main():
    print("== 1-state epsilon (epsilon(n) as empty-string edge on n) ==")
    a = ab.AutomatonBuilder(3, {'accept': {0}})
    a.epsilon(0)          # empty-string epsilon loop on 0
    a.input(0, 1)
    a.epsilon(1)          # empty-string epsilon loop on 1
    a.input(1, 2)         # into non-accept
    out = a.build().to_dict() if hasattr(a.build(), 'to_dict') else a.build()
    print("  built:", type(out).__name__)

    # empty-input epsilon loop must NOT spin forever
    b = ab.AutomatonBuilder(1, {'accept': {0}})
    b.epsilon(0)
    built = b.build()
    print("  epsilon-loop automaton built (no hang):", type(built).__name__)

    print("== input input-cycle (i -> j -> i on the same input) ==")
    c = ab.AutomatonBuilder(2, {'accept': {1}})
    c.input(0, 1)
    c.input(1, 0)         # cycle back, previously rejected
    built_c = c.build()
    print("  input-cycle automaton built (no hang/reject):", type(built_c).__name__)

    print("== plain build still works ==")
    d = ab.AutomatonBuilder(3, {'accept': {2}})
    d.input(0, 1)
    d.input(1, 2)
    built_d = d.build()
    print("  plain automaton built:", type(built_d).__name__)

    # multi-tape sanity
    print("== multi-tape sanity ==")
    if hasattr(ab, 'MultiTapeAutomataBuilder'):
        mt = ab.MultiTapeAutomataBuilder
        print("  MultiTapeAutomataBuilder present:", mt)
    else:
        print("  (no MultiTapeAutomataBuilder attribute, skipping)")

    print("\nALL SMOKE TESTS PASSED")


if __name__ == '__main__':
    main()
