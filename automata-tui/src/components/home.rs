use color_eyre::Result;
use indexmap::IndexMap;
use ratatui::{prelude::*, widgets::*};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::{action::Action, config::Config};
use automata_builder::automata::composer::{MultiTape, Tape, TapeCellState};

const NUM_NAMED_COLORS: usize = 16;
const NAMED_COLORS: [Color; NUM_NAMED_COLORS] = [
    Color::Black,
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::Gray,
    Color::DarkGray,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightCyan,
    Color::White,
];

fn clip_string(s: &str, max_chars: usize) -> &str {
    s.char_indices()
        .nth(max_chars)
        .map(|(idx, _)| &s[..idx])
        .unwrap_or(s)
}

#[derive(Default)]
pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    left_position: i64,
    multi_tape_state: MultiTape,
}

pub struct VerticalAccumulator<'a> {
    paragraphs: Vec<Paragraph<'a>>,
}
impl<'a> VerticalAccumulator<'a> {
    pub fn new() -> Self {
        VerticalAccumulator {
            paragraphs: Vec::new(),
        }
    }
    pub fn add_paragraph(&mut self, paragraph: Paragraph<'a>) {
        self.paragraphs.push(paragraph);
    }
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        for (index, paragraph) in self.paragraphs.iter().enumerate() {
            let y = area.y + index as u16;
            let paragraph_area = Rect::new(area.x, y, area.width, 1);
            frame.render_widget(paragraph.clone(), paragraph_area);
        }
    }
}

impl Home {
    pub fn new() -> Self {
        let mut new_instance = Self::default();
        let new_tape = Tape::default();
        new_instance.multi_tape_state.insert_named_tape(
            "tape0".to_string(),
            new_tape
        ).unwrap();
        new_instance
    }
    fn render_for_tape(
        &self, tape: &Tape, start_cell_pos: i64,
        width: i64, cell_width: usize
    ) -> String {
        let mut row_str = String::new();
        let mut cell_pos = start_cell_pos;

        loop {
            let current_len = row_str.len();
            let new_len = current_len + cell_width + 1; // +1 for the separator
            if new_len > (width as usize) { break; }

            let cell_value = tape.read(cell_pos);
            let cell_str = format!("{:0cell_width$x}", cell_value);

            row_str.push_str(&cell_str);
            row_str.push_str("|");
            cell_pos += 1;
        }
        row_str
    }
    fn render_positions(&self, width: usize, cell_width: usize) -> String {
        // have to consider border between cells, hence the +1 in (cell_width + 1)
        let max_cells_fit = (width as usize) / (cell_width + 1);
        let right_position = self.left_position + (max_cells_fit as i64) - 1;

        let left_pos_str = self.left_position.to_string();
        let right_pos_str = right_position.to_string();
        let center_spacing = width as usize - left_pos_str.len() - right_pos_str.len();
        let center_spacing_str = " ".repeat(center_spacing);
        let row = format!("{}{}{}", left_pos_str, center_spacing_str, right_pos_str);
        row
    }
}

impl Component for Home {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {
                // add any logic here that should run on every tick
            }
            Action::Render => {
                // add any logic here that should run on every render
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let mut accumulator = VerticalAccumulator::new();
        let width = frame.area().width;
        let row_y: u16 = 0;

        let tape_key_to_names_map = self.multi_tape_state.invert_tape_names_map();
        let tapes = self.multi_tape_state.get_tapes();

        let mut global_max_allowed_state: TapeCellState = 0;
        for tape in tapes.iter() {
            let tape_max_allowed_state = tape.get_max_allowed_state();
            global_max_allowed_state = TapeCellState::max(
                global_max_allowed_state, tape_max_allowed_state
            );
        }

        let cell_width = 1 + (
            global_max_allowed_state as f64
        ).log(16.0).ceil() as usize;

        accumulator.add_paragraph(Paragraph::new("Automata TUI"));
        let positions_row = self.render_positions(width as usize, cell_width);
        accumulator.add_paragraph(Paragraph::new(positions_row));

        for (index, tape) in tapes.iter().enumerate() {
            let tape_key = tape.get_tape_key();
            let tape_name = tape_key_to_names_map.get(&tape_key).unwrap();
            let tape_contents = self.render_for_tape(
                tape, self.left_position, width as i64, cell_width
            );
            let tape_str = format!("{}", tape_contents);
            accumulator.add_paragraph(Paragraph::new(tape_str));
        }

        accumulator.render(frame, area);
        Ok(())
    }
}
