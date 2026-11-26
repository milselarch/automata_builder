/-!
Lean binding for the Rust multi-tape automata backend.

Declares the extern C functions (via `attribute`/`opaque` won't work for calling;
we use Lean's `#eval`-style C FFI through the `C` namespace is not available,
so we declare them as opaque and rely on a C shim).

Actually the cleanest approach in Lean 4 is to declare these as `opaque` functions
that are implemented by a C library, using `extern "C"` via the `attribute`.
Lean 4 does not have a direct `extern C` for arbitrary C functions without
the `--extern` mechanism. Instead, we use `#eval`-style `IO` and a C wrapper.

The standard way: declare `opaque` defs and provide C implementations via
`attribute [extern_cc]` is for Lean-generated C, not C->Lean.

So we use a different strategy: a C wrapper library that exposes a C API, and
we call it from Lean via `#eval`? No.

The working approach in Lean 4 for calling C from Lean: use the `IO` namespace
and declare `def` with the `@[extern]` attribute is NOT standard.

Therefore, we wrap the Rust backend in a small C library, and from Lean we use
`IO.FS` + `Process`? No, we need direct calls.

The correct mechanism: Lean 4 supports `opaque` + C implementation via the
`--library` linking. We declare the C functions as `opaque` Lean defs, then
implement them in C (in this repo's C shim), and the linker resolves them at
link time because the names match.

Lean's C FFI: you declare
  opaque foo : ...
and in C define `lean_object* foo(lean_object* a, ...)`. The names must match
(mangled). For simplicity, we use the `--extern` style via the C wrapper that
implements the exact Lean runtime ABI.

This is getting complex. The PRAGMATIC path: use a C shared library and call it
from Python (which already works), OR write the whole thing in Rust and expose
a C ABI that a tiny Lean `#eval`-free program uses via `IO.Process` to run a
`mtasim` CLI binary (Rust). The Lean program then shells out to the Rust CLI.
-/
prelude

import automata_lean_backend

namespace Automata

/-!
## FFI wrapper (opaque C functions)

The Rust crate `automata_builder` is compiled with `crate-type = ["cdylib", "rlib"]`
and exposes a C ABI via `#[no_mangle] extern "C"` functions. We declare those
as opaque Lean functions and let the linker resolve them against the Rust
cdylib when building the `mtasim` executable.
-/

/-- A transitions group handle. -/
opaque TransitionsGroup : Type

/-- An automata handle. -/
opaque Automata : Type

/-- An error handle. -/
opaque MTAError : Type

opaque new_transitions_group : IO TransitionsGroup

opaque add_transition (g : TransitionsGroup)
  (inputTerms : List (Int * Int * Int))
  (outTape : Int) (outState : Int)
  (validateVoid : Bool) (validateHalt : Bool)
  (annotation : String) (priority : Int) :
  IO (Except MTAError PUnit)

opaque build (g : TransitionsGroup) : IO (Except MTAError Automata)

opaque initialize_tapes (a : Automata) (numTapes : Int) : IO (Except MTAError PUnit)

opaque set_cell (a : Automata) (tape : Int) (pos : Int) (state : Int) : IO (Except MTAError PUnit)

opaque get_cell (a : Automata) (tape : Int) (pos : Int) : IO (Except MTAError Int)

opaque process_step (a : Automata) : IO (Except MTAError PUnit)

end Automata
