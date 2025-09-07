use crate::models::sort::{SortDir, SortSpec};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SearchState {
    pub pattern: Option<String>,
    pub sort: Option<SortSpec>,
    /// Maximum number of sortable columns, for column navigation
    pub max_cols: usize,
}

impl SearchState {
    pub fn new(max_cols: usize) -> Self {
        Self { pattern: None, sort: None, max_cols }
    }

    pub fn sort_rev(&mut self) {
        if let Some(ob) = self.sort.as_mut() {
            ob.dir = ob.dir.toggle();
        }
    }

    pub fn sort_next(&mut self) {
        if self.max_cols == 0 {
            return;
        }
        if let Some(s) = self.sort.as_mut() {
            s.col = (s.col + 1) % self.max_cols;
        } else {
            self.sort = Some(SortSpec { col: 0, dir: Default::default() });
        }
    }

    pub fn sort_prev(&mut self) {
        if self.max_cols == 0 {
            return;
        }
        if let Some(s) = self.sort.as_mut() {
            s.col = (s.col + self.max_cols - 1) % self.max_cols;
        } else {
            self.sort = Some(SortSpec { col: self.max_cols - 1, dir: SortDir::Asc });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_navigation() {
        let mut state = SearchState::new(3);
        assert_eq!(state.sort, None);

        // Test next
        for idx in 0..3 {
            state.sort_next();
            assert_eq!(state.sort.map(|v| v.col), Some(idx));
        }
        // wrap around to first sortable column
        state.sort_next();
        assert_eq!(state.sort.map(|v| v.col), Some(0));

        // Reset
        state.sort = None;
        // Test prev
        for idx in (0..3).rev() {
            state.sort_prev();
            assert_eq!(state.sort.map(|v| v.col), Some(idx));
        }
        // wrap around to last sortable column
        state.sort_prev();
        assert_eq!(state.sort.map(|v| v.col), Some(2));
    }
}
