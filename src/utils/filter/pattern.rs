use nucleo_matcher::pattern::{Atom as NucleoAtom, CaseMatching, Normalization};
use tracing::debug;

use super::parser;

#[derive(Debug, Clone)]
pub struct FilterPattern {
    raw: String,
    expr: FilterExpr,
}

impl FilterPattern {
    pub fn new(raw: String) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }

        let expr = FilterExpr::parse(raw)?;
        Some(Self { raw: raw.into(), expr })
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn expr(&self) -> &FilterExpr {
        &self.expr
    }
}

#[derive(Debug, Clone)]
pub enum FilterExpr {
    Legacy(NucleoAtom),
    Field { terms: Vec<FilterTerm>, fields: Vec<String> },
}

impl FilterExpr {
    fn parse(raw: &str) -> Option<Self> {
        let raw_terms = match parser::parse_filter_expr(raw) {
            Ok(terms) if terms.is_empty() => return None,
            Ok(terms) => terms,
            Err(e) => {
                debug!(?e, "failed to parse filter expression, falling back to legacy filter");
                return Self::legacy(raw);
            }
        };

        debug!(?raw_terms, "parsed filter expression terms");
        let fields = raw_terms.iter().filter_map(|term| term.field.clone()).collect::<Vec<_>>();
        if fields.is_empty() && !raw_terms.iter().any(|term| term.quoted) {
            return Self::legacy(raw);
        }

        let terms = raw_terms.into_iter().filter_map(FilterTerm::parse).collect();
        Some(Self::Field { terms, fields })
    }

    fn legacy(raw: &str) -> Option<Self> {
        parse_atom(raw).map(Self::Legacy)
    }
}

#[derive(Debug, Clone)]
pub struct FilterTerm {
    pub field: Option<String>,
    pub atom: NucleoAtom,
}

impl FilterTerm {
    fn parse(term: parser::Term) -> Option<Self> {
        let atom = parse_atom(&term.pattern)?;
        Some(FilterTerm { field: term.field, atom })
    }
}

fn parse_atom(pattern: &str) -> Option<NucleoAtom> {
    let atom = NucleoAtom::parse(pattern, CaseMatching::Smart, Normalization::Smart);
    (!atom.needle_text().is_empty()).then_some(atom)
}

#[cfg(test)]
mod tests {
    use super::*;

    enum ExpectedExpr {
        None,
        Legacy,
        Field,
    }

    #[test]
    fn filter_pattern_cases() {
        let cases = [
            ("  Host:google  ", Some("Host:google"), ExpectedExpr::Field),
            ("foo bar", Some("foo bar"), ExpectedExpr::Legacy),
            (r#""com:443""#, Some(r#""com:443""#), ExpectedExpr::Field),
            (r#""global expr" tail"#, Some(r#""global expr" tail"#), ExpectedExpr::Field),
            (r#""!global expr" tail"#, Some(r#""!global expr" tail"#), ExpectedExpr::Field),
            ("Host:google direct", Some("Host:google direct"), ExpectedExpr::Field),
            ("Host:", Some("Host:"), ExpectedExpr::Field),
            (r#"Host:"google"#, Some(r#"Host:"google"#), ExpectedExpr::Legacy),
            ("   ", None, ExpectedExpr::None),
        ];

        for (raw, expected_raw, expected_expr) in cases {
            let pattern = FilterPattern::new(raw.to_owned());

            match expected_expr {
                ExpectedExpr::None => assert!(pattern.is_none(), "input: {raw:?}"),
                ExpectedExpr::Legacy => {
                    let pattern = pattern.unwrap_or_else(|| panic!("input: {raw:?}"));
                    assert_eq!(pattern.raw(), expected_raw.unwrap(), "input: {raw:?}");
                    assert!(matches!(pattern.expr(), FilterExpr::Legacy(_)), "input: {raw:?}");
                }
                ExpectedExpr::Field => {
                    let pattern = pattern.unwrap_or_else(|| panic!("input: {raw:?}"));
                    assert_eq!(pattern.raw(), expected_raw.unwrap(), "input: {raw:?}");
                    assert!(matches!(pattern.expr(), FilterExpr::Field { .. }), "input: {raw:?}");
                }
            }
        }
    }

    #[test]
    fn quoted_filter_uses_term_boundaries() {
        let cases: &[(&str, &[(&str, bool)])] = &[
            (r#""com:443""#, &[("com:443", false)]),
            (r#""global expr" tail"#, &[("global expr", false), ("tail", false)]),
            (r#""!global expr" tail"#, &[("global expr", true), ("tail", false)]),
        ];

        for (raw, expected_terms) in cases {
            let pattern = FilterPattern::new((*raw).to_owned()).unwrap();
            let FilterExpr::Field { terms, fields } = pattern.expr() else {
                panic!("quoted pattern should preserve term boundaries: {raw:?}");
            };

            assert!(fields.is_empty(), "input: {raw:?}");
            assert_eq!(terms.len(), expected_terms.len(), "input: {raw:?}");
            for (term, (expected_needle, expected_negative)) in
                terms.iter().zip(expected_terms.iter())
            {
                assert_eq!(term.atom.needle_text().to_string(), *expected_needle, "input: {raw:?}");
                assert_eq!(term.atom.negative, *expected_negative, "input: {raw:?}");
            }
        }
    }
}
