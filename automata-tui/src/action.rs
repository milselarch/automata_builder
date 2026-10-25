use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    ClearScreen,
    Error(String),
    Help,
    /// Start/stop the automata simulation.
    TogglePlay,
    /// Advance the simulation by a single generation.
    StepForward,
    /// Reset the simulation back to its initial tape state.
    Restart,
    /// Move the selected cell one position to the left/right.
    MoveCursorLeft,
    MoveCursorRight,
    /// Move the selection to the tape above/below.
    MoveCursorUp,
    MoveCursorDown,
    /// Move the selected cell a full screen to the left/right.
    PageCursorLeft,
    PageCursorRight,
    /// Centre the view on the selected cell.
    CenterOnCursor,
    /// Cycle through the product writes of the most recent step.
    NextWrite,
    PrevWrite,
    /// Select the cell written to by the currently selected product write.
    SelectWriteCell,
    /// Increase/decrease the number of generations simulated per tick.
    SpeedUp,
    SpeedDown,
}
