/-
  Runnable tests for the counter automata formalisation.

  These are the executable assertions behind the theorems in
  `CounterProof.lean`: they evaluate `halfReduce` at concrete inputs and
  check the invariant `halfReduce b n = (n + 1) / 2`, plus base-independence
  and monotonicity, at specific values.  Run them with:

      lake exe CounterProof

  (see the `lakefile.lean` executable target, or simply:
      lean --run AutomataBuilder/Counter/Tests.lean)

  Each `#eval` line both *type-checks* (proving the equality) and
  *evaluates* to `true`, so a `false` would indicate a broken invariant.
-/
import AutomataBuilder.Counter.CounterProof
import Mathlib.Tactic

open CounterProof

/-! ### Correctness at concrete sizes -/

/-- `n = 1`: one unary cell -> value 1 (ceil (1/2) = 1). -/
#eval (halfReduce 2 1 = (1 + 1) / 2) -- true
#eval (halfReduce 5 1 = 1)         -- true

/-- `n = 2`: two unary cells -> value 1 (ceil (2/2) = 1). -/
#eval (halfReduce 3 2 = (2 + 1) / 2) -- true
#eval (halfReduce 8 2 = 1)           -- true

/-- `n = 4`: four unary cells -> value 2 (ceil (4/2) = 2). -/
#eval (halfReduce 2 4 = 2)  -- true
#eval (halfReduce 10 4 = 2) -- true

/-- `n = 10`: ten unary cells -> value 5 (ceil (10/2) = 5). -/
#eval (halfReduce 2 10 = 5)   -- true
#eval (halfReduce 16 10 = 5)  -- true

/-! ### Base independence at concrete sizes -/

/-- The same `n` gives the same value in every base >= 2. -/
#eval (halfReduce 2 7 = halfReduce 5 7)  -- true
#eval (halfReduce 3 9 = halfReduce 16 9) -- true
#eval (halfReduce 2 9 = halfReduce 10 9) -- true

/-! ### Monotonicity at concrete sizes -/

/-- More input never decreases the half-reduction. -/
#eval (halfReduce 2 3 <= halfReduce 2 5)   -- true
#eval (halfReduce 16 4 <= halfReduce 16 8) -- true
#eval (halfReduce 2 1 <= halfReduce 2 100) -- true

/-! ### Sanity of the `halfReduce_eq_ceil_half` theorem by `simp`-checking -/

/-- The theorem discharges to `rfl` for arbitrary `n` (base-independent). -/
example (n : ℕ) (b : ℕ) (hb : b >= 2) : halfReduce b n = (n + 1) / 2 :=
  halfReduce_eq_ceil_half b n hb

/-- And it is stable under rewriting: `halfReduce b n` simplifies to `(n+1)/2`. -/
example (n : ℕ) (b : ℕ) (hb : b >= 2) : halfReduce b n * 2 = (n + 1) / 2 * 2 := by
  rw [halfReduce_eq_ceil_half b n hb]
