use std::borrow::Cow;

use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

#[derive(Default)]
pub struct Matcher {
    fuzzy_matcher: SkimMatcherV2,
}

#[derive(Debug, PartialEq)]
pub enum PatternKind {
    Empty,
    Prefix,
    Suffix,
    Exact,
    Substring,
    Fuzzy,
}

/// A pattern that can be applied to a string, with optional inverse logic.
///
/// # Symbols
/// - `^`: Prefix match
/// - `$`: Suffix match
/// - `'`: Substring
/// - _ : Fuzzy match
///
/// # Inverse
/// If `inverse` is true, the match is negated.
#[derive(Debug, PartialEq)]
pub struct Pattern<'a> {
    needle: Cow<'a, str>,
    kind: PatternKind,
    inverse: bool,
    case_sensitive: bool,
}

impl Matcher {
    pub fn matches(&self, haystack: &str, pattern: &Pattern) -> bool {
        let hit = match pattern.kind {
            PatternKind::Empty => true,
            PatternKind::Prefix => self.prefix_match(haystack, pattern),
            PatternKind::Suffix => self.suffix_match(haystack, pattern),
            PatternKind::Exact => self.exact_match(haystack, pattern),
            PatternKind::Substring => self.substring_match(haystack, pattern),
            PatternKind::Fuzzy => self.fuzzy_match(haystack, pattern),
        };
        hit ^ pattern.inverse
    }

    pub fn prefix_match(&self, haystack: &str, pat: &Pattern) -> bool {
        if pat.case_sensitive {
            return haystack.starts_with(pat.needle.as_ref());
        }
        let h = haystack.as_bytes();
        let p = pat.needle.as_bytes();
        h.len() >= p.len() && h[..p.len()].eq_ignore_ascii_case(p)
    }

    pub fn suffix_match(&self, haystack: &str, pat: &Pattern) -> bool {
        if pat.case_sensitive {
            return haystack.ends_with(pat.needle.as_ref());
        }
        let h = haystack.as_bytes();
        let p = pat.needle.as_bytes();
        h.len() >= p.len() && h[h.len() - p.len()..].eq_ignore_ascii_case(p)
    }

    pub fn exact_match(&self, haystack: &str, pat: &Pattern) -> bool {
        if pat.case_sensitive {
            return haystack == pat.needle.as_ref();
        }
        haystack.eq_ignore_ascii_case(pat.needle.as_ref())
    }

    pub fn substring_match(&self, haystack: &str, pat: &Pattern) -> bool {
        if pat.case_sensitive {
            return haystack.contains(pat.needle.as_ref());
        }
        let h = haystack.as_bytes();
        let p = pat.needle.as_bytes();
        h.windows(p.len()).any(|w| w.eq_ignore_ascii_case(p))
    }

    pub fn fuzzy_match(&self, haystack: &str, pat: &Pattern) -> bool {
        self.fuzzy_matcher.fuzzy_match(haystack, pat.needle.as_ref()).is_some()
    }
}

impl<'a> Pattern<'a> {
    pub fn new(
        needle: impl Into<Cow<'a, str>>,
        kind: PatternKind,
        inverse: bool,
        case_sensitive: bool,
    ) -> Self {
        Self { needle: needle.into(), kind, inverse, case_sensitive }
    }

    pub fn is_empty(&self) -> bool {
        self.kind == PatternKind::Empty
    }

    /// Parses a pattern from the input string.
    /// Returns `None` if no effective pattern is found.
    pub fn parse(input: &'a str) -> Option<Self> {
        let mut raw = input.trim();
        if raw.is_empty() {
            return None;
        }

        let inverse = match raw.as_bytes() {
            [b'!', ..] => {
                raw = &raw[1..];
                true
            }
            [b'\\', b'!', ..] => {
                raw = &raw[1..];
                false
            }
            _ => false,
        };
        let case_sensitive = Pattern::contains_upper(raw);
        let mut kind = match raw.as_bytes() {
            [b'^', ..] => {
                raw = &raw[1..];
                PatternKind::Prefix
            }
            [b'\'', ..] => {
                raw = &raw[1..];
                PatternKind::Substring
            }
            [b'\\', b'^' | b'\'', ..] => {
                raw = &raw[1..];
                PatternKind::Fuzzy
            }
            _ => PatternKind::Fuzzy,
        };
        let needle = match raw.as_bytes() {
            [.., b'\\', b'$'] => {
                let mut s = String::with_capacity(raw.len() - 1);
                s.push_str(&raw[..raw.len() - 2]);
                s.push('$');
                Cow::Owned(s)
            }
            [.., b'$'] => {
                kind = if kind == PatternKind::Fuzzy {
                    PatternKind::Suffix
                } else {
                    PatternKind::Exact
                };
                Cow::Borrowed(&raw[..raw.len() - 1])
            }
            _ => Cow::Borrowed(raw),
        };
        if needle.is_empty() {
            return None;
        }

        Some(Self::new(needle, kind, inverse, case_sensitive))
    }

    fn contains_upper(string: &str) -> bool {
        for ch in string.chars() {
            if ch.is_ascii_uppercase() {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches() {
        let haystack =
            "[TCP] 198.18.0.1:4216 --> ab.chatgpt.com:443 match RuleSet(ai) using AI 新加坡①";
        let matcher = Matcher::default();
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
            ("!matchi", false),
            ("abcd", false),
            ("!abcd", true),
            ("RuleSet", true),
            ("ruleset", true),
            ("Ruleset", false),
            // exact match
            ("^[TCP]坡①$", false),
            ("!^[TCP]坡①$", true),
        ];
        for (pattern, expected) in cases {
            let pat = Pattern::parse(pattern)
                .unwrap_or_else(|| panic!("parse failed on pattern {:?}", pattern));
            assert_eq!(
                matcher.matches(haystack, &pat),
                expected,
                "match failed on pattern {:?}",
                pattern
            );
        }
    }

    #[test]
    fn test_pattern_parse() {
        use PatternKind::*;
        let cases = [
            // empty pattern
            ("", None),
            ("  ", None),
            ("!", None),
            ("^", None),
            ("'", None),
            ("$", None),
            // parse ^
            ("^nginx", Some(Pattern::new("nginx", Prefix, false, false))),
            (r"\^nginx", Some(Pattern::new("^nginx", Fuzzy, false, false))),
            ("!^nginx", Some(Pattern::new("nginx", Prefix, true, false))),
            (r"\!^nginx", Some(Pattern::new("!^nginx", Fuzzy, false, false))),
            // parse $
            ("nginx$", Some(Pattern::new("nginx", Suffix, false, false))),
            (r"nginx\$", Some(Pattern::new("nginx$", Fuzzy, false, false))),
            ("!nginx$", Some(Pattern::new("nginx", Suffix, true, false))),
            // parse '
            ("'nginx", Some(Pattern::new("nginx", Substring, false, false))),
            (r"\'nginx", Some(Pattern::new("'nginx", Fuzzy, false, false))),
            ("nginx", Some(Pattern::new("nginx", Fuzzy, false, false))),
            // parse ^ and $
            ("^nginx$", Some(Pattern::new("nginx", Exact, false, false))),
            ("!^nginx$", Some(Pattern::new("nginx", Exact, true, false))),
            // parse case sensitive
            ("Nginx", Some(Pattern::new("Nginx", Fuzzy, false, true))),
            ("^Nginx", Some(Pattern::new("Nginx", Prefix, false, true))),
            ("!Nginx$", Some(Pattern::new("Nginx", Suffix, true, true))),
        ];

        for (input, expected) in cases {
            assert_eq!(Pattern::parse(input), expected, "failed on input {:?}", input);
        }
    }
}
