use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::prelude::Style;
use ratatui::style::Color;
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use crate::app::action::Action;
use crate::components::{Component, ComponentContext, PopupComponent, PopupId};
use crate::models::Connection;
use crate::ui::layout::{popup_area, top_title_line};
use crate::ui::scrollbar::Scroller;
use crate::ui::shortcut::{Fragment, Shortcut};
use crate::ui::symbols::arrow;
use crate::utils::json::pretty_json;

#[derive(Debug)]
pub struct ConnectionDetailComponent {
    show: bool,
    total_lines: usize,
    data: String,

    scroller: Scroller,
}

impl ConnectionDetailComponent {
    pub fn new(_ctx: &ComponentContext) -> Self {
        Self {
            show: Default::default(),
            total_lines: Default::default(),
            data: Default::default(),
            scroller: Default::default(),
        }
    }

    fn show(&mut self, data: &Connection) {
        self.show = true;

        let pretty = pretty_json(data);
        self.total_lines = pretty.lines().count();
        self.data = pretty;
        self.scroller.position(0);
    }

    fn hide(&mut self) {
        self.show = false;
        self.data = String::default();
    }
}

impl PopupComponent for ConnectionDetailComponent {
    fn popup_id(&self) -> PopupId {
        PopupId::ConnectionDetail
    }
}

impl Component for ConnectionDetailComponent {
    fn shortcuts(&self) -> Vec<Shortcut> {
        vec![
            Shortcut::new(vec![
                Fragment::key("q"),
                Fragment::sep("/"),
                Fragment::key("Esc"),
                Fragment::sep("/"),
                Fragment::key("Enter"),
            ]),
            Shortcut::new(vec![
                Fragment::key(arrow::UP),
                Fragment::label("shortcuts.scroll"),
                Fragment::key(arrow::DOWN),
            ]),
            Shortcut::new(vec![
                Fragment::key("PgUp"),
                Fragment::label("shortcuts.page"),
                Fragment::key("PgDn"),
            ]),
        ]
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        if self.scroller.handle_key_event(key).is_consumed() {
            return Ok(None);
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => {
                self.hide();
                return Ok(Some(self.close_action()));
            }
            _ => {}
        };
        Ok(None)
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        if let Action::ConnectionDetail(connection) = action {
            self.show(connection.as_ref())
        };

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if !self.show {
            return Ok(());
        }

        let area = popup_area(area, 80, 75);
        self.scroller.length(self.total_lines, area.height.saturating_sub(2) as usize);

        // content
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Color::LightBlue)
            .title(top_title_line("titles.detail", Style::default()));
        let paragraph =
            Paragraph::new(self.data.as_str()).scroll((self.scroller.pos() as u16, 0)).block(block);

        frame.render_widget(Clear, area); // clears out the background
        frame.render_widget(paragraph, area);

        self.scroller.render(frame, area);

        Ok(())
    }
}
