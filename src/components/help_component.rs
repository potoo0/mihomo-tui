use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{
    Block, BorderType, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use super::{Component, ComponentId};
use crate::action::Action;
use crate::config::get_config_path;
use crate::utils::symbols::arrow;

const REPOSITORY_URL: &str =
    concat!("https://github.com/potoo0/mihomo-tui/tree/v", env!("CARGO_PKG_VERSION"));

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

        Self { scroll: 0, viewport: 0, total, scroll_state }
    }

    fn lines<'a>() -> (Vec<Line<'a>>, Vec<Line<'a>>, Vec<Line<'a>>) {
        vec![
            (None, None, None),
            (None, None, None),
            (
                Line::raw("Default configuration").bold().into(),
                None,
                Line::raw(format!("'{}'", get_config_path().display())).into(),
            ),
            (Line::raw("Version").bold().into(), None, Line::raw(REPOSITORY_URL).into()),
            // >>> key bindings
            (None, None, None),
            (Line::raw("Key").bold().into(), None, Line::raw("Description").bold().into()),
            // common key bindings
            (
                Line::raw("---").into(),
                Line::raw("common").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (Line::raw("h").into(), None, Line::raw("Toggle help").into()),
            (Line::raw("q / Ctrl+c").into(), None, Line::raw("Quits program").into()),
            (Line::raw("Number").into(), None, Line::raw("switch to tab").into()),
            (
                Line::raw("k / Up, j / Down").into(),
                None,
                Line::raw("select in table or list").into(),
            ),
            (Line::raw("g, G").into(), None, Line::raw("go to first, last row").into()),
            // `filter` key bindings
            (
                Line::raw("---").into(),
                Line::raw("filter").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (Line::raw("f").into(), None, Line::raw("input mode").into()),
            (Line::raw("Esc, Enter").into(), None, Line::raw("exit input mode").into()),
            (
                Line::raw("Ctrl+Left, Ctrl+Right").into(),
                None,
                Line::raw("go to previous, next word").into(),
            ),
            (
                Line::raw("Ctrl+w / Alt+Backspace").into(),
                None,
                Line::raw("delete previous word").into(),
            ),
            (Line::raw("Home, End").into(), None, Line::raw("go to start, end").into()),
            // `connections` key bindings
            (
                Line::raw("---").into(),
                Line::raw("connections").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (Line::raw("Esc").into(), None, Line::raw("live mode").into()),
            (Line::raw("Enter").into(), None, Line::raw("toggle connection detail").into()),
            (Line::raw("t").into(), None, Line::raw("terminate connection").into()),
            (Line::raw("h / Left, l / Right").into(), None, Line::raw("select sort column").into()),
            (Line::raw("r").into(), None, Line::raw("reverse sort direction").into()),
            // `connection detail` key bindings
            (
                Line::raw("---").into(),
                Line::raw("detail").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (Line::raw("PageUp, Space / PageDown").into(), None, Line::raw("page up, down").into()),
            // `logs` key bindings
            (
                Line::raw("---").into(),
                Line::raw("logs").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (
                Line::raw("e, w, i, d").into(),
                None,
                Line::raw("filter log level: error, warn, info, debug").into(),
            ),
        ]
        .into_iter()
        .fold((Vec::new(), Vec::new(), Vec::new()), |mut acc, (l, c, r)| {
            acc.0.push(l.unwrap_or_else(|| Line::raw("")));
            acc.1.push(c.unwrap_or_else(|| Line::raw("")));
            acc.2.push(r.unwrap_or_else(|| Line::raw("")));
            acc
        })
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
                self.scroll = self.scroll.saturating_add(self.viewport).min(self.total - 1);
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

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let (left, center, right) = Self::lines();

        // border
        let border = Block::bordered().border_type(BorderType::Rounded);
        let inner = border.inner(area);
        frame.render_widget(border, area);

        // content
        self.viewport = inner.height as usize;
        let cols = Layout::horizontal([
            Constraint::Percentage(40),
            Constraint::Length(12),
            Constraint::Fill(1),
        ])
        .split(inner);

        frame.render_widget(Clear, inner);
        frame.render_widget(
            Paragraph::new(left).scroll((self.scroll as u16, 0)).alignment(Alignment::Right),
            cols[0],
        );
        frame.render_widget(
            Paragraph::new(center).scroll((self.scroll as u16, 0)).alignment(Alignment::Center),
            cols[1],
        );
        frame.render_widget(
            Paragraph::new(right).scroll((self.scroll as u16, 0)).alignment(Alignment::Left),
            cols[2],
        );

        // scrollbar
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some(arrow::UP))
            .end_symbol(Some(arrow::DOWN));
        frame.render_stateful_widget(scrollbar, inner, &mut self.scroll_state);

        Ok(())
    }
}
