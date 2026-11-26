/-!
Lean binding for the Rust multi-tape automata backend, declared as opaque
extern C functions. These link against the compiled Rust library `libautomata_builder`.
-/
prelude

opaque MTATransitionsGroup : Type
opaque MTAError : Type
opaque MTAAutomata : Type

/-- Construct a new transitions group (empty product trie). -/
opaque mta_new_transitions_group : IO MTATransitionsGroup

/-- Add a transition to the group. inputTerms: List (tapeNo, offset, state).
    output: (tapeNo, state). -/
opaque mta_add_transition :
  MTATransitionsGroup -> List (Int * Int * Int) -> Int * Int -> Bool -> Bool -> String -> Int ->
  IO (Except MTAError PUnit)

/-- Build the automata from the transition group (constructs the product trie). -/
opaque mta_build : MTATransitionsGroup -> IO (Except MTAError MTAAutomata)

/-- Initialize all tapes to void (0) for `numTapes` tapes. -/
opaque mta_initialize_tapes : MTAAutomata -> Int -> IO (Except MTAError PUnit)

/-- Set a single cell. -/
opaque mta_set_cell : MTAAutomata -> Int -> Int -> Int -> IO (Except MTAError PUnit)

/-- Get a single cell. -/
opaque mta_get_cell : MTAAutomata -> Int -> Int -> IO (Except MTAError Int)

/-- Process one step. -/
opaque mta_process_step : MTAAutomata -> IO (Except MTAError PUnit)
