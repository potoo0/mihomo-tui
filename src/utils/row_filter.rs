use std::borrow::Cow;
use std::sync::Arc;

use fuzzy_matcher::FuzzyMatcher;

use crate::utils::columns::ColDef;

/// An iterator that filters items based on a fuzzy pattern and column definitions
pub struct RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    iter: I,
    matcher: &'a dyn FuzzyMatcher,
    pattern: Option<&'a str>,
    cols: &'a [ColDef<T>],
}

impl<'a, T, I> RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    pub fn new(
        iter: I,
        matcher: &'a dyn FuzzyMatcher,
        pattern: Option<&'a str>,
        cols: &'a [ColDef<T>],
    ) -> Self {
        Self { iter, matcher, pattern, cols }
    }
}

impl<'a, T, I> Iterator for RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    type Item = Arc<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let pat = match self.pattern {
            Some(p) if !p.is_empty() => p,
            _ => return self.iter.next().cloned(),
        };
        while let Some(item) = self.iter.next() {
            let hit = self.cols.iter().filter(|col| col.filterable).any(|col| {
                let text: Cow<'_, str> = (col.accessor)(item);
                self.matcher.fuzzy_match(&text, pat).is_some()
            });
            if hit {
                return Some(Arc::clone(item));
            }
        }
        None
    }
}
