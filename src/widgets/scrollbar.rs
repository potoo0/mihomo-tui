use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::symbols::line;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

use crate::utils::symbols::arrow;

#[derive(Debug)]
pub struct Scroller {
    step: usize,
    content_length: usize,
    viewport_content_length: usize,

    state: ScrollbarState,
}

impl Default for Scroller {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Scroller {
    pub fn new(step: usize) -> Self {
        Self { step, content_length: 0, viewport_content_length: 0, state: Default::default() }
    }

    pub fn step(&mut self, step: usize) -> &mut Self {
        self.step = step;
        self
    }

    pub fn position(&mut self, position: usize) -> &mut Self {
        self.state = self.state.position(position);
        self
    }

    pub fn length(&mut self, content_length: usize, viewport_content_length: usize) -> &mut Self {
        self.content_length = content_length;
        self.viewport_content_length = viewport_content_length;
        self.state = self
            .state
            .content_length(self.align_up(content_length.saturating_sub(viewport_content_length)))
            .viewport_content_length(viewport_content_length);
        self
    }

    pub fn align_up(&self, value: usize) -> usize {
        value.div_ceil(self.step) * self.step
    }

    pub fn first(&mut self) {
        self.position(0);
    }

    pub fn last(&mut self) {
        self.position(
            self.align_up(self.content_length.saturating_sub(self.viewport_content_length)),
        );
    }

    pub fn next(&mut self) {
        if let Some(max_pos) = self.content_length.checked_sub(self.viewport_content_length) {
            let pos = self.pos().saturating_add(self.step).min(self.align_up(max_pos));
            self.position(pos);
        }
    }

    pub fn prev(&mut self) {
        self.position(self.pos().saturating_sub(self.step));
    }

    pub fn page_down(&mut self) {
        if let Some(max_pos) = self.content_length.checked_sub(self.viewport_content_length) {
            let pos =
                self.pos().saturating_add(self.viewport_content_length).min(self.align_up(max_pos));
            self.position(pos);
        }
    }

    pub fn page_up(&mut self) {
        self.position(self.pos().saturating_sub(self.viewport_content_length));
    }

    /// Handle scroll key events
    ///
    /// # Arguments
    ///
    /// - `key`: the key event to handle
    ///
    /// # Returns
    ///
    /// - `true` if the key was handled, `false` otherwise
    pub fn handle_key_event(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('g') => self.first(),
            KeyCode::Char('G') => self.last(),
            KeyCode::Char('j') | KeyCode::Down => self.next(),
            KeyCode::Char('k') | KeyCode::Up => self.prev(),
            KeyCode::PageDown | KeyCode::Char(' ') => self.page_down(),
            KeyCode::PageUp => self.page_up(),
            _ => return false,
        }

        true
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .track_symbol(Some(line::VERTICAL))
            .begin_symbol(Some(arrow::UP))
            .end_symbol(Some(arrow::DOWN));
        frame.render_stateful_widget(scrollbar, area, &mut self.state);
    }

    #[inline]
    pub fn pos(&self) -> usize {
        self.state.get_position()
    }

    pub fn end_pos(&self) -> usize {
        self.state
            .get_position()
            .saturating_add(self.viewport_content_length)
            .min(self.content_length)
    }

    pub fn content_length(&self) -> usize {
        self.content_length
    }

    pub fn viewport_content_length(&self) -> usize {
        self.viewport_content_length
    }

    pub fn step_value(&self) -> usize {
        self.step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next() {
        let mut scroll = Scroller::new(2);
        scroll.length(13, 10);

        let expected = vec![(0, 10), (2, 12), (4, 13), (4, 13), (4, 13)];
        for pair in expected.into_iter() {
            assert_eq!((scroll.pos(), scroll.end_pos()), pair);
            scroll.next();
        }
    }

    #[test]
    fn test_zero_len() {
        let mut scroll = Scroller::new(2);
        scroll.length(0, 10);
        assert_eq!((scroll.pos(), scroll.end_pos()), (0, 0));

        scroll.next();
        assert_eq!((scroll.pos(), scroll.end_pos()), (0, 0));
        scroll.prev();
        assert_eq!((scroll.pos(), scroll.end_pos()), (0, 0));

        scroll.first();
        assert_eq!((scroll.pos(), scroll.end_pos()), (0, 0));
        scroll.last();
        assert_eq!((scroll.pos(), scroll.end_pos()), (0, 0));

        scroll.page_down();
        assert_eq!((scroll.pos(), scroll.end_pos()), (0, 0));
        scroll.page_up();
        assert_eq!((scroll.pos(), scroll.end_pos()), (0, 0));
    }

    #[test]
    fn test_scroll() {
        let mut scroll = Scroller::new(2);
        scroll.length(100, 10);
        assert_eq!((scroll.pos(), scroll.end_pos()), (0, 10));

        scroll.next();
        assert_eq!((scroll.pos(), scroll.end_pos()), (2, 12));

        scroll.next();
        assert_eq!((scroll.pos(), scroll.end_pos()), (4, 14));

        scroll.prev();
        assert_eq!((scroll.pos(), scroll.end_pos()), (2, 12));

        scroll.first();
        assert_eq!((scroll.pos(), scroll.end_pos()), (0, 10));

        scroll.prev();
        assert_eq!((scroll.pos(), scroll.end_pos()), (0, 10));

        scroll.last();
        assert_eq!((scroll.pos(), scroll.end_pos()), (90, 100));

        scroll.next();
        assert_eq!((scroll.pos(), scroll.end_pos()), (90, 100));
    }
}
