use std::borrow::Cow;
use std::sync::Arc;

use crate::utils::columns::ColDef;
use crate::utils::matcher::{Matcher, Pattern};

/// An iterator that filters items based on a fuzzy pattern and column definitions
pub struct RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    iter: I,
    matcher: &'a Matcher,
    pattern: Option<Pattern<'a>>,
    cols: &'a [ColDef<T>],
}

impl<'a, T, I> RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    pub fn new(
        iter: I,
        matcher: &'a Matcher,
        pattern: Option<&'a str>,
        cols: &'a [ColDef<T>],
    ) -> Self {
        let pattern = pattern.and_then(Pattern::parse);
        Self { iter, matcher, pattern, cols }
    }
}

impl<'a, T, I> Iterator for RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    type Item = Arc<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let pat = match self.pattern.as_ref() {
            Some(p) if !p.is_empty() => p,
            _ => return self.iter.next().cloned(),
        };
        while let Some(item) = self.iter.next() {
            let hit = self.cols.iter().filter(|col| col.filterable).any(|col| {
                let text: Cow<'_, str> = (col.accessor)(item);
                self.matcher.matches(&text, pat)
            });
            if hit {
                return Some(Arc::clone(item));
            }
        }
        None
    }
}
