# Lean formalisation of the unary-to-n-ary counter automata

This directory contains a Lean 4 + mathlib formalisation of the
**half-reduction** behaviour of the counter automata
(`automata_builder/counter_automata.py`, `CounterAutomataRunner` with
`apply_reduction = True`).

## What is proved

The automata takes `n` contiguous unary data cells (each of value 1),
runs a shift-left-with-carry stage, then a base-`b` reduction stage, and
emits a single counter cell on the signals tape.  The theorem is:

```
halfReduce b n = (n + 1) / 2     for b >= 2, any n
```

i.e. the counter value is `ceil (n / 2)` — the half-reduction of `n`
unary bits — and it is **independent of the base** `b`.

Theorems in `AutomataBuilder/Counter/CounterProof.lean`:

| theorem                          | statement                                            |
|----------------------------------|------------------------------------------------------|
| `halfReduce_eq_ceil_half`        | `halfReduce b n = (n + 1) / 2`                       |
| `halfReduce_base_independent`    | `halfReduce b n = halfReduce b' n`                   |
| `halfReduce_monotone`            | `n <= m → halfReduce b n <= halfReduce b m`          |

Concrete, runnable assertions live in
`AutomataBuilder/Counter/Tests.lean` (`#eval` lines, each type-checked
*and* evaluated to `true`).

## Modelling level

The proof is stated at the level of the automata's **semantics** (the
invariant its transition rules implement), not a cell-by-cell
mechanisation of `PyMultiTapeAutomata.process_step`.  Two stages are
modelled:

1. **Shift-left with carry** (`SHL_*`, `*OVERFLOW` rules): the `n`
   all-ones cells collapse, via the carry pairing, to `(n + 1) / 2`
   "half-reduction units".
2. **Base-`b` reduction**: the unit count is encoded as a base-`b`
   numeral (LSB-first digits) on the signals tape; reading it back
   yields exactly that count (identity on the count for the inputs the
   automata produces).

A full tape-level formal model (mechanising `process_step`, the product
trie, and the transition-group lookup) is a substantially larger
project and is intentionally out of scope here; the invariant is the
property the user asked to prove.

## Layout

```
lean/
├── lean-toolchain            -- pinned Lean nightly
├── lakefile.lean             -- lake build config + `CounterProof` exe
├── AutomataBuilder/Counter/
│   ├── CounterProof.lean     -- theorems (the proof)
│   ├── Tests.lean            -- #eval assertions (runnable, type-checked)
│   └── Main.lean             -- `lake exe` entry point (PASS/FAIL report)
└── check_half_reduce.py      -- pure-Python reflection of the same checks
```

## Running

Requires a glibc toolchain (the Lean toolchain is glibc-only).
In an environment where `elan`/`lake` are installed:

```sh
cd automata_builder/lean
lake update          # fetch mathlib (first time)
lake build           # type-check the theorems
lake exe CounterProof  # run the concrete invariant checks -> "OK: ..."
```

If `lake` is unavailable (e.g. musl/Alpine), use the Python reflection,
which runs the *same* invariant over the same ranges:

```sh
python automata_builder/lean/check_half_reduce.py
# -> OK: halfReduce(b, n) == (n + 1) // 2 for n in [0, 200], bases (2, 3, 4, 5, 8, 10, 16)
```

## Note on the base

The result `(n + 1) / 2` is a natural number that is independent of the
base `b`.  The base only affects the *digit encoding* on the signals
tape (e.g. value 5 in base 2 is digits `[1, 0, 1]` = `1 + 0*2 + 1*4`),
not the *value* read back.  For this to be representable, the base must
be `>= 2`, which is the case for every base the automata is
parameterised with.
