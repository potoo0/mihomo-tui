use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};
use tui_input::Input;

use super::{KeyOutcome, SettingPane};
use crate::utils::symbols::arrow;
use crate::utils::tui_input::input_request;
use crate::widgets::scrollable_navigator::ScrollableNavigator;
use crate::widgets::shortcut::{Fragment, Shortcut};

#[derive(Debug, Default)]
pub(super) struct SourceIpAliasSettingPane {
    alias_input: Input,
    source_ips: Vec<String>,
    aliases: HashMap<String, String>,
    navigator: ScrollableNavigator,
}

impl SourceIpAliasSettingPane {
    pub(super) fn load(&mut self, mut source_ips: Vec<String>, aliases: &HashMap<String, String>) {
        source_ips.extend(aliases.keys().cloned());
        source_ips.sort_unstable();
        source_ips.dedup();

        self.source_ips = source_ips;
        self.aliases = aliases.clone();
        self.sync_navigator_length(0);
        self.load_selected_alias();
    }

    pub(super) fn reset(&mut self) {
        self.alias_input.reset();
        self.source_ips.clear();
        self.aliases.clear();
        self.navigator = ScrollableNavigator::default();
    }

    pub(super) fn aliases(&mut self) -> HashMap<String, String> {
        self.save_alias_input();
        self.aliases
            .iter()
            .filter_map(|(source_ip, alias)| {
                let alias = alias.trim();
                (!alias.is_empty()).then(|| (source_ip.clone(), alias.to_string()))
            })
            .collect()
    }
}

impl SettingPane for SourceIpAliasSettingPane {
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![Shortcut::new(vec![
            Fragment::hl(arrow::UP),
            Fragment::raw("/"),
            Fragment::hl("PgUp"),
            Fragment::raw(" nav "),
            Fragment::hl("PgDn"),
            Fragment::raw("/"),
            Fragment::hl(arrow::DOWN),
        ])]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> KeyOutcome {
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => return KeyOutcome::Ignored,
            KeyCode::Enter => return KeyOutcome::Ignored,
            _ => (),
        }

        if self.handle_input_key(key).is_consumed() {
            return KeyOutcome::Consumed;
        }

        self.handle_navigation_key(key)
    }

    fn draw_content(&mut self, frame: &mut Frame, area: Rect, active: bool) {
        self.draw_alias(frame, area, active);
    }
}

impl SourceIpAliasSettingPane {
    fn handle_input_key(&mut self, key: KeyEvent) -> KeyOutcome {
        if self.source_ips.is_empty() {
            return KeyOutcome::Ignored;
        }

        let Some(req) = input_request(key) else {
            return KeyOutcome::Ignored;
        };

        let _ = self.alias_input.handle(req);
        self.save_alias_input();
        KeyOutcome::Consumed
    }

    fn handle_navigation_key(&mut self, key: KeyEvent) -> KeyOutcome {
        let before = self.navigator.focused;
        self.save_alias_input();

        if !self.navigator.handle_key_event(false, key) {
            return KeyOutcome::Ignored;
        }

        if self.navigator.focused != before {
            self.load_selected_alias();
        }
        KeyOutcome::Consumed
    }
}

impl SourceIpAliasSettingPane {
    fn sync_navigator_length(&mut self, viewport_content_length: usize) {
        self.navigator.length(self.source_ips.len(), viewport_content_length);
        if self.source_ips.is_empty() {
            self.navigator.focused = None;
        } else if self.navigator.focused.is_none() {
            self.navigator.focused = Some(0);
            if viewport_content_length > 0 {
                self.navigator.scroller.first();
            }
        }
    }

    fn selected_source_ip(&self) -> Option<&str> {
        self.navigator.focused.and_then(|idx| self.source_ips.get(idx)).map(String::as_str)
    }

    fn save_alias_input(&mut self) {
        let Some(source_ip) = self.selected_source_ip().map(str::to_string) else {
            return;
        };

        let alias = self.alias_input.value().trim();
        if alias.is_empty() {
            self.aliases.remove(&source_ip);
        } else {
            self.aliases.insert(source_ip, alias.to_string());
        }
    }

    fn load_selected_alias(&mut self) {
        let alias = self
            .selected_source_ip()
            .and_then(|source_ip| self.aliases.get(source_ip))
            .cloned()
            .unwrap_or_default();
        self.alias_input = alias.into();
    }

    fn visible_range(&mut self, height: usize) -> (usize, usize) {
        self.sync_navigator_length(height);

        if let Some(selected) = self.navigator.focused {
            let start = self.navigator.scroller.pos();
            let end = self.navigator.scroller.end_pos();

            if !(start..end).contains(&selected) {
                self.navigator.focus(selected);
            }
        }

        (self.navigator.scroller.pos(), self.navigator.scroller.end_pos())
    }
}

impl SourceIpAliasSettingPane {
    fn draw_alias(&mut self, frame: &mut Frame, area: Rect, active: bool) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(if active { Color::Cyan } else { Color::DarkGray })
            .title(" Source IP Alias ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let [list_area, scrollbar_area] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
        self.render_list(frame, list_area, active);
        self.navigator.render(frame, scrollbar_area);
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect, active: bool) {
        if self.source_ips.is_empty() {
            let line =
                Line::from(Span::styled("No source IPs from current connections", Color::DarkGray));
            frame.render_widget(line, area);
            return;
        }

        let height = area.height as usize;
        let (start, end) = self.visible_range(height);
        let source_width = (area.width / 2).saturating_sub(3) as usize;
        let lines: Vec<_> = self.source_ips[start..end]
            .iter()
            .enumerate()
            .map(|(offset, source_ip)| {
                let idx = start + offset;
                let selected = self.navigator.focused == Some(idx);
                let editing = active && selected;
                let alias = if editing {
                    self.alias_input.value()
                } else {
                    self.aliases.get(source_ip).map(String::as_str).unwrap_or_default()
                };
                let alias_style = if editing {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED)
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::styled(if selected { "> " } else { "  " }, Color::Cyan),
                    Span::raw(pad(source_ip, source_width)),
                    Span::styled(alias, alias_style),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);

        if active && let Some(selected) = self.navigator.focused {
            let source_width = (area.width / 2).saturating_sub(3);
            let cursor_x = area.x + 2 + source_width + self.alias_input.visual_cursor() as u16;
            let cursor_y = area.y + (selected - start) as u16;
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn pad(value: &str, width: usize) -> String {
    format!("{value:<width$}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aliases(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(source_ip, alias)| (source_ip.to_string(), alias.to_string()))
            .collect()
    }

    #[test]
    fn alias_load_keeps_current_ips_and_existing_aliases() {
        let mut pane = SourceIpAliasSettingPane::default();

        pane.load(
            vec!["10.0.0.1".into(), "10.0.0.2".into()],
            &aliases(&[("10.0.0.1", "phone"), ("10.0.0.9", "stale")]),
        );

        assert_eq!(pane.selected_source_ip(), Some("10.0.0.1"));
        assert_eq!(pane.alias_input.value(), "phone");
        assert_eq!(
            pane.source_ips,
            vec!["10.0.0.1".to_string(), "10.0.0.2".to_string(), "10.0.0.9".to_string()]
        );
        assert_eq!(pane.aliases.get("10.0.0.9").map(String::as_str), Some("stale"));
    }

    #[test]
    fn alias_save_drops_empty_current_aliases_and_keeps_stale_aliases() {
        let mut pane = SourceIpAliasSettingPane::default();
        pane.load(
            vec!["10.0.0.1".into(), "10.0.0.2".into()],
            &aliases(&[("10.0.0.1", "phone"), ("10.0.0.9", "stale")]),
        );

        pane.alias_input.reset();
        let saved = pane.aliases();

        assert_eq!(saved.get("10.0.0.1"), None);
        assert_eq!(saved.get("10.0.0.9").map(String::as_str), Some("stale"));
    }

    #[test]
    fn alias_selection_saves_old_input_and_loads_new_alias() {
        let mut pane = SourceIpAliasSettingPane::default();
        pane.load(vec!["10.0.0.1".into(), "10.0.0.2".into()], &aliases(&[("10.0.0.2", "desktop")]));

        pane.alias_input = "phone".into();
        pane.handle_key_event(KeyEvent::from(KeyCode::Down));

        assert_eq!(pane.aliases.get("10.0.0.1").map(String::as_str), Some("phone"));
        assert_eq!(pane.alias_input.value(), "desktop");
        assert_eq!(pane.selected_source_ip(), Some("10.0.0.2"));
    }

    #[test]
    fn alias_page_down_uses_navigator_when_input_ignores_key() {
        let source_ips = (0..10).map(|idx| format!("10.0.0.{idx}")).collect();
        let mut pane = SourceIpAliasSettingPane::default();
        pane.load(source_ips, &HashMap::new());
        pane.visible_range(3);

        assert_eq!(pane.navigator.focused, Some(0));

        pane.handle_key_event(KeyEvent::from(KeyCode::PageDown));

        assert_eq!(pane.navigator.focused, Some(3));
        assert_eq!((pane.navigator.scroller.pos(), pane.navigator.scroller.end_pos()), (3, 6));
    }

    #[test]
    fn alias_tab_and_backtab_are_ignored_for_parent_pane_switching() {
        let mut pane = SourceIpAliasSettingPane::default();

        assert_eq!(pane.handle_key_event(KeyEvent::from(KeyCode::Tab)), KeyOutcome::Ignored);
        assert_eq!(pane.handle_key_event(KeyEvent::from(KeyCode::BackTab)), KeyOutcome::Ignored);
    }

    #[test]
    fn alias_q_is_consumed_by_input_focus() {
        let mut pane = SourceIpAliasSettingPane::default();
        pane.load(vec!["10.0.0.1".into()], &HashMap::new());

        assert_eq!(pane.handle_key_event(KeyEvent::from(KeyCode::Char('q'))), KeyOutcome::Consumed);
        assert_eq!(pane.alias_input.value(), "q");
    }

    #[test]
    fn alias_space_and_jk_are_consumed_by_input_before_navigator() {
        let mut pane = SourceIpAliasSettingPane::default();
        pane.load(vec!["10.0.0.1".into(), "10.0.0.2".into()], &HashMap::new());

        assert_eq!(pane.handle_key_event(KeyEvent::from(KeyCode::Char(' '))), KeyOutcome::Consumed);
        assert_eq!(pane.handle_key_event(KeyEvent::from(KeyCode::Char('j'))), KeyOutcome::Consumed);
        assert_eq!(pane.handle_key_event(KeyEvent::from(KeyCode::Char('k'))), KeyOutcome::Consumed);

        assert_eq!(pane.alias_input.value(), " jk");
        assert_eq!(pane.navigator.focused, Some(0));
    }

    #[test]
    fn alias_enter_is_ignored_by_input_focus() {
        let mut pane = SourceIpAliasSettingPane::default();

        assert_eq!(pane.handle_key_event(KeyEvent::from(KeyCode::Enter)), KeyOutcome::Ignored);
    }

    #[test]
    fn alias_input_is_ignored_without_source_ips() {
        let mut pane = SourceIpAliasSettingPane::default();

        assert_eq!(pane.handle_key_event(KeyEvent::from(KeyCode::Char('q'))), KeyOutcome::Ignored);
        assert!(pane.alias_input.value().is_empty());
        assert_eq!(pane.navigator.focused, None);
    }
}
