use std::borrow::Cow;
use std::cmp::PartialEq;
use std::str::FromStr;

use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::prelude::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use strum::{Display, EnumIter, IntoEnumIterator};
use tokio::sync::mpsc::UnboundedSender;
use tui_input::{Input, InputRequest};
use url::Url;

use crate::action::Action;
use crate::components::proxy_setting::get_proxy_setting;
use crate::components::shortcut::{Fragment, Shortcut};
use crate::components::{Component, ComponentId};
use crate::utils::text_ui::{popup_area, top_title_line};

const LINE_HEIGHT: u16 = 3;

#[derive(Debug, Default, Copy, Clone, PartialEq, Display, EnumIter)]
pub enum ProxySettingField {
    #[default]
    #[strum(to_string = "Test URL")]
    TestUrl,
    #[strum(to_string = "Test Timeout (ms)")]
    TestTimeout,
    #[strum(to_string = "Threshold (good,bad)")]
    Threshold,
}

impl ProxySettingField {
    pub fn next(&self) -> Self {
        match self {
            ProxySettingField::TestUrl => ProxySettingField::TestTimeout,
            ProxySettingField::TestTimeout => ProxySettingField::Threshold,
            ProxySettingField::Threshold => ProxySettingField::TestUrl,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            ProxySettingField::TestUrl => ProxySettingField::Threshold,
            ProxySettingField::TestTimeout => ProxySettingField::TestUrl,
            ProxySettingField::Threshold => ProxySettingField::TestTimeout,
        }
    }

    pub fn value(&self) -> String {
        let lock = get_proxy_setting();
        let setting = lock.read().unwrap();

        match self {
            ProxySettingField::TestUrl => setting.test_url.clone(),
            ProxySettingField::TestTimeout => setting.test_timeout.to_string(),
            ProxySettingField::Threshold => {
                format!("{},{}", setting.threshold.0, setting.threshold.1)
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct ProxySettingComponent {
    show: bool,
    focused: ProxySettingField,
    input: Input,
    error: Option<String>,

    action_tx: Option<UnboundedSender<Action>>,
}

impl ProxySettingComponent {
    fn show(&mut self) {
        self.show = true;
        self.focused = ProxySettingField::default();
        self.input = self.focused.value().into();
        self.error = None;
    }

    fn hide(&mut self) {
        self.show = false;
        self.input.reset();
        self.error = None;
    }

    fn submit(&self) -> Result<(), String> {
        let lock = get_proxy_setting();
        let mut setting = lock.write().unwrap();

        match self.focused {
            ProxySettingField::TestUrl => {
                let url = self.input.value().trim();
                if url.is_empty() {
                    Err("URL cannot be empty".into())
                } else if !url.starts_with("http://") && !url.starts_with("https://") {
                    Err("URL must start with http:// or https://".into())
                } else {
                    Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
                    setting.test_url = url.to_string();
                    Ok(())
                }
            }

            ProxySettingField::TestTimeout => match u64::from_str(self.input.value().trim()) {
                Ok(v) if v > 0 && v <= 60000 => {
                    setting.test_timeout = v;
                    Ok(())
                }
                Ok(_) => Err("Timeout must be between 1 and 60000 milliseconds".into()),
                Err(_) => Err("Timeout must be a valid number".into()),
            },

            ProxySettingField::Threshold => {
                let parts: Vec<_> = self.input.value().split(',').collect();
                if parts.len() != 2 {
                    return Err(
                        "Threshold must be two comma-separated numbers (e.g. 500,800)".into()
                    );
                }
                let (a, b) = (u64::from_str(parts[0].trim()), u64::from_str(parts[1].trim()));
                match (a, b) {
                    (Ok(x), Ok(y)) if x > 0 && x < y => {
                        setting.threshold = (x, y);
                        Ok(())
                    }
                    (Ok(_), Ok(_)) => Err("Threshold must satisfy good < bad".into()),
                    _ => Err("Threshold values must be valid positive numbers".into()),
                }
            }
        }
    }

    fn next(&mut self) {
        self.error = self.submit().err();
        if self.error.is_some() {
            return;
        }
        self.focused = self.focused.next();
        self.input = self.focused.value().into();
    }

    fn prev(&mut self) {
        self.error = self.submit().err();
        if self.error.is_some() {
            return;
        }
        self.focused = self.focused.prev();
        self.input = self.focused.value().into();
    }

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
            (Home, KeyModifiers::NONE) => Some(GoToStart),
            (End, KeyModifiers::NONE) => Some(GoToEnd),
            (Char(c), KeyModifiers::NONE) => Some(InsertChar(c)),
            (Char(c), KeyModifiers::SHIFT) => Some(InsertChar(c)),
            (_, _) => None,
        }
    }

    fn render_settings(&self, frame: &mut Frame, mut area: Rect) {
        area.height = LINE_HEIGHT;

        for field in ProxySettingField::iter() {
            let focused = self.focused == field;
            let (border_color, val) = if focused {
                (Color::Cyan, Cow::from(self.input.value()))
            } else {
                (Color::DarkGray, field.value().into())
            };
            let block = Block::bordered()
                .title(field.to_string())
                .border_type(BorderType::Rounded)
                .border_style(border_color);
            let line = Line::raw(val);
            let paragraph = Paragraph::new(line).block(block);
            frame.render_widget(paragraph, area);
            if focused {
                frame.set_cursor_position((
                    area.x + self.input.visual_cursor() as u16 + 1,
                    area.y + 1,
                ));
            }
            area.y += LINE_HEIGHT;
        }
        if let Some(err) = &self.error {
            let block = Block::bordered().border_type(BorderType::Rounded).border_style(Color::Red);
            let line = Line::from(Span::styled(err, Style::default().fg(Color::Red)));
            let paragraph = Paragraph::new(line).block(block);
            frame.render_widget(paragraph, area);
        }
    }
}

impl Component for ProxySettingComponent {
    fn id(&self) -> ComponentId {
        ComponentId::ProxySetting
    }

    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![Fragment::hl("⇧⇤"), Fragment::raw(" nav "), Fragment::hl("⇥")]),
            Shortcut::new(vec![Fragment::raw("confirm "), Fragment::hl("↵")]),
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
            KeyCode::Char('q') | KeyCode::Esc => {
                self.hide();
                return Ok(Some(Action::Unfocus));
            }
            KeyCode::Tab => self.next(),
            KeyCode::BackTab => self.prev(),
            KeyCode::Enter => {
                self.error = self.submit().err();
                if self.error.is_none() {
                    self.hide();
                    self.action_tx.as_ref().unwrap().send(Action::ProxiesRefresh)?;
                    return Ok(Some(Action::Unfocus));
                }
            }
            _ => {
                if let Some(req) = self.input_request(key) {
                    let _ = self.input.handle(req);
                }
            }
        }

        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        if matches!(action, Action::ProxySetting) {
            self.show();
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if !self.show {
            return Ok(());
        }

        let area = popup_area(area, 80, 80);
        frame.render_widget(Clear, area); // clears out the background
        // outer margin
        let area = area.inner(Margin::new(2, 1));

        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("proxy settings", Style::default()));
        let content_area = block.inner(area);
        frame.render_widget(block, area);
        self.render_settings(frame, content_area);

        Ok(())
    }
}
