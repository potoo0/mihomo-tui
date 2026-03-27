use std::borrow::Cow;
use std::sync::Arc;

use nucleo_matcher::pattern::{Atom, CaseMatching, Normalization};
use nucleo_matcher::{Matcher, Utf32Str};

use crate::utils::columns::ColDef;

/// An iterator that filters items based on a fuzzy pattern and column definitions
pub struct RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    iter: I,
    matcher: &'a mut Matcher,
    pattern: Option<Atom>,
    haystack_buffer: Vec<char>,
    cols: &'a [ColDef<T>],
}

impl<'a, T, I> RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    pub fn new(
        iter: I,
        matcher: &'a mut Matcher,
        pattern: Option<&'a str>,
        cols: &'a [ColDef<T>],
    ) -> Self {
        let pattern = pattern.and_then(|p| {
            let atom = Atom::parse(p, CaseMatching::Smart, Normalization::Smart);
            if atom.needle_text().is_empty() { None } else { Some(atom) }
        });
        let haystack_buffer = Vec::new();
        Self { iter, matcher, pattern, haystack_buffer, cols }
    }
}

impl<'a, T, I> Iterator for RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    type Item = Arc<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let pat = match self.pattern.as_ref() {
            Some(p) => p,
            _ => return self.iter.next().cloned(),
        };
        while let Some(item) = self.iter.next() {
            let hit = self.cols.iter().filter(|col| col.filterable).any(|col| {
                let text: Cow<'_, str> = (col.accessor)(item);
                pat.score(Utf32Str::new(&text, &mut self.haystack_buffer), self.matcher).is_some()
            });
            if hit {
                return Some(Arc::clone(item));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use nucleo_matcher::pattern::{Atom, CaseMatching, Normalization};
    use nucleo_matcher::{Matcher, Utf32Str};

    #[test]
    fn test_matcher() {
        let text =
            "[TCP] 198.18.0.1:4216 --> ab.chatgpt.com:443 match RuleSet(ai) using AI 新加坡①";
        let mut matcher = Matcher::default();
        let cases = [
            // prefix match
            ("^[TCP]", true),
            (r"\^[TCP]", false),
            ("!^[TCP]", false),
            (r"\!^[TCP]", false),
            ("^[tcp]", true),
            ("^[TCp]", false),
            // suffix match
            ("坡①$", true),
            (r"坡①\$", false),
            ("!坡①$", false),
            ("坡①1$", false),
            ("!坡①1$", true),
            ("AI 新加坡①$", true),
            ("ai 新加坡①$", true),
            ("aI 新加坡①$", false),
            // substring match
            ("'match", true),
            (r"\'match", false),
            ("'matchi", false),
            (r"!'matchi", true),
            ("'RuleSet", true),
            ("'ruleset", true),
            ("'Ruleset", false),
            // fuzzy match
            ("matchi", true),
            // ("!matchi", false),
            ("abcd", false),
            ("!abcd", true),
            ("RuleSet", true),
            ("ruleset", true),
            ("Ruleset", false),
            // exact match
            ("^[TCP]坡①$", false),
            ("!^[TCP]坡①$", true),
        ];
        let mut buf = Vec::new();
        for (pat, expected) in cases {
            let atom = Atom::parse(pat, CaseMatching::Smart, Normalization::Smart);
            if atom.needle_text().is_empty() {
                assert!(!expected, "Pattern: {pat}");
                return;
            }
            let matched = atom.score(Utf32Str::new(text, &mut buf), &mut matcher).is_some();
            println!("Pattern buf cap: {}, size: {}", buf.capacity(), buf.len());
            assert_eq!(matched, expected, "match failed on pattern {:?}", pat);
        }
    }
}
