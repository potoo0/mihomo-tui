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
            (Line::raw("k / Up, j / Down").into(), None, Line::raw("navigation").into()),
            (Line::raw("g, G").into(), None, Line::raw("go to first, last").into()),
            (Line::raw("PageUp, Space / PageDown").into(), None, Line::raw("page up, down").into()),
            (Line::raw("Esc").into(), None, Line::raw("cancel / back / live toggle").into()),
            (Line::raw("Enter").into(), None, Line::raw("confirm / open detail").into()),
            // search / proxy setting input keys
            (
                Line::raw("---").into(),
                Line::raw("input box").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (Line::raw("Shift+Tab, Tab").into(), None, Line::raw("navigate fields").into()),
            (
                Line::raw("Left, Right, Ctrl+Left, Ctrl+Right").into(),
                None,
                Line::raw("move cursor").into(),
            ),
            (Line::raw("Back, Ctrl+Back, Del, Ctrl-Del").into(), None, Line::raw("delete").into()),
            (Line::raw("Home, End").into(), None, Line::raw("jump to line start, end").into()),
            // `connections` key bindings
            (
                Line::raw("---").into(),
                Line::raw("# Connections (Conn)").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (Line::raw("Left, Right").into(), None, Line::raw("select sort column").into()),
            (Line::raw("t").into(), None, Line::raw("terminate connection").into()),
            (Line::raw("r").into(), None, Line::raw("reverse sort direction").into()),
            (Line::raw("c").into(), None, Line::raw("capture mode").into()),
            // proxies / proxy detail
            (
                Line::raw("---").into(),
                Line::raw("# Proxies (Pxy)").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (Line::raw("r").into(), None, Line::raw("refresh proxies").into()),
            (Line::raw("s").into(), None, Line::raw("open settings").into()),
            (Line::raw("t").into(), None, Line::raw("test proxy").into()),
            // proxy providers / proxy provider detail
            (
                Line::raw("---").into(),
                Line::raw("# ProxyProviders (Pxy-Pr)").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (Line::raw("Enter").into(), None, Line::raw("show provider detail").into()),
            (Line::raw("u").into(), None, Line::raw("update providers").into()),
            // `logs` key bindings
            (
                Line::raw("---").into(),
                Line::raw("# Logs (log)").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (
                Line::raw("e, w, i, d").into(),
                None,
                Line::raw("filter log level: error, warn, info, debug").into(),
            ),
            // `rule providers` key bindings
            (
                Line::raw("---").into(),
                Line::raw("# RuleProviders (rule)").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (Line::raw("r").into(), None, Line::raw("refresh rule providers").into()),
            (Line::raw("u").into(), None, Line::raw("update rule providers").into()),
            // `config` key bindings
            (
                Line::raw("---").into(),
                Line::raw("# Config (Cfg)").italic().bold().into(),
                Line::raw("---").into(),
            ),
            (
                Line::raw("Shift+Tab, Tab").into(),
                None,
                Line::raw("submit editor content or execute focused action").into(),
            ),
            (Line::raw("Enter").into(), None, Line::raw("execute focused action / confirm").into()),
            (
                Line::raw("e").into(),
                None,
                Line::raw("open config in external editor ($EDITOR → vim → vi)").into(),
            ),
            (Line::raw("d").into(), None, Line::raw("discard changes and reload config").into()),
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
        // todo: improve layout
        let cols = Layout::horizontal([
            Constraint::Percentage(35),
            Constraint::Fill(1),
            Constraint::Percentage(45),
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
