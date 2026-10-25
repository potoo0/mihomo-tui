use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph};
use strum::{EnumCount, IntoEnumIterator};
use tokio::sync::mpsc::UnboundedSender;

use super::{Component, ComponentId};
use crate::action::Action;
use crate::config::Config;
use crate::i18n::{self, Language};
use crate::utils::symbols::{arrow, dot, key};
use crate::utils::text_ui::{popup_area, top_title_line};
use crate::widgets::scrollbar::Scroller;
use crate::widgets::shortcut::{Fragment, Shortcut};

#[derive(Default)]
pub struct LanguageComponent {
    selected: usize,
    state: ListState,
    visible_rows: usize,
    scrollable: bool,
    persist_preference: bool,
    action_tx: Option<UnboundedSender<Action>>,
}

impl Component for LanguageComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Language
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Arc<Config>) -> Result<()> {
        self.persist_preference = config.runtime_config;
        Ok(())
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        let mut shortcuts = vec![
            Shortcut::new(vec![
                Fragment::key(arrow::UP),
                Fragment::label("shortcuts.nav"),
                Fragment::key(arrow::DOWN),
            ]),
            Shortcut::new(vec![Fragment::key(key::ENTER), Fragment::label("shortcuts.apply")]),
            Shortcut::new(vec![Fragment::key("Esc"), Fragment::label("shortcuts.cancel")]),
        ];
        if self.scrollable {
            shortcuts.push(Shortcut::new(vec![
                Fragment::key("PgUp"),
                Fragment::label("shortcuts.page"),
                Fragment::key("PgDn"),
            ]));
            shortcuts.push(Shortcut::new(vec![
                Fragment::key("g"),
                Fragment::label("shortcuts.jump"),
                Fragment::key("G"),
            ]));
        }
        shortcuts
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(Language::COUNT - 1)
            }
            KeyCode::PageUp if self.scrollable => {
                self.selected = self.selected.saturating_sub(self.visible_rows);
            }
            KeyCode::PageDown | KeyCode::Char(' ') if self.scrollable => {
                self.selected = (self.selected + self.visible_rows).min(Language::COUNT - 1);
            }
            KeyCode::Home | KeyCode::Char('g') if self.scrollable => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') if self.scrollable => {
                self.selected = Language::COUNT - 1
            }
            KeyCode::Esc | KeyCode::Char('q') => return Ok(Some(Action::Unfocus)),
            KeyCode::Enter => {
                return Ok(Some(Action::LanguageChanged(
                    Language::iter().nth(self.selected).unwrap(),
                )));
            }
            _ => {}
        }
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        if matches!(action, Action::LanguageSelect) {
            self.selected =
                Language::iter().position(|value| value == i18n::language()).unwrap_or(0);
            self.state = ListState::default().with_selected(Some(self.selected));
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let area = popup_area(area, 30, 50);
        frame.render_widget(Clear, area);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(top_title_line("tabs.language", Style::default()));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let rows = Language::iter().map(|language| {
            let marker = if language == i18n::language() { dot::BLACK } else { dot::WHITE };
            ListItem::new(Line::from(vec![
                Span::styled(marker, Color::LightCyan),
                Span::raw(" "),
                Span::raw(language.native_name()),
            ]))
        });
        let list = List::new(rows.collect::<Vec<_>>())
            .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::REVERSED));
        // Keep room for a gap and a wrapped persistence note without shrinking short lists.
        let list_area = language_list_area(inner);
        self.visible_rows = list_area.height as usize;
        let scrollable = Language::COUNT > self.visible_rows;
        if self.scrollable != scrollable {
            self.scrollable = scrollable;
            if let Some(tx) = &self.action_tx {
                tx.send(Action::Shortcuts(self.shortcuts()))?;
            }
        }
        self.state.select(Some(self.selected));
        frame.render_stateful_widget(list, list_area, &mut self.state);
        if self.scrollable {
            let mut scroller = Scroller::default();
            scroller.length(Language::COUNT, self.visible_rows).position(self.state.offset());
            scroller.render(
                frame,
                Rect::new(area.right().saturating_sub(1), list_area.y, 1, list_area.height),
            );
        }
        let note_height = inner.height.min(2);
        let note_y = inner.bottom().saturating_sub(note_height);
        frame.render_widget(
            Paragraph::new(rust_i18n::t!(preference_note_key(self.persist_preference)))
                .wrap(ratatui::widgets::Wrap { trim: false }),
            Rect::new(inner.x, note_y, inner.width, note_height),
        );
        Ok(())
    }
}

/// Keep a gap and two fixed bottom rows separate from the scrollable language list.
fn language_list_area(inner: Rect) -> Rect {
    Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(3))
}

/// Whether the preference persists depends on the `runtime-config` setting.
fn preference_note_key(persist: bool) -> &'static str {
    if persist { "language.preference_saved" } else { "language.preference_session_only" }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyModifiers;

    use super::*;

    #[test]
    fn selection_requires_confirmation_and_navigation_stays_in_bounds() {
        let mut component = LanguageComponent::default();
        for _ in 0..3 {
            assert!(
                component
                    .handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
                    .unwrap()
                    .is_none()
            );
        }
        assert!(matches!(
            component.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap(),
            Some(Action::LanguageChanged(Language::SimplifiedChinese))
        ));
        for _ in 0..3 {
            component.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)).unwrap();
        }
        assert!(matches!(
            component.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).unwrap(),
            Some(Action::LanguageChanged(Language::English))
        ));
    }

    #[test]
    fn cancel_does_not_apply_candidate() {
        let mut component = LanguageComponent::default();
        component.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)).unwrap();
        for key in [KeyCode::Esc, KeyCode::Char('q')] {
            assert!(matches!(
                component.handle_key_event(KeyEvent::new(key, KeyModifiers::NONE)).unwrap(),
                Some(Action::Unfocus)
            ));
        }
    }

    #[test]
    fn short_list_fits_small_popup_and_long_list_has_a_viewport() {
        let area = popup_area(Rect::new(0, 1, 80, 17), 30, 50);
        assert_eq!(area.width, 24);
        let inner = Block::bordered().inner(area);
        let short = language_list_area(inner);
        assert!(short.height >= 2);
        assert!(short.bottom() + 1 < inner.bottom());
        let long = language_list_area(inner);
        assert!(long.height > 0 && long.height < 20);
        assert!(long.bottom() <= inner.bottom());
    }

    #[test]
    fn scrolling_hints_only_appear_for_overflow() {
        let mut component = LanguageComponent::default();
        assert_eq!(component.shortcuts().len(), 3);
        component.scrollable = true;
        let labels: String = component
            .shortcuts()
            .iter()
            .flat_map(|shortcut| {
                shortcut
                    .spans_for(crate::widgets::shortcut::ShortcutMode::Full, None)
                    .into_iter()
                    .map(|span| span.content.into_owned())
            })
            .collect();
        assert!(labels.contains("PgUp") && labels.contains("PgDn"));
        assert!(labels.contains('g') && labels.contains('G'));
    }

    #[test]
    fn preference_note_follows_runtime_config_setting() {
        let mut component = LanguageComponent::default();
        let mut config = crate::config::default_config().unwrap();
        component.register_config_handler(Arc::new(config.clone())).unwrap();
        assert_eq!(preference_note_key(component.persist_preference), "language.preference_saved");

        config.runtime_config = false;
        component.register_config_handler(Arc::new(config)).unwrap();
        assert_eq!(
            preference_note_key(component.persist_preference),
            "language.preference_session_only"
        );

        for persist in [true, false] {
            let key = preference_note_key(persist);
            assert_ne!(rust_i18n::t!(key, locale = "en").as_ref(), key, "{key}: missing entry");
        }
    }
}
