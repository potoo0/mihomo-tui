use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Span;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph};
use tokio::sync::mpsc::UnboundedSender;
use tracing::debug;
use tui_input::Input;

use crate::app::action::Action;
use crate::components::{Component, ComponentContext, ComponentId, ComponentIdentity, FocusTarget};
use crate::i18n::column_title;
use crate::shortcut;
use crate::ui::input::input_request;
use crate::ui::layout::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::ui::shortcut::{Fragment, Shortcut, ShortcutMode, shortcuts_full_width};
use crate::ui::symbols::key;

/// The block's two border cells and one cell between its left and right titles.
const TOP_TITLE_RESERVED_WIDTH: usize = 2 + 1;

#[derive(Debug, Clone)]
pub struct FilterComponent {
    action_tx: UnboundedSender<Action>,

    is_active: bool,
    should_send: bool,
    input: Input,
    /// Canonical column titles for the active tab; localized at draw time.
    placeholder: Option<Vec<&'static str>>,
    shortcuts_full_width: usize,
}

impl FilterComponent {
    pub fn new(ctx: &ComponentContext) -> Self {
        let mut component = Self {
            action_tx: ctx.actions.clone(),

            is_active: Default::default(),
            should_send: Default::default(),
            input: Default::default(),
            placeholder: Default::default(),
            shortcuts_full_width: Default::default(),
        };
        component.shortcuts_full_width = shortcuts_title_width(&component.shortcuts());
        component
    }

    fn send(&mut self) -> Result<()> {
        if self.is_active && self.should_send {
            let pattern =
                Some(str::trim(self.input.value())).filter(|s| !s.is_empty()).map(str::to_owned);
            self.action_tx.send(Action::FilterChanged(pattern))?;
            self.should_send = false;
        }

        Ok(())
    }

    /// Builds the localized placeholder at draw time so language changes apply
    /// immediately without replaying `Action::FilterPlaceholder`.
    fn placeholder_text(&self) -> String {
        self.placeholder.as_deref().map_or_else(String::new, |fields| {
            let titles = fields.iter().map(|field| column_title(field)).collect::<Vec<_>>();
            rust_i18n::t!("filter.placeholder", fields = titles.join(" ")).into_owned()
        })
    }
}

impl ComponentIdentity for FilterComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Filter
    }
}

impl Component for FilterComponent {
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::key(key::left_with_ctrl()),
                Fragment::label("shortcuts.move").compact_sep("/"),
                Fragment::key(key::right_with_ctrl()),
            ]),
            Shortcut::new(vec![
                Fragment::key("Back/C-Back").compact("Back"),
                Fragment::label("shortcuts.delete").compact_sep("/"),
                Fragment::key("Del/C-Del").compact("Del"),
            ]),
            Shortcut::new(vec![Fragment::key("C-Y"), Fragment::label("shortcuts.yank")]),
            Shortcut::new(vec![
                Fragment::key("Home"),
                Fragment::label("shortcuts.jump").compact_sep("/"),
                Fragment::key("End"),
            ]),
            Shortcut::new(vec![
                Fragment::key("Esc"),
                Fragment::sep("/"),
                Fragment::key(key::ENTER),
                Fragment::label("shortcuts.escape").hidden(),
            ]),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                self.is_active = false;
                self.send()?;
                return Ok(Some(Action::Unfocus(FocusTarget::Filter)));
            }
            _ => {
                if let Some(req) = input_request(key) {
                    self.should_send = true;
                    let _ = self.input.handle(req);
                }
            }
        }
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Focus(FocusTarget::Filter) => self.is_active = true,
            Action::LanguageChanged(_) => {
                self.shortcuts_full_width = shortcuts_title_width(&self.shortcuts());
            }
            Action::Tick => self.send()?,
            Action::FilterSet(pattern) => {
                debug!("handle Action::FilterSet, pattern={pattern:?}");
                self.input = pattern.unwrap_or_default().into();
            }
            Action::FilterPlaceholder(placeholder) => {
                debug!("handle Action::FilterPlaceholder, placeholder={placeholder:?}");
                self.placeholder = placeholder;
            }
            _ => (),
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let style =
            if self.is_active { Style::default().fg(Color::LightBlue) } else { Style::default() };

        let width = area.width.max(3) - 3;
        let scroll = self.input.visual_scroll(width as usize);

        // left align
        let mut left = Line::from(Span::raw(TOP_TITLE_LEFT));
        left.extend(shortcut!("filter", 0).into_spans(None));
        left.push_span(Span::raw(TOP_TITLE_RIGHT));
        // right align
        let right_width =
            (area.width as usize).saturating_sub(left.width() + TOP_TITLE_RESERVED_WIDTH);
        let mode = if self.shortcuts_full_width <= right_width {
            ShortcutMode::Full
        } else {
            ShortcutMode::Compact
        };
        let shortcuts = self.shortcuts();
        let mut right = Line::default();
        for shortcut in &shortcuts {
            right.push_span(Span::raw(TOP_TITLE_LEFT));
            right.extend(shortcut.spans_for(mode, None));
            right.push_span(Span::raw(TOP_TITLE_RIGHT));
        }

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style)
            .title(left.left_aligned())
            .title(right.right_aligned());
        let paragraph = if self.input.value().is_empty() {
            Paragraph::new(Line::from(Span::styled(
                self.placeholder_text(),
                Style::default().fg(Color::DarkGray),
            )))
        } else {
            Paragraph::new(self.input.value()).scroll((0, scroll as u16)).style(style)
        };
        let input = paragraph.block(block);
        frame.render_widget(input, area);
        if self.is_active {
            let x = self.input.visual_cursor().max(scroll) - scroll + 1;
            frame.set_cursor_position((area.x + x as u16, area.y + 1));
        }

        Ok(())
    }
}

fn shortcuts_title_width(shortcuts: &[Shortcut]) -> usize {
    // The outer markers add one more pair to the separators between shortcuts.
    let sep_width = Span::raw(TOP_TITLE_LEFT).width() + Span::raw(TOP_TITLE_RIGHT).width();
    shortcuts_full_width(shortcuts, sep_width)
}
