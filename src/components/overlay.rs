use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::{Color, Line, Span, Style};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph};

use crate::utils::symbols::dot;
use crate::utils::text_ui::{TOP_TITLE_LEFT, TOP_TITLE_RIGHT, popup_area};

pub struct OverlayComponent {
    pub icon: &'static str,
    pub icon_style: Style,
    pub title: &'static str,
    pub content: Box<str>,
}

impl OverlayComponent {
    pub fn error(title: &'static str, content: impl Into<Box<str>>) -> Self {
        Self {
            icon: dot::RED_LARGE,
            icon_style: Style::default().fg(Color::Red),
            title,
            content: content.into(),
        }
    }

    /// Determine whether the overlay should be closed for the given key event.
    pub fn should_close_on_key(&self, key: KeyEvent) -> bool {
        matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q'))
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) -> Result<()> {
        let area = popup_area(area, 80, 75);

        // content
        let title_line = Line::from(vec![
            Span::raw(TOP_TITLE_LEFT),
            Span::styled(self.icon, self.icon_style),
            Span::raw("  "),
            Span::raw(self.title),
            Span::raw(TOP_TITLE_RIGHT),
        ]);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(title_line)
            .padding(Padding::symmetric(2, 1));
        let paragraph = Paragraph::new(self.content.as_ref()).block(block);

        frame.render_widget(Clear, area); // clears out the background
        frame.render_widget(paragraph, area);

        Ok(())
    }
}
