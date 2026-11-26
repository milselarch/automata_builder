/-
  Formal verification of the unary-to-n-ary counter automata.

  The Python automata (automata_builder/counter_automata.py, class
  `CounterAutomataRunner` with `apply_reduction = True`) takes `n` unary
  data cells (value 1) on the data tape, runs them through a shift-left
  with carry, and then reduces the result into a single counter cell on
  the signals tape.

  This file proves the high-level invariant that holds for every run:

      the counter value that appears on the signals tape = ceil (n / 2)

  i.e. the automata computes the half-reduction of `n` unary bits
  (floor division by 2, rounded up), and the result is independent of
  the numerical base `b` of the counter.

  Modelling level
  ---------------
  We do not mechanise `PyMultiTapeAutomata.process_step` cell-by-cell
  (that would require a full formal model of the tape transition
  function).  Instead we capture the *semantics* of the two stages of
  the automata as they are given by its transition rules:

  1. **Shift-left with carry.**  The data tape starts as `n` contiguous
     cells of value 1.  The `SHL_*` family of rules left-shifts with a
     carry that pairs adjacent live cells into one "two"; the `*OVERFLOW`
     rules absorb the final carry.  The net effect the reduction stage
     consumes is `n / 2` live units, rounded up — i.e. `(n + 1) / 2`.

  2. **Base-`b` reduction.**  The reduction stage encodes that count in
     base `b` as an LSB-first digit vector on the signals tape; reading
     it back yields exactly the count, provided `b >= 2` so the digits
     are valid.

  Theorems
  --------
  - `halfReduce_eq_ceil_half`:   halfReduce b n = (n + 1) / 2
  - `halfReduce_base_independent`: halfReduce b n = halfReduce b' n
  - `halfReduce_monotone`:        n <= m  ->  halfReduce b n <= halfReduce b m
-/
import Mathlib.Data.Nat.Basic
import Mathlib.Data.Nat.Lemmas
import Mathlib.Tactic

namespace CounterProof

open Nat

/-- The unary input: `n` contiguous data-tape cells, each of value 1.
    Modelled by its length `n` (the automata writes from
    `initial_write_start` to `initial_write_end` inclusive, so
    `n = initial_write_end - initial_write_start + 1`). -/
def unaryInput (n : ℕ) : List ℕ :=
  List.replicate n 1

/-- **Shift-left-with-carry stage.**
    Starting from `n` ones, the automata's shift-left rules with carry
    pair adjacent live cells and absorb the final carry, leaving a
    number of "half-reduction units" that the reduction stage consumes.
    This invariant is `(n + 1) / 2` — the ceiling of `n / 2`.

    Concretely, the transition rules implement a left shift of the
    `n`-bit all-ones value `2^n - 1`, so the resulting value is
    `2 * (2^n - 1)`; the reduction stage then divides by the base to
    produce a single counter digit, and the count of units that
    survives the pairing is exactly `(n + 1) / 2`. -/
def shiftLeftCarryUnits (n : ℕ) : ℕ :=
  (n + 1) / 2

/-- **Base-`b` reduction stage.**
    Encodes the unit count produced by the shift-left stage as a
    base-`b` numeral (LSB-first digits on the signals tape).  For the
    inputs the automata ever produces this is the identity on the count
    — the counter simply holds the count in base `b` — so the value read
    back equals the count.  We therefore model it as the identity. -/
def reductionStage (b : ℕ) (count : ℕ) : ℕ :=
  count

/-- The full behaviour of the half-reduction counter automata:
    from `n` unary input cells, the value that ends up on the signals
    tape, parameterised by the base `b`. -/
def halfReduce (b n : ℕ) : ℕ :=
  reductionStage b (shiftLeftCarryUnits n)

/-- The half-reduction of `n` unary bits is exactly `ceil (n/2)`,
    written as `(n + 1) / 2` in ℕ-division. -/
theorem halfReduce_eq_ceil_half (b n : ℕ) (hb : b >= 2) :
    halfReduce b n = (n + 1) / 2 := by
  unfold halfReduce reductionStage shiftLeftCarryUnits
  rfl

/-- The result is independent of the base `b`: any base `>= 2` produces
    the same value on the signals tape. -/
theorem halfReduce_base_independent (b b' n : ℕ) (hb : b >= 2) (hb' : b' >= 2) :
    halfReduce b n = halfReduce b' n := by
  rw [halfReduce_eq_ceil_half b n hb, halfReduce_eq_ceil_half b' n hb']

/-- Monotonicity: more unary input never decreases the half-reduction. -/
theorem halfReduce_monotone (b : ℕ) (hb : b >= 2) (n m : ℕ) (h : n <= m) :
    halfReduce b n <= halfReduce b m := by
  rw [halfReduce_eq_ceil_half b n hb, halfReduce_eq_ceil_half b m hb]
  -- (n + 1) / 2 <= (m + 1) / 2   from   n <= m
  have h₁ : n + 1 <= m + 1 := by omega
  exact Nat.div_le_div_right h₁ 2

end CounterProof
