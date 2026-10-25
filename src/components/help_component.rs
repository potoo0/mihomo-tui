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
use crate::utils::symbols::arrow;
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
        Self::Title(Line::from(vec![
            "--- ".into(),
            localized_span(s.into()).italic().bold(),
            " ---".into(),
        ]))
    }

    fn entry(left: impl Into<Span<'a>>, right: impl Into<Span<'a>>) -> Self {
        Self::Entry { left: localized_span(left.into()), right: localized_span(right.into()) }
    }

    fn key_entry(key: &'a str, description: &'a str) -> Self {
        Self::entry(Span::styled(key, Style::default().fg(DEFAULT_HL_COLOR)), description)
    }
}

fn localized_span(span: Span<'_>) -> Span<'_> {
    let content = rust_i18n::t!(span.content.as_ref()).into_owned();
    Span::styled(content, span.style)
}

impl HelpComponent {
    fn rows<'a>() -> Vec<HelpRow<'a>> {
        let config_path = get_config_path();
        let runtime_path = runtime_path_for(&config_path);

        vec![
            HelpRow::Empty,
            HelpRow::Empty,
            HelpRow::entry(
                Span::raw("help.default_configuration").bold(),
                format!("'{}'", config_path.display()),
            ),
            HelpRow::entry(
                Span::raw("help.runtime_configuration").bold(),
                format!("'{}'", runtime_path.display()),
            ),
            HelpRow::entry(Span::raw("help.version").bold(), REPOSITORY_URL),
            // >>> key bindings
            HelpRow::Empty,
            HelpRow::entry(Span::raw("help.key").bold(), Span::raw("help.description").bold()),
            // common key bindings
            HelpRow::key_title("help.section.common"),
            HelpRow::key_entry("?", "help.common.open_help"),
            HelpRow::key_entry("q", "help.common.quit_or_close_help"),
            HelpRow::key_entry("Ctrl+c", "help.common.quit_program"),
            HelpRow::key_entry("Number", "help.common.switch_tab"),
            HelpRow::key_entry("k / Up, j / Down", "help.common.vertical_navigation"),
            HelpRow::key_entry("h / Left, l / Right", "help.common.horizontal_navigation"),
            HelpRow::key_entry("g, G", "help.common.first_or_last"),
            HelpRow::key_entry("PageUp, Space / PageDown", "help.common.page_navigation"),
            HelpRow::key_entry("Esc", "help.common.cancel_back_or_live"),
            HelpRow::key_entry("Enter", "help.common.confirm_or_detail"),
            HelpRow::key_entry("Ctrl+l", "help.common.clear_idle_tabs"),
            HelpRow::key_entry("Ctrl+u", "help.common.open_updates"),
            HelpRow::key_entry("L", "help.common.open_language"),
            // filter / proxy setting input keys
            HelpRow::Empty,
            HelpRow::key_title("help.section.input_box"),
            HelpRow::key_entry("Shift+Tab, Tab", "help.input.navigate_fields"),
            HelpRow::key_entry("Left, Right, Ctrl+Left, Ctrl+Right", "help.input.move_cursor"),
            HelpRow::key_entry("Back, Ctrl+Back, Del, Ctrl-Del", "help.input.delete"),
            HelpRow::key_entry("Ctrl+y", "help.input.yank_deleted_word"),
            HelpRow::key_entry("Home, End", "help.input.line_edges"),
            // filter syntax
            HelpRow::Empty,
            HelpRow::key_title("help.section.filter_syntax"),
            HelpRow::entry("str", "help.filter.fuzzy"),
            HelpRow::entry("^str", "help.filter.starts_with"),
            HelpRow::entry("str$", "help.filter.ends_with"),
            HelpRow::entry("^str$", "help.filter.exact"),
            HelpRow::entry("'str", "help.filter.contains"),
            HelpRow::entry("!<pattern>", "help.filter.negate"),
            HelpRow::entry("\"com:443\"", "help.filter.quote_pattern"),
            HelpRow::entry("field:pattern", "help.filter.named_column"),
            HelpRow::entry("field:\"two words\"", "help.filter.quote_value"),
            HelpRow::entry("field1:pat1 field2:pat2 pat3", "help.filter.combine_fields"),
            // `connections` key bindings
            HelpRow::Empty,
            HelpRow::key_title("help.section.connections"),
            HelpRow::key_entry("Left, Right", "help.connections.sort_column"),
            HelpRow::key_entry("t", "help.connections.terminate_selected"),
            HelpRow::key_entry("T", "help.connections.terminate_filtered"),
            HelpRow::key_entry("r", "help.connections.reverse_sort"),
            HelpRow::key_entry("c", "help.connections.capture"),
            HelpRow::key_entry("s", "help.connections.open_settings"),
            HelpRow::key_entry("-, +", "help.connections.resize_sort_column"),
            HelpRow::key_entry("Delete", "help.connections.reset_sort_width"),
            // connections settings
            HelpRow::Empty,
            HelpRow::key_title("help.section.connection_settings"),
            HelpRow::key_entry("Shift+Tab, Tab", "help.connection_settings.navigate_panes"),
            HelpRow::key_entry("Enter", "help.connection_settings.apply"),
            HelpRow::key_entry("Esc", "help.connection_settings.cancel"),
            HelpRow::key_entry("Left, Right", "help.connection_settings.navigate_columns"),
            HelpRow::key_entry("Ctrl+Left, Ctrl+Right", "help.connection_settings.move_column"),
            HelpRow::key_entry("Space", "help.connection_settings.toggle_column"),
            HelpRow::key_entry("a", "help.connection_settings.select_all_columns"),
            HelpRow::key_entry("i", "help.connection_settings.invert_columns"),
            HelpRow::key_entry("Up, Down", "help.connection_settings.navigate_aliases"),
            // proxies / proxy detail
            HelpRow::Empty,
            HelpRow::key_title("help.section.proxies"),
            HelpRow::key_entry("r", "help.proxies.refresh"),
            HelpRow::key_entry("s", "help.proxies.open_settings"),
            HelpRow::key_entry("t", "help.proxies.test"),
            // proxy detail
            HelpRow::Empty,
            HelpRow::key_title("help.section.proxy_detail"),
            HelpRow::key_entry("Enter", "help.proxy_detail.select"),
            HelpRow::key_entry("-, +", "help.proxy_detail.resize_card"),
            HelpRow::key_entry("c", "help.proxy_detail.jump_to_current"),
            HelpRow::key_entry("[, ]", "help.proxy_detail.navigate_groups"),
            HelpRow::key_entry("s", "help.proxy_detail.change_sort"),
            HelpRow::key_entry("S", "help.proxy_detail.reverse_sort"),
            // proxy providers / proxy provider detail
            HelpRow::Empty,
            HelpRow::key_title("help.section.proxy_providers"),
            HelpRow::key_entry("Enter", "help.proxy_providers.show_detail"),
            HelpRow::key_entry("u", "help.proxy_providers.update"),
            // `logs` key bindings
            HelpRow::Empty,
            HelpRow::key_title("help.section.logs"),
            HelpRow::key_entry("e, w, i, d", "help.logs.filter_level"),
            // `rules` key bindings
            HelpRow::Empty,
            HelpRow::key_title("help.section.rules"),
            HelpRow::key_entry("r", "help.rules.refresh"),
            HelpRow::key_entry("t", "help.rules.toggle_disabled"),
            HelpRow::key_entry("s", "help.rules.submit_disabled"),
            // `rule providers` key bindings
            HelpRow::Empty,
            HelpRow::key_title("help.section.rule_providers"),
            HelpRow::key_entry("r", "help.rule_providers.refresh"),
            HelpRow::key_entry("u", "help.rule_providers.update"),
            // `config` key bindings
            HelpRow::Empty,
            HelpRow::key_title("help.section.config"),
            HelpRow::key_entry("Shift+Tab, Tab", "help.config.switch_focus"),
            HelpRow::key_entry("Enter", "help.config.execute"),
            HelpRow::key_entry("e", "help.config.open_external_editor"),
            HelpRow::key_entry("d", "help.config.discard_changes"),
            HelpRow::key_entry("n", "help.config.open_dns_query"),
            // dns query dialog
            HelpRow::Empty,
            HelpRow::key_title("help.section.dns_query"),
            HelpRow::key_entry("Shift+Tab, Tab", "help.dns.navigate_fields"),
            HelpRow::key_entry("Enter", "help.dns.query_records"),
            HelpRow::key_entry("Left, Right", "help.dns.select_type"),
            HelpRow::key_entry("k / Up, j / Down", "help.dns.scroll_answers"),
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
                Fragment::key("?"),
                Fragment::sep("/"),
                Fragment::key("q"),
                Fragment::sep("/"),
                Fragment::key("Esc"),
            ]),
            Shortcut::new(vec![
                Fragment::key(arrow::LEFT),
                Fragment::sep("/"),
                Fragment::key(arrow::UP),
                Fragment::sep("/"),
                Fragment::key("PgUp"),
                Fragment::sep("/"),
                Fragment::key("g"),
                Fragment::label("shortcuts.nav"),
                Fragment::key("G"),
                Fragment::sep("/"),
                Fragment::key("PgDn"),
                Fragment::sep("/"),
                Fragment::key(arrow::DOWN),
                Fragment::sep("/"),
                Fragment::key(arrow::RIGHT),
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
            .title(top_title_line("titles.help", Style::default()));
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
