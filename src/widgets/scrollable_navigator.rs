use std::rc::Rc;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::widgets::scrollbar::Scroller;

#[derive(Debug, Default)]
pub struct ScrollableNavigator {
    pub focused: Option<usize>,
    pub scroller: Scroller,
}

impl ScrollableNavigator {
    pub fn new(scroll_step: usize) -> Self {
        Self { focused: None, scroller: Scroller::new(scroll_step) }
    }

    pub fn step(&mut self, step: usize) -> &mut Self {
        self.scroller.step(step);
        self
    }

    pub fn length(&mut self, content_length: usize, viewport_content_length: usize) -> &mut Self {
        self.scroller.length(content_length, viewport_content_length);
        if let Some(focused) = self.focused
            && focused >= content_length
        {
            self.focused = Some(content_length.saturating_sub(1));
        }
        self
    }

    /// Iterate over visible items, returning (item, is_focused, area)
    ///
    /// # Arguments
    ///
    /// - `records`: all items
    /// - `height`: height of each item
    /// - `col_areas`: pre-calculated column areas
    ///
    /// # Returns
    ///
    /// - Iterator over (item, is_focused, area)
    pub fn iter_visible<'a, T>(
        &self,
        records: &'a [T],
        height: u16,
        col_areas: Rc<[Rect]>,
    ) -> impl Iterator<Item = (&'a T, bool, Rect)> {
        let cols = col_areas.len();
        let visible = &records[self.scroller.pos()..self.scroller.end_pos()];
        visible.iter().enumerate().map(move |(idx, child)| {
            let row = (idx / cols) as u16;
            let col = idx % cols;

            // Calculate card area
            let mut rect = col_areas[col];
            rect.y += row * height;
            rect.height = height;

            let focused = {
                let idx = self.scroller.pos() + idx;
                self.focused.is_some_and(|v| v == idx)
            };
            (child, focused, rect)
        })
    }

    /// Ensure there is at least one focusable item
    ///
    /// # Returns
    ///
    /// - `true` if there is at least one focusable item, `false` otherwise
    pub fn ensure_focusable(&mut self) -> bool {
        if self.scroller.content_length() == 0 {
            self.focused = None;
            false
        } else {
            true
        }
    }

    pub fn first(&mut self) {
        if !self.ensure_focusable() {
            return;
        }
        self.focused = Some(0);
        self.scroller.first();
    }

    pub fn last(&mut self) {
        if !self.ensure_focusable() {
            return;
        }
        self.focused = Some(self.scroller.content_length().saturating_sub(1));
        self.scroller.last();
    }

    pub fn next(&mut self, step: usize) {
        if !self.ensure_focusable() {
            return;
        }
        match self.focused {
            None => self.focused = Some(self.scroller.pos()),
            Some(focused) => {
                let focused = focused
                    .saturating_add(step)
                    .min(self.scroller.content_length().saturating_sub(1));
                self.focused = Some(focused);
                if focused >= self.scroller.end_pos() {
                    self.scroller.next();
                }
            }
        }
    }

    pub fn prev(&mut self, step: usize) {
        if !self.ensure_focusable() {
            return;
        }
        match self.focused {
            None => self.focused = Some(self.scroller.end_pos() - 1),
            Some(focused) => {
                let focused = focused.saturating_sub(step);
                self.focused = Some(focused);
                if focused < self.scroller.pos() {
                    self.scroller.prev();
                }
            }
        }
    }

    pub fn page_down(&mut self) {
        if !self.ensure_focusable() {
            return;
        }
        match self.focused {
            None => self.focused = Some(self.scroller.pos()),
            Some(focused) => {
                self.focused = Some(
                    focused
                        .saturating_add(self.scroller.viewport_content_length())
                        .min(self.scroller.content_length().saturating_sub(1)),
                );
                self.scroller.page_down();
            }
        }
    }

    pub fn page_up(&mut self) {
        if self.scroller.content_length() == 0 {
            self.focused = None;
            return;
        }
        match self.focused {
            None => self.focused = Some(self.scroller.end_pos() - 1),
            Some(focused) => {
                self.focused = Some(
                    focused
                        .saturating_sub(self.scroller.viewport_content_length())
                        .min(self.scroller.content_length().saturating_sub(1)),
                );
                self.scroller.page_up();
            }
        }
    }

    /// Handle key events
    ///
    /// # Arguments
    ///
    /// - `horizontal`: whether horizontal navigation is enabled
    /// - `key`: the key event to handle
    ///
    /// # Returns
    ///
    /// - `true` if the key was handled, `false` otherwise
    pub fn handle_key_event(&mut self, horizontal: bool, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('g') => self.first(),
            KeyCode::Char('G') => self.last(),
            KeyCode::Char('j') | KeyCode::Down => self.next(self.scroller.step_value()),
            KeyCode::Char('k') | KeyCode::Up => self.prev(self.scroller.step_value()),
            KeyCode::Char('h') | KeyCode::Left if horizontal => self.prev(1),
            KeyCode::Char('l') | KeyCode::Right if horizontal => self.next(1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.page_down(),
            KeyCode::PageUp => self.page_up(),
            _ => return false,
        }

        true
    }

    #[inline]
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.scroller.render(frame, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero() {
        let mut navigator = ScrollableNavigator::new(2);
        navigator.last();
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (0, 0));
        assert_eq!(navigator.focused, None);
    }

    #[test]
    fn test_page_up() {
        let mut navigator = ScrollableNavigator::new(2);
        navigator.scroller.length(20, 4);
        navigator.last();
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (16, 20));
        assert_eq!(navigator.focused, Some(19));

        // should init to last
        navigator.focused = None;
        navigator.page_up();
        assert_eq!(navigator.focused, Some(19));
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (16, 20));

        navigator.page_up();
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (12, 16));
        assert_eq!(navigator.focused, Some(15));
        navigator.page_up();
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (8, 12));
        assert_eq!(navigator.focused, Some(11));

        navigator.prev(1);
        navigator.page_up();
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (4, 8));
        assert_eq!(navigator.focused, Some(6));
    }

    #[test]
    fn test_page_down() {
        let mut navigator = ScrollableNavigator::new(2);
        navigator.scroller.length(20, 4);

        // should init to first
        navigator.focused = None;
        navigator.page_down();
        assert_eq!(navigator.focused, Some(0));

        navigator.page_down();
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (4, 8));
        assert_eq!(navigator.focused, Some(4));
        navigator.page_down();
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (8, 12));
        assert_eq!(navigator.focused, Some(8));

        navigator.next(1);
        navigator.page_down();
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (12, 16));
        assert_eq!(navigator.focused, Some(13));
    }

    #[test]
    fn test_goto() {
        let mut navigator = ScrollableNavigator::new(2);
        navigator.scroller.length(20, 4);

        // go to last
        navigator.last();
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (16, 20));
        assert_eq!(navigator.focused, Some(19));

        // go to first
        navigator.first();
        assert_eq!(navigator.focused, Some(0));
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (0, 4));
    }

    #[test]
    fn test_prev() {
        let mut navigator = ScrollableNavigator::new(2);
        navigator.scroller.length(20, 4);
        navigator.last();
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (16, 20));
        assert_eq!(navigator.focused, Some(19));

        // should init to last
        navigator.focused = None;
        navigator.prev(1);
        assert_eq!(navigator.focused, Some(19));
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (16, 20));

        // go to prev
        navigator.prev(2);
        assert_eq!(navigator.focused, Some(17));
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (16, 20));
        navigator.prev(2);
        assert_eq!(navigator.focused, Some(15));
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (14, 18));
        navigator.prev(1);
        assert_eq!(navigator.focused, Some(14));
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (14, 18));
    }

    #[test]
    fn test_next() {
        let mut navigator = ScrollableNavigator::new(2);
        navigator.scroller.length(20, 4);
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (0, 4));
        assert_eq!(navigator.focused, None);

        // go to next
        navigator.next(1);
        assert_eq!(navigator.focused, Some(0));
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (0, 4));
        navigator.next(2);
        assert_eq!(navigator.focused, Some(2));
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (0, 4));

        // go to next, should scroll
        navigator.next(2);
        assert_eq!(navigator.focused, Some(4));
        assert_eq!((navigator.scroller.pos(), navigator.scroller.end_pos()), (2, 6));

        // should init focused to first
        navigator.focused = None;
        navigator.next(2);
        assert_eq!(navigator.focused, Some(2));
    }
}
