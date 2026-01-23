use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Span;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph};
use tokio::sync::mpsc::UnboundedSender;
use tracing::debug;
use tui_input::{Input, InputRequest};

use crate::action::Action;
use crate::components::{Component, ComponentId};
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT};
use crate::widgets::shortcut::{Fragment, Shortcut};

#[derive(Debug, Clone, Default)]
pub struct SearchComponent {
    is_active: bool,
    should_send: bool,
    input: Input,
    action_tx: Option<UnboundedSender<Action>>,
}

impl SearchComponent {
    fn input_request(&mut self, key: KeyEvent) -> Option<InputRequest> {
        use KeyCode::*;
        use tui_input::InputRequest::*;

        match (key.code, key.modifiers) {
            (Backspace, KeyModifiers::NONE) => Some(DeletePrevChar),
            (Delete, KeyModifiers::NONE) => Some(DeleteNextChar),
            (Left, KeyModifiers::NONE) => Some(GoToPrevChar),
            (Left, KeyModifiers::CONTROL) => Some(GoToPrevWord),
            (Right, KeyModifiers::NONE) => Some(GoToNextChar),
            (Right, KeyModifiers::CONTROL) => Some(GoToNextWord),
            (Char('w'), KeyModifiers::CONTROL)
            | (Backspace, KeyModifiers::META)
            | (Backspace, KeyModifiers::ALT) => Some(DeletePrevWord),
            (Delete, KeyModifiers::CONTROL) => Some(DeleteNextWord),
            (Char('y'), KeyModifiers::CONTROL) => Some(Yank),
            (Home, KeyModifiers::NONE) => Some(GoToStart),
            (End, KeyModifiers::NONE) => Some(GoToEnd),
            (Char(c), KeyModifiers::NONE) => Some(InsertChar(c)),
            (Char(c), KeyModifiers::SHIFT) => Some(InsertChar(c)),
            (_, _) => None,
        }
    }

    fn send(&mut self) -> Result<()> {
        if self.is_active && self.should_send {
            let pattern =
                Some(str::trim(self.input.value())).filter(|s| !s.is_empty()).map(str::to_owned);
            self.action_tx.as_ref().unwrap().send(Action::SearchInputChanged(pattern))?;
            self.should_send = false;
        }

        Ok(())
    }
}

impl Component for SearchComponent {
    fn id(&self) -> ComponentId {
        ComponentId::Search
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::hl("←/C-←"),
                Fragment::raw(" move "),
                Fragment::hl("→/C-→"),
            ]),
            Shortcut::new(vec![
                Fragment::hl("Back/C-Back"),
                Fragment::raw(" del "),
                Fragment::hl("Del/C-Del"),
            ]),
            Shortcut::new(vec![Fragment::raw("Yank "), Fragment::hl("C-Y")]),
            Shortcut::new(vec![Fragment::hl("Home"), Fragment::raw(" jump "), Fragment::hl("End")]),
            Shortcut::new(vec![
                Fragment::raw("esc "),
                Fragment::hl("Esc"),
                Fragment::raw("/"),
                Fragment::hl("↵"),
            ]),
        ]
    }

    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(Some(Action::Quit));
            }
            KeyCode::Enter | KeyCode::Esc => {
                self.is_active = false;
                self.send()?;
                return Ok(Some(Action::Unfocus));
            }
            _ => {
                if let Some(req) = self.input_request(key) {
                    self.should_send = true;
                    let _ = self.input.handle(req);
                }
            }
        }
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Focus(ComponentId::Search) => self.is_active = true,
            Action::Tick => self.send()?,
            Action::SearchInputSet(pattern) => {
                debug!("handle Action::SearchInputSet, pattern={pattern:?}");
                self.input = pattern.unwrap_or_default().into();
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
        let mut right = Line::default();
        for shortcut in self.shortcuts() {
            right.push_span(Span::raw(TOP_TITLE_LEFT));
            right.extend(shortcut.into_spans(None));
            right.push_span(Span::raw(TOP_TITLE_RIGHT));
        }

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style)
            .title(left.left_aligned())
            .title(right.right_aligned());
        let input =
            Paragraph::new(self.input.value()).scroll((0, scroll as u16)).style(style).block(block);
        frame.render_widget(input, area);
        if self.is_active {
            let x = self.input.visual_cursor().max(scroll) - scroll + 1;
            frame.set_cursor_position((area.x + x as u16, area.y + 1));
        }

        Ok(())
    }
}
