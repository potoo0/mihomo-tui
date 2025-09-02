use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

use super::{AppState, Component, ComponentId};
use crate::action::Action;
use crate::config::get_config_path;

const REPOSITORY_URL: &str = concat!(
    "https://github.com/potoo0/mihomo-tui/tree/v",
    env!("CARGO_PKG_VERSION")
);

pub struct HelpComponent {
    scroll: usize,
    viewport: usize,
    total: usize,
    scroll_state: ScrollbarState,
}

impl Default for HelpComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpComponent {
    pub fn new() -> Self {
        let total = Self::lines().0.len();
        let scroll_state = ScrollbarState::default().content_length(total);

        Self {
            scroll: 0,
            viewport: 0,
            total,
            scroll_state,
        }
    }

    fn lines<'a>() -> (Vec<Line<'a>>, Vec<Line<'a>>) {
        vec![
            (Line::raw(""), Line::raw("")),
            (Line::raw(""), Line::raw("")),
            (Line::raw(""), Line::raw("")),
            (Line::raw("Key").bold(), Line::raw("Description").bold()),
            (Line::raw("h"), Line::raw("Toggle help")),
            (Line::raw("q, ctrl + c"), Line::raw("Quits program")),
            (Line::raw(""), Line::raw("")),
            (
                Line::raw("Default configuration").bold(),
                Line::raw(format!("'{}'", get_config_path().display())),
            ),
            (Line::raw("Version").bold(), Line::raw(REPOSITORY_URL)),
        ]
        .into_iter()
        .unzip()
    }
}

impl Component for HelpComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Help
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(Action::Quit));
            }
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') => {
                return Ok(Some(Action::Unfocus));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1).min(self.total - 1);
                self.scroll_state = self.scroll_state.position(self.scroll);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                self.scroll_state = self.scroll_state.position(self.scroll);
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll = self
                    .scroll
                    .saturating_add(self.viewport)
                    .min(self.total - 1);
                self.scroll_state = self.scroll_state.position(self.scroll);
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(self.viewport);
                self.scroll_state = self.scroll_state.position(self.scroll);
            }
            _ => (),
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, _state: &AppState) -> Result<()> {
        let (left, right) = Self::lines();

        self.viewport = area.height as usize;

        let cols = Layout::horizontal([
            Constraint::Percentage(40),
            Constraint::Percentage(4),
            Constraint::Fill(1),
        ])
        .split(area);

        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(left)
                .scroll((self.scroll as u16, 0))
                .alignment(Alignment::Right),
            cols[0],
        );
        frame.render_widget(
            Paragraph::new(right)
                .scroll((self.scroll as u16, 0))
                .alignment(Alignment::Left),
            cols[2],
        );

        // scrollbar
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        frame.render_stateful_widget(scrollbar, area, &mut self.scroll_state);

        Ok(())
    }
}
