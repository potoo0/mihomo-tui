use std::borrow::Cow;

use color_eyre::Result;
use color_eyre::eyre::eyre;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

const DEFAULT_HL_COLOR: Color = Color::Indexed(130);

#[derive(Debug, Clone, PartialEq)]
pub enum Fragment<'a> {
    RawOwned(String),
    HlOwned(String),
    Raw(&'a str),
    Hl(&'a str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HighlightedLine<'a> {
    parts: Vec<Fragment<'a>>,
}

impl<'a> HighlightedLine<'a> {
    pub fn new(parts: Vec<Fragment<'a>>) -> Self {
        Self { parts }
    }

    /// Creates a `HighlightedLine` from `s`, highlighting the character at `hl_index`.
    ///
    /// # Notes
    /// - **ASCII-only**: treats `hl_index` as both byte and char index (single-byte chars).
    pub fn from<S: Into<Cow<'a, str>>>(s: S, hl_idx: usize) -> Result<Self> {
        let s: Cow<'a, str> = s.into();
        if !s.is_ascii() {
            return Err(eyre!("HighlightedLine::from expects ASCII text"));
        }
        let len = s.len();
        if hl_idx >= s.len() {
            return Err(eyre!("hl_index {} is out of bounds for string of length {}", hl_idx, len));
        }

        let pivot = hl_idx + 1;
        let mut parts = Vec::with_capacity(3);
        match s {
            Cow::Borrowed(text) => {
                if hl_idx > 0 {
                    parts.push(Fragment::Raw(&text[..hl_idx]));
                }
                parts.push(Fragment::Hl(&text[hl_idx..pivot]));
                if pivot < len {
                    parts.push(Fragment::Raw(&text[pivot..]));
                }
            }
            Cow::Owned(text) => {
                if hl_idx > 0 {
                    parts.push(Fragment::RawOwned(text[..hl_idx].to_owned()));
                }
                parts.push(Fragment::HlOwned(text[hl_idx..pivot].to_owned()));
                if pivot < len {
                    parts.push(Fragment::RawOwned(text[pivot..].to_owned()));
                }
            }
        }

        Ok(Self::new(parts))
    }

    /// Converts this `HighlightedLine` into a [`Line`], using the default highlight color
    /// [DEFAULT_HL_COLOR].
    #[inline]
    pub fn into_line(self) -> Line<'a> {
        self.into_line_styled(Style::default().fg(DEFAULT_HL_COLOR))
    }

    /// Converts this `HighlightedLine` into a [`Line`].
    pub fn into_line_styled(self, hl_style: Style) -> Line<'a> {
        Line::from(self.into_spans(hl_style))
    }

    pub fn into_spans(self, hl_style: Style) -> Vec<Span<'a>> {
        self.parts
            .into_iter()
            .map(|v| match v {
                Fragment::RawOwned(s) if !s.is_empty() => Span::raw(s),
                Fragment::HlOwned(s) if !s.is_empty() => Span::styled(s, hl_style),
                Fragment::Raw(s) if !s.is_empty() => Span::raw(s),
                Fragment::Hl(s) if !s.is_empty() => Span::styled(s, hl_style),
                _ => Span::raw(""),
            })
            .collect()
    }
}

impl<'a> From<HighlightedLine<'a>> for Line<'a> {
    fn from(value: HighlightedLine<'a>) -> Self {
        value.into_line()
    }
}

impl<'a> IntoIterator for HighlightedLine<'a> {
    type Item = Span<'a>;
    type IntoIter = std::vec::IntoIter<Span<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_spans(Style::default().fg(DEFAULT_HL_COLOR)).into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foo() {
        let hl = HighlightedLine::new(vec![Fragment::Hl("f"), Fragment::Raw("ilter")]);
        let l: Line = hl.into();
        println!("{l:?}");
    }

    #[test]
    fn test_to_line() {
        let hl = HighlightedLine::new(vec![Fragment::Hl("f"), Fragment::Raw("ilter")]);
        let line = hl.into_line();
        assert_eq!(line.spans.len(), 2);

        assert_eq!(line.spans[0].content, "f");
        assert_eq!(line.spans[0].style.fg, Some(DEFAULT_HL_COLOR));

        assert_eq!(line.spans[1].content, "ilter");
        assert_eq!(line.spans[1].style.fg, None);
    }

    #[test]
    fn test_to_line_nonascii() {
        let hl = HighlightedLine::new(vec![Fragment::Hl("⁰"), Fragment::Raw("filter")]);
        let line = hl.into_line();
        assert_eq!(line.spans.len(), 2);

        assert_eq!(line.spans[0].content, "⁰");
        assert_eq!(line.spans[0].style.fg, Some(DEFAULT_HL_COLOR));

        assert_eq!(line.spans[1].content, "filter");
        assert_eq!(line.spans[1].style.fg, None);
    }

    #[test]
    fn test_from() {
        // beginning
        let hl = HighlightedLine::from("filter", 0).unwrap();
        assert_eq!(hl.parts, vec![Fragment::Hl("f"), Fragment::Raw("ilter")]);

        // middle
        let hl = HighlightedLine::from("filter", 3).unwrap();
        assert_eq!(hl.parts, vec![Fragment::Raw("fil"), Fragment::Hl("t"), Fragment::Raw("er")]);

        // end
        let hl = HighlightedLine::from("filter", 5).unwrap();
        assert_eq!(hl.parts, vec![Fragment::Raw("filte"), Fragment::Hl("r")]);

        // illegal
        let hl = HighlightedLine::from("⁰filter", 0);
        assert!(hl.is_err());
        let hl = HighlightedLine::from("filter", 100);
        assert!(hl.is_err());
    }
}
