use std::sync::Arc;

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

use crate::action::Action;
use crate::api::Api;
use crate::components::{Component, ComponentId};
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::utils::tui_input::input_request;
use crate::widgets::shortcut::{Fragment, Shortcut, ShortcutMode, shortcuts_full_width};

#[derive(Debug, Clone, Default)]
pub struct FilterComponent {
    is_active: bool,
    should_send: bool,
    input: Input,
    placeholder: Option<String>,
    action_tx: Option<UnboundedSender<Action>>,
    shortcuts_full_width: usize,
}

impl FilterComponent {
    fn send(&mut self) -> Result<()> {
        if self.is_active && self.should_send {
            let pattern =
                Some(str::trim(self.input.value())).filter(|s| !s.is_empty()).map(str::to_owned);
            self.action_tx.as_ref().unwrap().send(Action::FilterChanged(pattern))?;
            self.should_send = false;
        }

        Ok(())
    }
}

impl Component for FilterComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Filter
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::hl("←/C-←"),
                Fragment::raw(" move "),
                Fragment::hl("→/C-→"),
            ])
            .compact(vec![
                Fragment::hl("←/C-←"),
                Fragment::raw("/"),
                Fragment::hl("→/C-→"),
            ]),
            Shortcut::new(vec![
                Fragment::hl("Back/C-Back"),
                Fragment::raw(" del "),
                Fragment::hl("Del/C-Del"),
            ])
            .compact(vec![
                Fragment::hl("Back"),
                Fragment::raw("/"),
                Fragment::hl("Del"),
            ]),
            Shortcut::new(vec![Fragment::hl("C-Y"), Fragment::raw("ank")]),
            Shortcut::new(vec![Fragment::hl("Home"), Fragment::raw(" jump "), Fragment::hl("End")])
                .compact(vec![Fragment::hl("Home"), Fragment::raw("/"), Fragment::hl("End")]),
            Shortcut::new(vec![
                Fragment::raw("esc "),
                Fragment::hl("Esc"),
                Fragment::raw("/"),
                Fragment::hl("↵"),
            ])
            .compact(vec![Fragment::hl("Esc"), Fragment::raw("/"), Fragment::hl("↵")]),
        ]
    }

    fn init(&mut self, _api: Arc<Api>) -> Result<()> {
        self.shortcuts_full_width = shortcuts_full_width(&self.shortcuts(), 2);
        Ok(())
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                self.is_active = false;
                self.send()?;
                return Ok(Some(Action::Unfocus));
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
            Action::Focus(ComponentId::Filter) => self.is_active = true,
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
        left.extend(Shortcut::from("filter", 0).unwrap().into_spans(None));
        left.push_span(Span::raw(TOP_TITLE_RIGHT));
        // right align
        let right_width = area.width.saturating_sub(left.width() as u16 + 1);
        let mode = if self.shortcuts_full_width <= right_width as usize {
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
                self.placeholder.as_deref().unwrap_or_default(),
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
