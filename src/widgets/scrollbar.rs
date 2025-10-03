use ratatui::widgets::ScrollbarState;

#[derive(Debug)]
pub struct ScrollState {
    step: usize,
    position: usize,
    content_length: usize,
    viewport_content_length: usize,

    pub state: ScrollbarState,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new(1)
    }
}

impl ScrollState {
    pub fn new(step: usize) -> Self {
        Self {
            step,
            position: 0,
            content_length: 0,
            viewport_content_length: 0,
            state: Default::default(),
        }
    }

    pub fn step(&mut self, step: usize) -> &mut Self {
        self.step = step;
        self
    }

    pub fn position(&mut self, position: usize) -> &mut Self {
        self.position = position;
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

    fn align_up(&self, value: usize) -> usize {
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
            let pos = self.position.saturating_add(self.step).min(self.align_up(max_pos));
            self.position(pos);
        }
    }

    pub fn prev(&mut self) {
        self.position(self.position.saturating_sub(self.step));
    }

    pub fn pos(&self) -> usize {
        self.position
    }

    pub fn end_pos(&self) -> usize {
        self.position.saturating_add(self.viewport_content_length).min(self.content_length)
    }

    pub fn content_length(&self) -> usize {
        self.content_length
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
        let mut scroll = ScrollState::new(2);
        scroll.length(13, 10);

        let expected = vec![(0, 10), (2, 12), (4, 13), (4, 13), (4, 13)];
        for pair in expected.into_iter() {
            assert_eq!((scroll.pos(), scroll.end_pos()), pair);
            scroll.next();
        }
    }

    #[test]
    fn test_scroll_state() {
        let mut scroll = ScrollState::new(2);
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
