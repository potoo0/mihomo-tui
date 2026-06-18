use std::borrow::Cow;
use std::sync::Arc;

use nucleo_matcher::pattern::Atom as NucleoAtom;
use nucleo_matcher::{Matcher, Utf32Str};

use super::FilterExpr;
use crate::utils::columns::{ColDef, TextResolver};

/// An iterator that filters items based on a fuzzy pattern and column definitions
pub struct RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    iter: I,
    matcher: &'a mut Matcher,
    terms: Option<Vec<RowFilterTerm<'a, T>>>,
    haystack_buffer: Vec<char>,
    text_resolver: Option<&'a dyn TextResolver<T>>,
}

struct RowFilterTerm<'a, T> {
    atom: &'a NucleoAtom,
    cols: Vec<&'a ColDef<T>>,
    require_cols: bool,
}

impl<'a, T, I> RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    pub fn new<C, D>(
        iter: I,
        matcher: &'a mut Matcher,
        pattern: Option<&'a FilterExpr>,
        cols: C,
    ) -> Self
    where
        C: IntoIterator<Item = &'a D>,
        D: AsRef<ColDef<T>> + 'a,
    {
        let haystack_buffer = Vec::new();
        let cols = cols.into_iter().map(AsRef::as_ref).collect::<Vec<_>>();
        let terms = pattern.map(|filter| Self::compile_terms(filter, &cols));
        Self { iter, matcher, terms, haystack_buffer, text_resolver: None }
    }

    pub fn with_text_resolver(mut self, resolver: &'a dyn TextResolver<T>) -> Self {
        self.text_resolver = Some(resolver);
        self
    }

    fn compile_terms(filter: &'a FilterExpr, cols: &[&'a ColDef<T>]) -> Vec<RowFilterTerm<'a, T>> {
        match filter {
            FilterExpr::Legacy(atom) => vec![RowFilterTerm {
                atom,
                cols: cols.iter().copied().filter(|col| col.filterable).collect(),
                require_cols: false,
            }],
            FilterExpr::Field { terms, fields } => terms
                .iter()
                .map(|term| RowFilterTerm {
                    atom: &term.atom,
                    cols: cols
                        .iter()
                        .copied()
                        .filter(|col| match term.field.as_deref() {
                            Some(field) => col.title.eq_ignore_ascii_case(field),
                            None => {
                                col.filterable
                                    && !fields
                                        .iter()
                                        .any(|field| col.title.eq_ignore_ascii_case(field))
                            }
                        })
                        .collect(),
                    require_cols: true,
                })
                .collect(),
        }
    }

    fn matches(&mut self, terms: &[RowFilterTerm<T>], item: &T) -> bool {
        terms.iter().all(|term| self.matches_term(term, item))
    }

    fn matches_term(&mut self, term: &RowFilterTerm<T>, item: &T) -> bool {
        if term.require_cols && term.cols.is_empty() {
            return false;
        }

        if term.atom.negative {
            term.cols.iter().all(|col| self.matches_col(term.atom, col, item))
        } else {
            term.cols.iter().any(|col| self.matches_col(term.atom, col, item))
        }
    }

    fn matches_col(&mut self, atom: &NucleoAtom, col: &ColDef<T>, item: &T) -> bool {
        let text = Self::text(self.text_resolver, col, item);
        atom.score(Utf32Str::new(&text, &mut self.haystack_buffer), self.matcher).is_some()
    }

    fn text<'row>(
        text_resolver: Option<&dyn TextResolver<T>>,
        col: &ColDef<T>,
        item: &'row T,
    ) -> Cow<'row, str> {
        let text = (col.accessor)(item);
        match text_resolver {
            Some(resolver) => resolver.resolve(col, item, text),
            None => text,
        }
    }
}

impl<'a, T, I> Iterator for RowFilter<'a, T, I>
where
    I: Iterator<Item = &'a Arc<T>>,
{
    type Item = Arc<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let terms = match self.terms.take() {
            Some(terms) => terms,
            _ => return self.iter.next().cloned(),
        };

        loop {
            let Some(item) = self.iter.next() else {
                self.terms = Some(terms);
                return None;
            };

            if self.matches(&terms, item) {
                self.terms = Some(terms);
                return Some(Arc::clone(item));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::sync::Arc;

    use nucleo_matcher::pattern::{Atom, CaseMatching, Normalization};
    use nucleo_matcher::{Matcher, Utf32Str};

    use super::*;
    use crate::utils::filter::FilterPattern;

    struct Row {
        id: &'static str,
        host: &'static str,
        rule: &'static str,
        hidden: &'static str,
    }

    fn row_filter_ids(pattern: &str) -> Vec<&'static str> {
        let rows = [
            Arc::new(Row { id: "1", host: "google.com", rule: "DIRECT", hidden: "secret" }),
            Arc::new(Row { id: "2", host: "openai.com", rule: "PROXY", hidden: "secret" }),
        ];
        let cols = row_cols();
        let pattern = FilterPattern::new(pattern.to_owned());
        let mut matcher = Matcher::default();

        RowFilter::new(rows.iter(), &mut matcher, pattern.as_ref().map(FilterPattern::expr), &cols)
            .map(|row| row.id)
            .collect()
    }

    fn row_cols() -> [ColDef<Row>; 3] {
        [
            ColDef {
                id: "host_id",
                title: "Host",
                filterable: true,
                sortable: false,
                accessor: |row| Cow::Borrowed(row.host),
                sort_key: None,
            },
            ColDef {
                id: "rule_id",
                title: "Rule",
                filterable: true,
                sortable: false,
                accessor: |row| Cow::Borrowed(row.rule),
                sort_key: None,
            },
            ColDef {
                id: "hidden_id",
                title: "Hidden",
                filterable: false,
                sortable: false,
                accessor: |row| Cow::Borrowed(row.hidden),
                sort_key: None,
            },
        ]
    }

    #[test]
    fn field_filter_cases() {
        let cases = [
            ("host:google", vec!["1"]),
            ("Host:google", vec!["1"]),
            ("HOST:google", vec!["1"]),
            ("host_id:google", vec![]),
            ("Host:!google", vec!["2"]),
            ("Host:google Rule:direct", vec!["1"]),
            ("Host:google Rule:proxy", vec![]),
            ("Host:google direct", vec!["1"]),
            ("Host:google google", vec![]),
            ("Host:", vec!["1", "2"]),
            ("secret", vec![]),
            ("Hidden:secret", vec!["1", "2"]),
            ("Unknown:google", vec![]),
        ];

        for (pattern, expected) in cases {
            assert_eq!(row_filter_ids(pattern), expected, "pattern: {pattern:?}");
        }
    }

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
