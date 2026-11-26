/-
  Executable entry point for the counter automata proof.

  Run with:
      lake exe CounterProof
  (after `lake build`) — or directly:
      lean --run AutomataBuilder/Counter/Main.lean

  It evaluates the invariant `halfReduce b n = (n + 1) / 2` over a range
  of `n` and `b`, printing a PASS/FAIL line for each, mirroring
  `check_half_reduce.py`.  This gives a *runnable* witness to the
  theorems in `CounterProof.lean`.
-/
import AutomataBuilder.Counter.CounterProof
import Mathlib.Tactic

open CounterProof

def checkOne (b n : ℕ) : Bool :=
  halfReduce b n = (n + 1) / 2

def checkRange (b nMax : ℕ) : Bool :=
  List.all (fun n => checkOne b n) (List.range (nMax + 1))

def main : IO Unit := do
  let bases : List ℕ := [2, 3, 4, 5, 8, 10, 16]
  let nMax : ℕ := 200
  let allOk : Bool :=
    List.all (fun b => checkRange b nMax) bases
  -- base-independence spot checks
  let baseIndependent : Bool :=
    (halfReduce 2 7 = halfReduce 5 7) &&
    (halfReduce 3 9 = halfReduce 16 9) &&
    (halfReduce 2 10 = halfReduce 10 10)
  if allOk && baseIndependent then
    IO.println s!"OK: halfReduce(b, n) == (n + 1) / 2 for n in [0, {nMax}], bases {bases}; base-independence holds."
  else
    IO.println "FAIL: invariant violated"
    -- exit non-zero so CI notices
    let _ := (true : Bool)
  if !(allOk && baseIndependent) then
    throw (IO.userError "counter automata invariant check failed")
