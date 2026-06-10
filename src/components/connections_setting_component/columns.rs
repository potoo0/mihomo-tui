use const_format::concatcp;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use super::{Direction, SettingPane};
use crate::store::connections::{
    ALIVE_COLUMN_INDEX, CONNECTION_COLS, DEFAULT_CONNECTION_COL_INDICES,
};
use crate::utils::input::KeyOutcome;
use crate::utils::symbols::arrow;
use crate::widgets::shortcut::{Fragment, Shortcut};

const COLUMN_PAGE_STEP: usize = 4;

#[derive(Debug, Clone)]
struct ColumnSettingItem {
    original_index: usize,
    title: &'static str,
    selected: bool,
}

#[derive(Debug, Default)]
pub(super) struct ColumnsSettingPane {
    items: Vec<ColumnSettingItem>,
    selected: usize,
    error: Option<String>,
}

impl ColumnsSettingPane {
    pub(super) fn load(&mut self, selected_columns: &[usize]) {
        self.items = column_items_from_selected(selected_columns);
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        self.error = None;
    }

    pub(super) fn reset(&mut self) {
        self.items.clear();
        self.selected = 0;
        self.error = None;
    }

    pub(super) fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub(super) fn selected_column_indices(&self) -> Vec<usize> {
        self.items.iter().filter(|item| item.selected).map(|item| item.original_index).collect()
    }
}

impl SettingPane for ColumnsSettingPane {
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::hl(arrow::LEFT),
                Fragment::raw("/"),
                Fragment::hl("PgUp"),
                Fragment::raw("/"),
                Fragment::hl("g"),
                Fragment::raw(" nav "),
                Fragment::hl("G"),
                Fragment::raw("/"),
                Fragment::hl("PgDn"),
                Fragment::raw("/"),
                Fragment::hl(arrow::RIGHT),
            ]),
            Shortcut::new(vec![
                Fragment::hl(concatcp!("C-", arrow::LEFT)),
                Fragment::raw(" move "),
                Fragment::hl(concatcp!("C-", arrow::RIGHT)),
            ]),
            Shortcut::new(vec![Fragment::raw("toggle "), Fragment::hl("Space")]),
            Shortcut::from("all", 0).unwrap(),
            Shortcut::from("invert", 0).unwrap(),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> KeyOutcome {
        match key.code {
            KeyCode::Left if key.modifiers == KeyModifiers::CONTROL => {
                self.reorder_selected_column(Direction::Prev)
            }
            KeyCode::Right if key.modifiers == KeyModifiers::CONTROL => {
                self.reorder_selected_column(Direction::Next)
            }
            KeyCode::Left if key.modifiers == KeyModifiers::NONE => {
                self.move_selection(1, Direction::Prev)
            }
            KeyCode::Right if key.modifiers == KeyModifiers::NONE => {
                self.move_selection(1, Direction::Next)
            }
            KeyCode::PageUp => self.move_selection(COLUMN_PAGE_STEP, Direction::Prev),
            KeyCode::PageDown => self.move_selection(COLUMN_PAGE_STEP, Direction::Next),
            KeyCode::Char('g') | KeyCode::Home => self.jump_selection_to(0),
            KeyCode::Char('G') | KeyCode::End => {
                self.jump_selection_to(self.items.len().saturating_sub(1))
            }
            KeyCode::Char(' ') => self.toggle_selected_column(),
            KeyCode::Char('a') => self.select_all_columns(),
            KeyCode::Char('i') => self.invert_selected_columns(),
            _ => return KeyOutcome::Ignored,
        };

        KeyOutcome::Consumed
    }

    fn draw_content(&mut self, frame: &mut Frame, area: Rect, active: bool) {
        self.draw_columns(frame, area, active);
    }

    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn clear_error(&mut self) {
        self.error = None;
    }
}

impl ColumnsSettingPane {
    fn move_selection(&mut self, step: usize, direction: Direction) {
        let len = self.items.len();
        if len == 0 {
            self.selected = 0;
            return;
        }

        self.selected = match direction {
            Direction::Prev => self.selected.saturating_sub(step),
            Direction::Next => self.selected.saturating_add(step).min(len - 1),
        };
        self.clear_error();
    }

    fn jump_selection_to(&mut self, index: usize) {
        let len = self.items.len();
        if len == 0 {
            self.selected = 0;
            return;
        }

        self.selected = index.min(len - 1);
        self.clear_error();
    }

    fn toggle_selected_column(&mut self) {
        let selected_count = self.items.iter().filter(|item| item.selected).count();
        let Some(item) = self.items.get_mut(self.selected) else {
            return;
        };

        if item.selected && selected_count == 1 {
            self.set_error("At least one column must be selected");
            return;
        }

        item.selected = !item.selected;
        self.clear_error();
    }

    fn select_all_columns(&mut self) {
        for item in &mut self.items {
            item.selected = true;
        }
        self.clear_error();
    }

    fn invert_selected_columns(&mut self) {
        for item in &mut self.items {
            item.selected = !item.selected;
        }
        if let Some(item) = self.items.get_mut(self.selected) {
            item.selected = true;
        }
        self.clear_error();
    }

    fn reorder_selected_column(&mut self, direction: Direction) {
        if self.items.is_empty() {
            self.selected = 0;
            return;
        }

        let next = match direction {
            Direction::Prev => self.selected.saturating_sub(1),
            Direction::Next => self.selected.saturating_add(1).min(self.items.len() - 1),
        };
        self.items.swap(self.selected, next);
        self.selected = next;
        self.clear_error();
    }
}

impl ColumnsSettingPane {
    fn draw_columns(&self, frame: &mut Frame, area: Rect, active: bool) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(if active { Color::Cyan } else { Color::DarkGray })
            .title(" Columns ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut tokens = Vec::with_capacity(self.items.len() * 2);
        let last_index = self.items.len().saturating_sub(1);
        for (index, item) in self.items.iter().enumerate() {
            let selected = self.selected == index && active;
            tokens.push(Span::styled(item.title, self.token_style(item.selected, selected)));
            if index != last_index {
                tokens.push(Span::raw(" "));
            }
        }
        frame.render_widget(Paragraph::new(Line::from(tokens)).wrap(Wrap { trim: false }), inner);
    }

    fn token_style(&self, enabled: bool, selected: bool) -> Style {
        let style = if enabled {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        if selected { style.fg(Color::Cyan).add_modifier(Modifier::REVERSED) } else { style }
    }
}

fn column_items_from_selected(selected_columns: &[usize]) -> Vec<ColumnSettingItem> {
    let selected_columns =
        if selected_columns.is_empty() { DEFAULT_CONNECTION_COL_INDICES } else { selected_columns };
    let mut items = Vec::with_capacity(CONNECTION_COLS.len());

    for &original_index in selected_columns {
        if original_index == ALIVE_COLUMN_INDEX {
            continue;
        }
        if let Some(def) = CONNECTION_COLS.get(original_index) {
            items.push(ColumnSettingItem { original_index, title: def.col.title, selected: true });
        }
    }

    for (original_index, def) in CONNECTION_COLS.iter().enumerate() {
        if original_index == ALIVE_COLUMN_INDEX || selected_columns.contains(&original_index) {
            continue;
        }
        items.push(ColumnSettingItem { original_index, title: def.col.title, selected: false });
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_load_keeps_selected_order_and_appends_unselected() {
        let mut pane = ColumnsSettingPane::default();

        pane.load(&[2, 1, ALIVE_COLUMN_INDEX]);

        assert_eq!(pane.items[0].original_index, 2);
        assert_eq!(pane.items[1].original_index, 1);
        assert!(pane.items[0].selected);
        assert!(pane.items[1].selected);
        assert_eq!(pane.items.len(), CONNECTION_COLS.len() - 1);
        assert!(!pane.items.iter().any(|item| item.original_index == ALIVE_COLUMN_INDEX));
    }

    #[test]
    fn columns_cannot_toggle_last_selected_column() {
        let mut pane = ColumnsSettingPane::default();
        pane.load(&[1]);
        for item in pane.items.iter_mut().skip(1) {
            item.selected = false;
        }

        pane.toggle_selected_column();

        assert_eq!(pane.error(), Some("At least one column must be selected"));
        assert!(pane.items[0].selected);
    }

    #[test]
    fn columns_reorder_changes_selected_column_order() {
        let mut pane = ColumnsSettingPane::default();
        pane.load(&[1, 2, 3]);

        pane.reorder_selected_column(Direction::Next);

        assert_eq!(pane.selected_column_indices()[..3], [2, 1, 3]);
        assert_eq!(pane.selected, 1);
    }

    #[test]
    fn columns_reorder_uses_ctrl_arrow_shortcut() {
        let mut pane = ColumnsSettingPane::default();
        pane.load(&[1, 2, 3]);

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT)),
            KeyOutcome::Ignored
        );
        assert_eq!(pane.selected_column_indices()[..3], [1, 2, 3]);

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected_column_indices()[..3], [2, 1, 3]);
        assert_eq!(pane.selected, 1);
    }

    #[test]
    fn columns_g_and_shift_g_jump_to_first_and_last_column() {
        let mut pane = ColumnsSettingPane::default();
        pane.load(&[1, 2, 3]);
        pane.selected = 2;

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected, 0);

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected, pane.items.len() - 1);
    }

    #[test]
    fn columns_select_all_marks_every_column_selected() {
        let mut pane = ColumnsSettingPane::default();
        pane.load(&[1, 2, 3]);
        for item in pane.items.iter_mut().skip(1) {
            item.selected = false;
        }

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            KeyOutcome::Consumed
        );

        assert!(pane.items.iter().all(|item| item.selected));
        assert_eq!(pane.error(), None);
    }

    #[test]
    fn columns_invert_keeps_current_column_selected() {
        let mut pane = ColumnsSettingPane::default();
        pane.load(&[1, 2, 3]);
        pane.selected = 1;

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)),
            KeyOutcome::Consumed
        );

        assert!(!pane.items[0].selected);
        assert!(pane.items[1].selected);
        assert!(!pane.items[2].selected);
        assert_eq!(pane.error(), None);
    }

    #[test]
    fn columns_page_up_and_page_down_move_by_four_with_bounds() {
        let mut pane = ColumnsSettingPane::default();
        pane.load(&[1, 2, 3]);

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected, COLUMN_PAGE_STEP.min(pane.items.len() - 1));

        pane.selected = pane.items.len().saturating_sub(2);
        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected, pane.items.len() - 1);

        pane.selected = COLUMN_PAGE_STEP + 1;
        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected, 1);

        pane.selected = 1;
        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected, 0);
    }

    #[test]
    fn columns_fast_navigation_accepts_modified_keys() {
        let mut pane = ColumnsSettingPane::default();
        pane.load(&[1, 2, 3]);

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected, 0);

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::CONTROL)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected, pane.items.len() - 1);

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::PageUp, KeyModifiers::SHIFT)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected, (pane.items.len() - 1).saturating_sub(COLUMN_PAGE_STEP));

        assert_eq!(
            pane.handle_key_event(KeyEvent::new(KeyCode::PageDown, KeyModifiers::SHIFT)),
            KeyOutcome::Consumed
        );
        assert_eq!(pane.selected, pane.items.len() - 1);
    }
}
