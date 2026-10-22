# automata-tui

Interactive terminal player for the multi-tape cellular automata implemented
by the `automata_builder` rust crate.

The tapes are rendered with the crate's own `RenderFrame` text representation
(`MultiTapeAutomata::render_tapes`), so the TUI always shows exactly what the
simulation engine produces.

Uses ratatui - see
[https://ratatui.rs/tutorials/hello-ratatui/](https://ratatui.rs/tutorials/hello-ratatui/)
for tutorials.
Run using `cargo run`, run the tests using `cargo test`.

## Controls

| Key | Action |
| --- | --- |
| `space` | start / stop the simulation |
| `n` | advance a single generation |
| `r` | restart from the initial tape state |
| `left` / `right` | move the selected cell one position |
| `up` / `down` | select the tape above / below |
| `page up` / `page down` | move the selected cell a whole screen |
| `c` | centre the view on the selected cell |
| `tab` / `shift-tab` | select a product write of the last generation |
| `enter` | jump the cell selection to the selected product write's cell |
| `+` / `-` | simulate more / fewer generations per second |
| `q`, `ctrl-c`, `ctrl-d` | quit |

Keybindings are defined in [`.config/config.json5`](.config/config.json5) and
can be overridden by a config file placed in the user config directory
(`automata-tui --version` prints its location).

## Panels

- **header** - play state, generation counter, simulation speed and the
  contents of the currently selected cell
- **tapes** - the rendered tapes with the selected cell highlighted
- **product writes** - every write applied by the most recent step, showing the
  annotation, the target `(tape, position)`, the written cell state and the
  product that triggered it; selecting an entry and pressing `enter` moves the
  cell selection onto the cell it wrote to

## Simulated automata

`src/simulation.rs` bundles a small two-tape demo automata: a marker bounces
back and forth along tape 0 while tape 1 records every visited cell. Replace
`build_demo_simulation` with another set of rules (a
`MultiTapeState -> MultiTapeExpression` map plus the initial tape regions) to
play a different automata.
