use color_eyre::Result;
use color_eyre::eyre::eyre;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

const DEFAULT_HL_COLOR: Color = Color::Indexed(130);

#[derive(Debug, Clone, PartialEq)]
pub enum Fragment<'a> {
    RawOwned(String),
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
    pub fn from(s: &'a str, hl_index: usize) -> Result<Self> {
        if !s.is_ascii() {
            return Err(eyre!("SegLine::from_ascii expects ASCII-only input"));
        }
        if hl_index >= s.len() {
            return Err(eyre!(
                "hl_index {} is out of bounds for string of length {}",
                hl_index,
                s.len()
            ));
        }
        debug_assert!(s.is_ascii(), "SegLine::from expects ASCII text");

        let mut parts = Vec::with_capacity(3);
        if hl_index > 0 {
            parts.push(Fragment::Raw(&s[..hl_index]));
        }
        parts.push(Fragment::Hl(&s[hl_index..hl_index + 1]));
        if hl_index + 1 < s.len() {
            parts.push(Fragment::Raw(&s[hl_index + 1..]));
        }

        Ok(HighlightedLine::new(parts))
    }

    /// Converts this `HighlightedLine` into a [`Line`], using the default highlight color
    /// [DEFAULT_HL_COLOR].
    #[inline]
    pub fn to_line(&self) -> Line<'a> {
        self.to_line_with(Style::default().fg(DEFAULT_HL_COLOR))
    }

    /// Converts this `HighlightedLine` into a [`Line`].
    pub fn to_line_with(&self, hl_style: Style) -> Line<'a> {
        Line::from(self.to_spans(hl_style))
    }

    pub fn to_spans(&self, hl_style: Style) -> Vec<Span<'a>> {
        let mut spans = Vec::with_capacity(self.parts.len());
        for seg in &self.parts {
            match seg {
                Fragment::RawOwned(s) if !s.is_empty() => spans.push(Span::raw(s.clone())),
                Fragment::Raw(s) if !s.is_empty() => spans.push(Span::raw(*s)),
                Fragment::Hl(s) if !s.is_empty() => spans.push(Span::styled(*s, hl_style)),
                _ => {}
            }
        }
        spans
    }
}

impl<'a> From<HighlightedLine<'a>> for Line<'a> {
    fn from(value: HighlightedLine<'a>) -> Self {
        value.to_line()
    }
}

impl<'a> IntoIterator for HighlightedLine<'a> {
    type Item = Span<'a>;
    type IntoIter = std::vec::IntoIter<Span<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.to_spans(Style::default().fg(DEFAULT_HL_COLOR)).into_iter()
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
        let line = hl.to_line();
        assert_eq!(line.spans.len(), 2);

        assert_eq!(line.spans[0].content, "f");
        assert_eq!(line.spans[0].style.fg, Some(DEFAULT_HL_COLOR));

        assert_eq!(line.spans[1].content, "ilter");
        assert_eq!(line.spans[1].style.fg, None);
    }

    #[test]
    fn test_to_line_nonascii() {
        let hl = HighlightedLine::new(vec![Fragment::Hl("⁰"), Fragment::Raw("filter")]);
        let line = hl.to_line();
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
