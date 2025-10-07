use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::{Component, ComponentId};
use crate::action::Action;
use crate::config::get_config_path;
use crate::widgets::scrollbar::Scroller;

const REPOSITORY_URL: &str =
    concat!("https://github.com/potoo0/mihomo-tui/tree/v", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Default)]
pub struct HelpComponent {
    scroller: Scroller,
}

impl HelpComponent {
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
            (Line::raw("PageUp, Space / PageDown").into(), None, Line::raw("page up, down").into()),
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
            (None, None, None),
            (None, None, None),
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
        if self.scroller.handle_key_event(key) {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(Action::Quit));
            }
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') => {
                return Ok(Some(Action::Unfocus));
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
        self.scroller.length(left.len(), inner.height as usize);
        let offset = (self.scroller.pos() as u16, 0u16);

        // content
        let cols = Layout::horizontal([
            Constraint::Percentage(40),
            Constraint::Length(12),
            Constraint::Fill(1),
        ])
        .split(inner);

        frame.render_widget(Clear, inner);
        frame.render_widget(
            Paragraph::new(left).scroll(offset).alignment(Alignment::Right),
            cols[0],
        );
        frame.render_widget(
            Paragraph::new(center).scroll(offset).alignment(Alignment::Center),
            cols[1],
        );
        frame.render_widget(
            Paragraph::new(right).scroll(offset).alignment(Alignment::Left),
            cols[2],
        );

        // scrollbar
        self.scroller.render(frame, area);

        Ok(())
    }
}
