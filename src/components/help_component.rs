use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::prelude::{Color, Style};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::{Component, ComponentId};
use crate::action::Action;
use crate::config::get_config_path;
use crate::config::runtime::runtime_path_for;
use crate::utils::text_ui::{popup_area, top_title_line};
use crate::widgets::scrollbar::Scroller;
use crate::widgets::shortcut::{DEFAULT_HL_COLOR, Fragment, Shortcut};

const REPOSITORY_URL: &str =
    concat!(env!("CARGO_PKG_REPOSITORY"), "/tree/v", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Default)]
pub struct HelpComponent {
    scroller: Scroller,
    horizontal_offset: u16,
    max_horizontal_offset: u16,
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

    fn key_entry(key: &'a str, description: &'a str) -> Self {
        Self::entry(Span::styled(key, Style::default().fg(DEFAULT_HL_COLOR)), description)
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
            HelpRow::key_entry("?", "Open help from the main view / close help"),
            HelpRow::key_entry("q", "Quit from the main view / close help"),
            HelpRow::key_entry("Ctrl+c", "Quit program"),
            HelpRow::key_entry("Number", "switch to tab"),
            HelpRow::key_entry("k / Up, j / Down", "vertical navigation where available"),
            HelpRow::key_entry("h / Left, l / Right", "horizontal navigation where available"),
            HelpRow::key_entry("g, G", "go to first, last"),
            HelpRow::key_entry("PageUp, Space / PageDown", "page up, down"),
            HelpRow::key_entry("Esc", "cancel / back / live toggle"),
            HelpRow::key_entry("Enter", "confirm / open detail"),
            HelpRow::key_entry("Ctrl+l", "clear idle tabs"),
            HelpRow::key_entry("Ctrl+u", "open updates"),
            // filter / proxy setting input keys
            HelpRow::Empty,
            HelpRow::key_title("input box"),
            HelpRow::key_entry("Shift+Tab, Tab", "navigate fields"),
            HelpRow::key_entry("Left, Right, Ctrl+Left, Ctrl+Right", "move cursor"),
            HelpRow::key_entry("Back, Ctrl+Back, Del, Ctrl-Del", "delete"),
            HelpRow::key_entry("Ctrl+y", "yank last deleted word"),
            HelpRow::key_entry("Home, End", "jump to line start, end"),
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
            HelpRow::key_entry("Left, Right", "select sort column"),
            HelpRow::key_entry("t", "terminate selected connection"),
            HelpRow::key_entry("T", "terminate filtered connections"),
            HelpRow::key_entry("r", "reverse sort direction"),
            HelpRow::key_entry("c", "capture mode"),
            HelpRow::key_entry("s", "open connection settings"),
            HelpRow::key_entry("-, +", "decrease/increase sort column width"),
            HelpRow::key_entry("Delete", "reset sort column width"),
            // connections settings
            HelpRow::Empty,
            HelpRow::key_title("## Connections Settings"),
            HelpRow::key_entry("Shift+Tab, Tab", "navigate settings panes"),
            HelpRow::key_entry("Enter", "apply settings"),
            HelpRow::key_entry("Esc", "cancel settings"),
            HelpRow::key_entry("Left, Right", "Columns:          navigate columns"),
            HelpRow::key_entry("Ctrl+Left, Ctrl+Right", "Columns:          move selected column"),
            HelpRow::key_entry("Space", "Columns:          toggle selected column"),
            HelpRow::key_entry("a", "Columns:          select all columns"),
            HelpRow::key_entry("i", "Columns:          invert selected columns"),
            HelpRow::key_entry("Up, Down", "Source IP Alias:  navigate source IP aliases"),
            // proxies / proxy detail
            HelpRow::Empty,
            HelpRow::key_title("# Proxies (Pxy)"),
            HelpRow::key_entry("r", "refresh proxies"),
            HelpRow::key_entry("s", "open proxy settings"),
            HelpRow::key_entry("t", "test proxy"),
            // proxy detail
            HelpRow::Empty,
            HelpRow::key_title("## Proxy Detail"),
            HelpRow::key_entry("Enter", "update selected proxy"),
            HelpRow::key_entry("c", "jump to current selected proxy"),
            HelpRow::key_entry("[, ]", "navigate nested groups"),
            HelpRow::key_entry("s", "switch sort by: none, latency, name"),
            HelpRow::key_entry("S", "toggle sort direction"),
            // proxy providers / proxy provider detail
            HelpRow::Empty,
            HelpRow::key_title("# ProxyProviders (Pxy-Pr)"),
            HelpRow::key_entry("Enter", "show provider detail"),
            HelpRow::key_entry("u", "update providers"),
            // `logs` key bindings
            HelpRow::Empty,
            HelpRow::key_title("# Logs (Log)"),
            HelpRow::key_entry("e, w, i, d", "filter log level: error, warn, info, debug"),
            // `rules` key bindings
            HelpRow::Empty,
            HelpRow::key_title("# Rules (Rule)"),
            HelpRow::key_entry("r", "refresh rules"),
            HelpRow::key_entry("t", "toggle disabled state (selected or all filtered)"),
            HelpRow::key_entry("s", "submit disabled state changes"),
            // `rule providers` key bindings
            HelpRow::Empty,
            HelpRow::key_title("# RuleProviders (R-Pr)"),
            HelpRow::key_entry("r", "refresh rule providers"),
            HelpRow::key_entry("u", "update rule providers"),
            // `config` key bindings
            HelpRow::Empty,
            HelpRow::key_title("# Config (Cfg)"),
            HelpRow::key_entry("Shift+Tab, Tab", "move focus between editor and actions"),
            HelpRow::key_entry("Enter", "execute focused action / confirm"),
            HelpRow::key_entry("e", "open config in external editor ($EDITOR → vim → vi)"),
            HelpRow::key_entry("d", "discard changes and reload config"),
            HelpRow::key_entry("n", "open DNS query dialog"),
            // dns query dialog
            HelpRow::Empty,
            HelpRow::key_title("## DNS Query"),
            HelpRow::key_entry("Shift+Tab, Tab", "navigate query fields"),
            HelpRow::key_entry("Enter", "query DNS records"),
            HelpRow::key_entry("Left, Right", "select DNS record type"),
            HelpRow::key_entry("k / Up, j / Down", "scroll answers"),
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

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::hl("?"),
                Fragment::raw("/"),
                Fragment::hl("q"),
                Fragment::raw("/"),
                Fragment::hl("Esc"),
            ]),
            Shortcut::new(vec![
                Fragment::hl("←"),
                Fragment::raw("/"),
                Fragment::hl("↑"),
                Fragment::raw("/"),
                Fragment::hl("PgUp"),
                Fragment::raw("/"),
                Fragment::hl("g"),
                Fragment::raw(" nav "),
                Fragment::hl("G"),
                Fragment::raw("/"),
                Fragment::hl("PgDn"),
                Fragment::raw("/"),
                Fragment::hl("↓"),
                Fragment::raw("/"),
                Fragment::hl("→"),
            ]),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.scroller.handle_key_event(key).is_consumed() {
            return Ok(None);
        }
        match key.code {
            KeyCode::Left => self.horizontal_offset = self.horizontal_offset.saturating_sub(1),
            KeyCode::Right => {
                self.horizontal_offset =
                    self.horizontal_offset.saturating_add(1).min(self.max_horizontal_offset);
            }
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') => {
                return Ok(Some(Action::Unfocus));
            }
            _ => (),
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let area = popup_area(area, 90, 90);
        frame.render_widget(Clear, area); // clears out the background
        // outer margin
        let area = area.inner(Margin::new(2, 1));

        // border
        let border = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("help", Style::default()));
        let inner = border.inner(area);
        frame.render_widget(border, area);

        // content
        let gap = 4; // gap between key and description
        let center_x = (inner.width as f32 * 0.35) as u16;
        let lines = Self::lines(gap, center_x);

        self.scroller.length(lines.len(), inner.height as usize);
        self.max_horizontal_offset = lines
            .iter()
            .map(Line::width)
            .max()
            .unwrap_or(0)
            .saturating_sub(inner.width as usize)
            .min(u16::MAX as usize) as u16;
        self.horizontal_offset = self.horizontal_offset.min(self.max_horizontal_offset);
        let offset = (self.scroller.pos() as u16, self.horizontal_offset);
        frame.render_widget(Paragraph::new(lines).scroll(offset), inner);

        // scrollbar
        self.scroller.render(frame, area);

        Ok(())
    }
}
