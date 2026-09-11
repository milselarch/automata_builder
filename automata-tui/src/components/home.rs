use color_eyre::Result;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use tokio::sync::mpsc::UnboundedSender;

use automata_builder::automata::multi_tape_automata::WriteRecord;
use automata_builder::automata::terms_multitape::{AbstractMultiTapeExpression, TapeNo};

use super::Component;
use crate::{
    action::Action,
    config::Config,
    simulation::{Simulation, build_demo_simulation},
};

const MIN_TICKS_PER_STEP: u32 = 1;
const MAX_TICKS_PER_STEP: u32 = 32;

/// Interactive player for a multi-tape cellular automata.
pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    simulation: Simulation,
    /// whether the simulation is currently advancing on its own
    running: bool,
    /// how many ticks have to elapse before the next simulation step
    ticks_per_step: u32,
    ticks_since_step: u32,
    /// leftmost tape position currently visible
    view_left: i64,
    /// currently selected cell: `(tape_no, position)`
    cursor_tape_no: TapeNo,
    cursor_position: i64,
    /// currently selected product write of the most recent step
    selected_write: usize,
    /// scroll state of the product writes list
    writes_list_state: ListState,
    /// number of tape cells that fitted on screen during the last render
    visible_cells: usize,
    status_message: String,
}

impl Home {
    pub fn new() -> Result<Self> {
        let simulation = build_demo_simulation()?;
        Ok(Self {
            command_tx: None,
            config: Config::default(),
            simulation,
            running: false,
            ticks_per_step: 2,
            ticks_since_step: 0,
            view_left: 0,
            cursor_tape_no: 0,
            cursor_position: 0,
            selected_write: 0,
            writes_list_state: ListState::default(),
            visible_cells: 0,
            status_message: String::from("paused"),
        })
    }

    fn step_simulation(&mut self) {
        match self.simulation.step() {
            Ok(()) => {
                let num_writes = self.simulation.last_writes().len();
                if self.selected_write >= num_writes {
                    self.selected_write = num_writes.saturating_sub(1);
                }
                self.status_message = format!(
                    "generation {} ({} product writes)",
                    self.simulation.generation(),
                    num_writes
                );
            }
            Err(error) => {
                self.running = false;
                self.status_message = format!("simulation halted: {}", error);
            }
        }
    }

    fn restart_simulation(&mut self) {
        match self.simulation.restart() {
            Ok(()) => {
                self.selected_write = 0;
                self.status_message = String::from("restarted");
            }
            Err(error) => {
                self.running = false;
                self.status_message = format!("failed to restart: {}", error);
            }
        }
    }

    fn move_cursor_horizontally(&mut self, offset: i64) {
        self.cursor_position = self.cursor_position.saturating_add(offset);
        self.scroll_to_cursor();
    }

    fn move_cursor_vertically(&mut self, offset: i64) {
        let tape_nos = self.simulation.get_tape_nos();
        if tape_nos.is_empty() {
            return;
        }
        let current_index = tape_nos
            .iter()
            .position(|tape_no| *tape_no == self.cursor_tape_no)
            .unwrap_or(0) as i64;
        let last_index = (tape_nos.len() - 1) as i64;
        let new_index = (current_index + offset).clamp(0, last_index) as usize;
        self.cursor_tape_no = tape_nos[new_index];
    }

    /// Scroll the view so that the selected cell stays visible.
    fn scroll_to_cursor(&mut self) {
        if self.visible_cells == 0 {
            self.view_left = self.cursor_position;
            return;
        }
        let view_right = self.view_left + self.visible_cells as i64 - 1;
        if self.cursor_position < self.view_left {
            self.view_left = self.cursor_position;
        } else if self.cursor_position > view_right {
            self.view_left = self.cursor_position - self.visible_cells as i64 + 1;
        }
    }

    fn center_on_cursor(&mut self) {
        if self.visible_cells == 0 {
            self.view_left = self.cursor_position;
            return;
        }
        self.view_left = self.cursor_position - (self.visible_cells as i64) / 2;
    }

    fn select_write(&mut self, offset: i64) {
        let num_writes = self.simulation.last_writes().len();
        if num_writes == 0 {
            self.selected_write = 0;
            return;
        }
        let num_writes = num_writes as i64;
        let current = self.selected_write as i64;
        let new_index = (current + offset).rem_euclid(num_writes);
        self.selected_write = new_index as usize;
    }

    /// Move the cell selection onto the cell written by the selected write.
    fn select_write_cell(&mut self) {
        let Some(write_record) = self
            .simulation
            .last_writes()
            .get(self.selected_write)
            .cloned()
        else {
            self.status_message = String::from("no product writes to select");
            return;
        };
        let (tape_no, position) = write_record.write_target;
        self.cursor_tape_no = tape_no;
        self.cursor_position = position;
        self.center_on_cursor();
        self.status_message = format!(
            "selected write {} -> (tape {}, position {})",
            write_record.annotation, tape_no, position
        );
    }

    fn change_speed(&mut self, faster: bool) {
        self.ticks_per_step = if faster {
            (self.ticks_per_step / 2).max(MIN_TICKS_PER_STEP)
        } else {
            (self.ticks_per_step * 2).min(MAX_TICKS_PER_STEP)
        };
        self.ticks_since_step = 0;
    }

    fn header_lines(&self) -> Vec<Line<'static>> {
        let play_state = if self.running { "playing" } else { "paused" };
        let cell_state = self
            .simulation
            .read_cell(self.cursor_tape_no, self.cursor_position);

        vec![
            Line::from(vec![
                Span::styled(
                    "Automata player",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    "  [{}]  generation {}  (1 step / {} ticks)",
                    play_state,
                    self.simulation.generation(),
                    self.ticks_per_step
                )),
            ]),
            Line::from(format!(
                "selected cell: tape {} position {} = {}   |   {}",
                self.cursor_tape_no, self.cursor_position, cell_state, self.status_message
            )),
        ]
    }

    /// Convert the text `RenderFrame` produced by the automata crate into
    /// styled lines, highlighting the currently selected cell.
    fn tape_lines(&mut self, width: usize) -> Vec<Line<'static>> {
        let sidebar_width = self.simulation.sidebar_width();
        let cell_width = self.simulation.cell_width();
        let content_width = width.saturating_sub(sidebar_width);
        self.visible_cells = content_width / (cell_width + 1);

        let render_frame = match self.simulation.render_tapes(self.view_left, width) {
            Ok(render_frame) => render_frame,
            Err(error) => {
                return vec![Line::from(format!("failed to render tapes: {}", error))];
            }
        };

        let tape_nos = self.simulation.get_tape_nos();
        let cursor_row = tape_nos
            .iter()
            .position(|tape_no| *tape_no == self.cursor_tape_no)
            // the first line of the render frame holds the position ruler
            .map(|index| index + 1);
        let cursor_offset = self.cursor_position - self.view_left;
        let cursor_visible = cursor_offset >= 0 && (cursor_offset as usize) < self.visible_cells;

        let mut lines: Vec<Line<'static>> = Vec::new();
        for (row, frame_line) in render_frame.get_lines().iter().enumerate() {
            let is_cursor_row = cursor_visible && cursor_row == Some(row);
            if !is_cursor_row {
                lines.push(Line::from(frame_line.clone()));
                continue;
            }

            let start = sidebar_width + (cursor_offset as usize) * (cell_width + 1);
            let end = start + cell_width;
            let chars: Vec<char> = frame_line.chars().collect();
            if end > chars.len() {
                lines.push(Line::from(frame_line.clone()));
                continue;
            }

            let prefix: String = chars[..start].iter().collect();
            let selected: String = chars[start..end].iter().collect();
            let suffix: String = chars[end..].iter().collect();
            lines.push(Line::from(vec![
                Span::raw(prefix),
                Span::styled(
                    selected,
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(suffix),
            ]));
        }
        lines
    }

    fn write_items(&self) -> Vec<ListItem<'static>> {
        let writes = self.simulation.last_writes();
        if writes.is_empty() {
            return vec![ListItem::new(
                "no product writes yet - press <space> to play or <n> to step",
            )];
        }

        writes
            .iter()
            .map(|write_record| ListItem::new(Line::from(Self::describe_write(write_record))))
            .collect()
    }

    fn describe_write(write_record: &WriteRecord) -> String {
        let (tape_no, position) = write_record.write_target;
        format!(
            "{:<18} (tape {}, position {}) <- {}   {}",
            write_record.annotation,
            tape_no,
            position,
            write_record.tape_cell_state,
            write_record.origin_product._to_string("D")
        )
    }

    fn footer_lines(&self) -> Vec<Line<'static>> {
        vec![
            Line::from(
                "<space> play/pause  <n> step  <r> restart  \
                 <arrows> move cell selection  <pgup>/<pgdn> move a screen  <c> center",
            ),
            Line::from(
                "<tab>/<shift-tab> select product write  <enter> go to its cell  \
                 <+>/<-> speed  <q> quit",
            ),
        ]
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
                if self.running {
                    self.ticks_since_step += 1;
                    if self.ticks_since_step >= self.ticks_per_step {
                        self.ticks_since_step = 0;
                        self.step_simulation();
                    }
                }
            }
            Action::TogglePlay => {
                self.running = !self.running;
                self.ticks_since_step = 0;
                self.status_message = if self.running {
                    String::from("playing")
                } else {
                    String::from("paused")
                };
            }
            Action::StepForward => {
                self.running = false;
                self.step_simulation();
            }
            Action::Restart => {
                self.running = false;
                self.restart_simulation();
            }
            Action::MoveCursorLeft => self.move_cursor_horizontally(-1),
            Action::MoveCursorRight => self.move_cursor_horizontally(1),
            Action::MoveCursorUp => self.move_cursor_vertically(-1),
            Action::MoveCursorDown => self.move_cursor_vertically(1),
            Action::PageCursorLeft => {
                let offset = (self.visible_cells.max(1)) as i64;
                self.move_cursor_horizontally(-offset);
            }
            Action::PageCursorRight => {
                let offset = (self.visible_cells.max(1)) as i64;
                self.move_cursor_horizontally(offset);
            }
            Action::CenterOnCursor => self.center_on_cursor(),
            Action::NextWrite => self.select_write(1),
            Action::PrevWrite => self.select_write(-1),
            Action::SelectWriteCell => self.select_write_cell(),
            Action::SpeedUp => self.change_speed(true),
            Action::SpeedDown => self.change_speed(false),
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let num_tape_rows = self.simulation.get_tape_nos().len() as u16 + 1;
        let [header_area, tapes_area, writes_area, footer_area] = Layout::vertical([
            Constraint::Length(2),
            // +2 for the block borders
            Constraint::Length(num_tape_rows + 2),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .areas(area);

        frame.render_widget(Paragraph::new(self.header_lines()), header_area);

        let tapes_block = Block::default().borders(Borders::ALL).title("tapes");
        let tapes_inner = tapes_block.inner(tapes_area);
        frame.render_widget(tapes_block, tapes_area);
        let tape_lines = self.tape_lines(tapes_inner.width as usize);
        frame.render_widget(Paragraph::new(tape_lines), tapes_inner);

        let writes_title = format!(
            "product writes of generation {}",
            self.simulation.generation()
        );
        let writes_block = Block::default().borders(Borders::ALL).title(writes_title);
        let has_writes = !self.simulation.last_writes().is_empty();
        let writes_list = List::new(self.write_items())
            .block(writes_block)
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
        self.writes_list_state
            .select(has_writes.then_some(self.selected_write));
        frame.render_stateful_widget(writes_list, writes_area, &mut self.writes_list_state);

        frame.render_widget(Paragraph::new(self.footer_lines()), footer_area);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn draw_home(home: &mut Home) -> String {
        let backend = TestBackend::new(90, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| home.draw(frame, frame.area()).unwrap())
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(90)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<String>>()
            .join("\n")
    }

    #[test]
    fn renders_tapes_and_selection() {
        let mut home = Home::new().unwrap();
        let rendered = draw_home(&mut home);
        assert!(rendered.contains("Automata player"));
        assert!(rendered.contains("[paused]"));
        assert!(rendered.contains("Tape 0:"));
        assert!(rendered.contains("selected cell: tape 0 position 0"));
    }

    #[test]
    fn stepping_records_product_writes() {
        let mut home = Home::new().unwrap();
        draw_home(&mut home);
        home.update(Action::StepForward).unwrap();
        assert!(!home.simulation.last_writes().is_empty());

        let rendered = draw_home(&mut home);
        assert!(rendered.contains("product writes of generation 1"));
    }

    #[test]
    fn play_pause_advances_the_simulation_on_ticks() {
        let mut home = Home::new().unwrap();
        home.update(Action::TogglePlay).unwrap();
        assert!(home.running);
        for _ in 0..home.ticks_per_step {
            home.update(Action::Tick).unwrap();
        }
        assert_eq!(home.simulation.generation(), 1);

        home.update(Action::TogglePlay).unwrap();
        assert!(!home.running);
        for _ in 0..(home.ticks_per_step * 2) {
            home.update(Action::Tick).unwrap();
        }
        assert_eq!(home.simulation.generation(), 1);
    }

    #[test]
    fn restart_resets_generation() {
        let mut home = Home::new().unwrap();
        home.update(Action::StepForward).unwrap();
        home.update(Action::StepForward).unwrap();
        assert_eq!(home.simulation.generation(), 2);
        home.update(Action::Restart).unwrap();
        assert_eq!(home.simulation.generation(), 0);
    }

    #[test]
    fn arrow_keys_move_the_cell_selection() {
        let mut home = Home::new().unwrap();
        draw_home(&mut home);
        home.update(Action::MoveCursorRight).unwrap();
        home.update(Action::MoveCursorRight).unwrap();
        assert_eq!(home.cursor_position, 2);
        home.update(Action::MoveCursorLeft).unwrap();
        assert_eq!(home.cursor_position, 1);

        home.update(Action::MoveCursorDown).unwrap();
        assert_eq!(home.cursor_tape_no, 0, "tape 1 only exists after a step");
        home.update(Action::StepForward).unwrap();
        home.update(Action::MoveCursorDown).unwrap();
        assert_eq!(home.cursor_tape_no, 1);
        home.update(Action::MoveCursorUp).unwrap();
        assert_eq!(home.cursor_tape_no, 0);
    }

    #[test]
    fn selecting_a_product_write_selects_its_cell() {
        let mut home = Home::new().unwrap();
        draw_home(&mut home);
        home.update(Action::StepForward).unwrap();

        let num_writes = home.simulation.last_writes().len();
        assert!(num_writes > 1);
        home.update(Action::NextWrite).unwrap();
        assert_eq!(home.selected_write, 1);
        home.update(Action::PrevWrite).unwrap();
        assert_eq!(home.selected_write, 0);
        // selection wraps around
        home.update(Action::PrevWrite).unwrap();
        assert_eq!(home.selected_write, num_writes - 1);

        let write_record = home.simulation.last_writes()[home.selected_write].clone();
        let (tape_no, position) = write_record.write_target;
        home.update(Action::SelectWriteCell).unwrap();
        assert_eq!(home.cursor_tape_no, tape_no);
        assert_eq!(home.cursor_position, position);
    }

    #[test]
    fn cursor_movement_scrolls_the_view() {
        let mut home = Home::new().unwrap();
        draw_home(&mut home);
        let visible_cells = home.visible_cells;
        assert!(visible_cells > 0);

        home.update(Action::PageCursorRight).unwrap();
        assert_eq!(home.cursor_position, visible_cells as i64);
        assert_eq!(home.view_left, 1);

        home.update(Action::CenterOnCursor).unwrap();
        assert_eq!(
            home.view_left,
            home.cursor_position - (visible_cells as i64) / 2
        );
    }

    #[test]
    fn speed_changes_are_clamped() {
        let mut home = Home::new().unwrap();
        for _ in 0..10 {
            home.update(Action::SpeedUp).unwrap();
        }
        assert_eq!(home.ticks_per_step, MIN_TICKS_PER_STEP);
        for _ in 0..10 {
            home.update(Action::SpeedDown).unwrap();
        }
        assert_eq!(home.ticks_per_step, MAX_TICKS_PER_STEP);
    }
}
