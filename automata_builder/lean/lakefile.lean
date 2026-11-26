import Lake
open Lake

/-- Build configuration for the Lean formalisation of the
    unary-to-n-ary counter automata. -/
package «automata_builder_lean» where
  name := "automata_builder_lean"
  version := v1
  -- Lean source files: the main proof and its runnable tests.
  leanFileGlobs := #[.subPath "AutomataBuilder/**/*.lean"]
  -- Dependency: mathlib (Nat lemmas + tactics like `omega`).
  require mathlib from git
    "https://github.com/leanprover-community/mathlib4" @ "main"
  -- Executable that runs the concrete invariant checks.
  exe «CounterProof» where
    root := "AutomataBuilder.Counter.Main"
