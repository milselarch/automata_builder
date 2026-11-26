/-
  Formal verification of the base-2 counter automata rules.

  The Python automata (automata_builder/counter_automata.py, class
  `CounterAutomataBuilder` with `base = 2`) implements a binary counter
  that increments by 1 each time the counter head passes over a data
  cell on the data tape.  The counter is stored on the signals tape,
  LSB-first (left = least significant, right = most significant).

  This file proves that the counter rules correctly implement
  increment-by-1 in base 2, and that after k increments the counter
  encodes the value k.

  Theorems
  --------
  - `inc_binary_correct`:   val₂ (inc₂ w) = val₂ w + 1
  - `counter_value_after_k`: after k increments from zero, counter = k
  - `digits_valid`:          all digits ∈ {0, 1}
  - `counter_monotone`:      increments never decrease the value
-/
import Mathlib.Data.Nat.Basic
import Mathlib.Data.List.Basic
import Mathlib.Data.Nat.Lemmas
import Mathlib.Tactic

namespace CounterProofBase2

open List

/-- A binary counter is a list of bits, LSB first. -/
def BinaryCounter := List Bool

/-- The numerical value of a binary counter (LSB first) in ℕ.
    `val₂ [a₀, a₁, ..., aₙ₋₁] = a₀ + 2·a₁ + 4·a₂ + ...` -/
def val₂ (w : List Bool) : ℕ :=
  w.zip (List.range w.length) |>.sum (fun p => if p.1 then 2 ^ p.2 else 0)

/-- Increment a binary counter by 1.
    - If the LSB is 0, set it to 1 and done.
    - If the LSB is 1, set it to 0 and carry to the rest.
    - If the list is empty (value 0), return [true] (value 1). -/
def inc₂ (w : List Bool) : List Bool :=
  match w with
  | [] => [true]
  | false :: rest => true :: rest
  | true :: rest => false :: inc₂ rest

/-- The increment of a binary counter adds exactly 1 to its value. -/
theorem inc_binary_correct (w : List Bool) :
    val₂ (inc₂ w) = val₂ w + 1 := by
  induction w with
  | nil =>
    unfold inc₂ val₂
    simp
  | cons b rest ih =>
    unfold inc₂ val₂
    -- Case split on b
    split_ifs at * <;> simp_all [inc₂]
    <;> try omega
    <;> try
      (
        -- For the carry case (b = true):
        -- val₂ (false :: inc₂ rest) = val₂ (inc₂ rest) * 2^1 + 0
        -- = 2 * val₂ (inc₂ rest)
        -- val₂ (true :: rest) = 1 + 2 * val₂ rest
        -- Need: 2 * val₂ (inc₂ rest) = 1 + 2 * val₂ rest + 1
        -- By IH: val₂ (inc₂ rest) = val₂ rest + 1
        -- So LHS = 2 * (val₂ rest + 1) = 2 * val₂ rest + 2
        -- RHS = 2 * val₂ rest + 2  ✓
        ring_nf
        rw [ih]
        ring
      )

/-- All digits in a binary counter are valid bits (trivially true
    since the type is Bool). -/
theorem digits_valid (w : List Bool) :
    ∀ b ∈ w, b = true ∨ b = false := by
  intro b hb
  exact Or.inr (by simp [hb])

/-- After k increments from the zero counter (empty list),
    the counter encodes the value k. -/
theorem counter_value_after_k (k : ℕ) :
    val₂ (List.foldr (fun _ acc => inc₂ acc) [] (List.replicate k ())) = k := by
  induction k with
  | zero =>
    unfold val₂
    simp
  | succ k ih =>
    unfold val₂
    rw [inc_binary_correct]
    exact Nat.succ_eq_add_one k

/-- The counter value is monotonically non-decreasing under
    increments (each increment adds exactly 1). -/
theorem counter_monotone (w : List Bool) :
    val₂ w ≤ val₂ (inc₂ w) := by
  rw [inc_binary_correct]
  omega

end CounterProofBase2
