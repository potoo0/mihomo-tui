use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::{Component, ComponentId};
use crate::action::Action;
use crate::config::get_config_path;
use crate::config::runtime::runtime_path_for;
use crate::widgets::scrollbar::Scroller;

const REPOSITORY_URL: &str =
    concat!(env!("CARGO_PKG_REPOSITORY"), "/tree/v", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Default)]
pub struct HelpComponent {
    scroller: Scroller,
}

enum HelpRow<'a> {
    Empty,
    Title(Line<'a>),
    Entry { left: Span<'a>, right: Span<'a> },
}

impl<'a> HelpRow<'a> {
    fn key_title(s: impl Into<Span<'a>>) -> Self {
        Self::Title(Line::from(vec!["--- ".into(), s.into().italic().bold(), " ---".into()]))
    }

    fn entry(left: impl Into<Span<'a>>, right: impl Into<Span<'a>>) -> Self {
        Self::Entry { left: left.into(), right: right.into() }
    }
}

impl HelpComponent {
    fn rows<'a>() -> Vec<HelpRow<'a>> {
        let config_path = get_config_path();
        let runtime_path = runtime_path_for(&config_path);

        vec![
            HelpRow::Empty,
            HelpRow::Empty,
            HelpRow::entry(
                Span::raw("Default configuration").bold(),
                format!("'{}'", config_path.display()),
            ),
            HelpRow::entry(
                Span::raw("Runtime configuration").bold(),
                format!("'{}'", runtime_path.display()),
            ),
            HelpRow::entry(Span::raw("Version").bold(), REPOSITORY_URL),
            // >>> key bindings
            HelpRow::Empty,
            HelpRow::entry(Span::raw("Key").bold(), Span::raw("Description").bold()),
            // common key bindings
            HelpRow::key_title("common"),
            HelpRow::entry("h", "Toggle help"),
            HelpRow::entry("q / Ctrl+c", "Quits program"),
            HelpRow::entry("Number", "switch to tab"),
            HelpRow::entry("k / Up, j / Down", "navigation"),
            HelpRow::entry("g, G", "go to first, last"),
            HelpRow::entry("PageUp, Space / PageDown", "page up, down"),
            HelpRow::entry("Esc", "cancel / back / live toggle"),
            HelpRow::entry("Enter", "confirm / open detail"),
            HelpRow::entry("Ctrl+l", "clear idle tabs"),
            HelpRow::entry("Ctrl+u", "open updates"),
            // filter / proxy setting input keys
            HelpRow::Empty,
            HelpRow::key_title("input box"),
            HelpRow::entry("Shift+Tab, Tab", "navigate fields"),
            HelpRow::entry("Left, Right, Ctrl+Left, Ctrl+Right", "move cursor"),
            HelpRow::entry("Back, Ctrl+Back, Del, Ctrl-Del", "delete"),
            HelpRow::entry("Ctrl+y", "yank last deleted word"),
            HelpRow::entry("Home, End", "jump to line start, end"),
            // filter syntax
            HelpRow::Empty,
            HelpRow::key_title("filter syntax"),
            HelpRow::entry("str", "match using fuzzy search for 'str'"),
            HelpRow::entry("^str", "match if the value starts with 'str'"),
            HelpRow::entry("str$", "match if the value ends with 'str'"),
            HelpRow::entry("^str$", "match exactly 'str'"),
            HelpRow::entry("'str", "match if the value contains substring 'str'"),
            HelpRow::entry("!<pattern>", "negate the match of <pattern>, examples: !^str, !'str"),
            HelpRow::entry("\"com:443\"", "quote plain patterns containing spaces or colons"),
            HelpRow::entry(
                "field:pattern",
                "match <pattern> only in the named column; field is case-insensitive",
            ),
            HelpRow::entry("field:\"two words\"", "quote values containing spaces"),
            HelpRow::entry(
                "field1:pat1 field2:pat2 pat3",
                "match named fields and remaining columns using AND",
            ),
            // `connections` key bindings
            HelpRow::Empty,
            HelpRow::key_title("# Connections (Conn)"),
            HelpRow::entry("Left, Right", "select sort column"),
            HelpRow::entry("t", "terminate selected connection"),
            HelpRow::entry("T", "terminate filtered connections"),
            HelpRow::entry("r", "reverse sort direction"),
            HelpRow::entry("c", "capture mode"),
            HelpRow::entry("s", "open connection settings"),
            // connections settings
            HelpRow::Empty,
            HelpRow::key_title("## Connections Settings"),
            HelpRow::entry("Shift+Tab, Tab", "navigate settings panes"),
            HelpRow::entry("Enter", "apply settings"),
            HelpRow::entry("Esc", "cancel settings"),
            HelpRow::entry("Left, Right", "Columns:          navigate columns"),
            HelpRow::entry("Ctrl+Left, Ctrl+Right", "Columns:          move selected column"),
            HelpRow::entry("Space", "Columns:          toggle selected column"),
            HelpRow::entry("a", "Columns:          select all columns"),
            HelpRow::entry("i", "Columns:          invert selected columns"),
            HelpRow::entry("Up, Down", "Source IP Alias:  navigate source IP aliases"),
            // proxies / proxy detail
            HelpRow::Empty,
            HelpRow::key_title("# Proxies (Pxy)"),
            HelpRow::entry("r", "refresh proxies"),
            HelpRow::entry("s", "open proxy settings"),
            HelpRow::entry("t", "test proxy"),
            // proxy detail
            HelpRow::Empty,
            HelpRow::key_title("## Proxy Detail"),
            HelpRow::entry("Enter", "update selected proxy"),
            HelpRow::entry("c", "jump to current selected proxy"),
            HelpRow::entry("[, ]", "navigate nested groups"),
            HelpRow::entry("s", "switch sort by: none, latency, name"),
            HelpRow::entry("S", "toggle sort direction"),
            // proxy providers / proxy provider detail
            HelpRow::Empty,
            HelpRow::key_title("# ProxyProviders (Pxy-Pr)"),
            HelpRow::entry("Enter", "show provider detail"),
            HelpRow::entry("u", "update providers"),
            // `logs` key bindings
            HelpRow::Empty,
            HelpRow::key_title("# Logs (Log)"),
            HelpRow::entry("e, w, i, d", "filter log level: error, warn, info, debug"),
            // `rules` key bindings
            HelpRow::Empty,
            HelpRow::key_title("# Rules (Rule)"),
            HelpRow::entry("r", "refresh rules"),
            HelpRow::entry("t", "toggle disabled state (selected or all filtered)"),
            HelpRow::entry("s", "submit disabled state changes"),
            // `rule providers` key bindings
            HelpRow::Empty,
            HelpRow::key_title("# RuleProviders (R-Pr)"),
            HelpRow::entry("r", "refresh rule providers"),
            HelpRow::entry("u", "update rule providers"),
            // `config` key bindings
            HelpRow::Empty,
            HelpRow::key_title("# Config (Cfg)"),
            HelpRow::entry("Shift+Tab, Tab", "move focus between editor and actions"),
            HelpRow::entry("Enter", "execute focused action / confirm"),
            HelpRow::entry("e", "open config in external editor ($EDITOR → vim → vi)"),
            HelpRow::entry("d", "discard changes and reload config"),
            HelpRow::entry("n", "open DNS query dialog"),
            // dns query dialog
            HelpRow::Empty,
            HelpRow::key_title("## DNS Query"),
            HelpRow::entry("Shift+Tab, Tab", "navigate query fields"),
            HelpRow::entry("Enter", "query DNS records"),
            HelpRow::entry("Left, Right", "select DNS record type"),
            HelpRow::entry("k / Up, j / Down", "scroll answers"),
            HelpRow::Empty,
            HelpRow::Empty,
        ]
    }

    fn lines<'a>(gap: u16, center: u16) -> Vec<Line<'a>> {
        Self::rows()
            .into_iter()
            .map(|row| match row {
                HelpRow::Empty => Line::raw(""),
                HelpRow::Title(title) => {
                    let title_len = title.width() as u16;
                    // Center title around our weighted axis (center)
                    let pad_left = center.saturating_sub(title_len / 2);
                    let mut spans = vec![" ".repeat(pad_left as usize).into()];
                    spans.extend(title.spans);
                    Line::from(spans)
                }
                HelpRow::Entry { left, right } => {
                    let left_len = left.width() as u16;

                    // Pad left to align right-edge to center
                    let pad_left = center.saturating_sub(left_len).saturating_sub(gap / 2);
                    let spans = vec![
                        " ".repeat(pad_left as usize).into(),
                        left,
                        " ".repeat(gap as usize).into(),
                        right,
                    ];
                    Line::from(spans)
                }
            })
            .collect()
    }
}

impl Component for HelpComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Help
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.scroller.handle_key_event(key).is_consumed() {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('h') => {
                return Ok(Some(Action::Unfocus));
            }
            _ => (),
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        frame.render_widget(Clear, area);

        // border
        let border = Block::bordered().border_type(BorderType::Rounded);
        let inner = border.inner(area);
        frame.render_widget(border, area);

        // content
        let gap = 4; // gap between key and description
        let center_x = (inner.width as f32 * 0.35) as u16;
        let lines = Self::lines(gap, center_x);

        self.scroller.length(lines.len(), inner.height as usize);
        let offset = (self.scroller.pos() as u16, 0u16);
        frame.render_widget(Paragraph::new(lines).scroll(offset), inner);

        // scrollbar
        self.scroller.render(frame, area);

        Ok(())
    }
}
