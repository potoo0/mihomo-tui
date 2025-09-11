use color_eyre::Result;
use color_eyre::eyre::eyre;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

const DEFAULT_HL_COLOR: Color = Color::Indexed(130);

#[derive(Debug, Clone, PartialEq)]
pub enum Fragment {
    Raw(Box<str>),
    Hl(Box<str>),
}

impl Fragment {
    #[inline]
    pub fn raw<S: Into<Box<str>>>(s: S) -> Self {
        Self::Raw(s.into())
    }

    #[inline]
    pub fn hl<S: Into<Box<str>>>(s: S) -> Self {
        Self::Hl(s.into())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shortcut {
    parts: Vec<Fragment>,
}

impl Shortcut {
    pub fn new(parts: Vec<Fragment>) -> Self {
        Self { parts }
    }

    /// Creates a `Shortcut` from `s`, highlighting the character at `hl_index`.
    ///
    /// # Notes
    /// - **ASCII-only**: treats `hl_index` as both byte and char index (single-byte chars).
    pub fn from<S: AsRef<str>>(s: S, hl_idx: usize) -> Result<Self> {
        let text = s.as_ref();
        if !text.is_ascii() {
            return Err(eyre!("Shortcut::from expects ASCII text"));
        }
        let len = text.len();
        if hl_idx >= text.len() {
            return Err(eyre!("hl_index {} is out of bounds for string of length {}", hl_idx, len));
        }

        let pivot = hl_idx + 1;
        let mut parts = Vec::with_capacity(3);
        if hl_idx > 0 {
            parts.push(Fragment::raw(&text[..hl_idx]));
        }
        parts.push(Fragment::hl(&text[hl_idx..pivot]));
        if pivot < len {
            parts.push(Fragment::raw(&text[pivot..]));
        }

        Ok(Self::new(parts))
    }

    /// Converts this `Shortcut` into a [`Line`].
    #[inline]
    pub fn into_line<'a>(self, hl_style: Option<Style>) -> Line<'a> {
        Line::from(self.into_spans(hl_style))
    }

    pub fn into_spans<'a>(self, hl_style: Option<Style>) -> Vec<Span<'a>> {
        let hl_style = hl_style.unwrap_or(Style::default().fg(DEFAULT_HL_COLOR));
        self.parts
            .into_iter()
            .map(|v| match v {
                Fragment::Raw(s) if !s.is_empty() => Span::raw(s.into_string()),
                Fragment::Hl(s) if !s.is_empty() => Span::styled(s.into_string(), hl_style),
                _ => Span::raw(""),
            })
            .collect()
    }

    pub fn spans(&'_ self, hl_style: Option<Style>) -> Vec<Span<'_>> {
        let hl_style = hl_style.unwrap_or(Style::default().fg(DEFAULT_HL_COLOR));
        self.parts
            .iter()
            .map(|v| match v {
                Fragment::Raw(s) if !s.is_empty() => Span::raw(s.as_ref()),
                Fragment::Hl(s) if !s.is_empty() => Span::styled(s.as_ref(), hl_style),
                _ => Span::raw(""),
            })
            .collect()
    }
}

/// Converts this `Shortcut` into a [`Line`],
/// using the default highlight color [DEFAULT_HL_COLOR].
impl<'a> From<Shortcut> for Line<'a> {
    fn from(value: Shortcut) -> Self {
        value.into_line(Some(Style::default().fg(DEFAULT_HL_COLOR)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foo() {
        let hl = Shortcut::new(vec![Fragment::hl("f"), Fragment::raw("ilter")]);
        let l: Line = hl.into();
        println!("{l:?}");
    }

    #[test]
    fn test_to_line() {
        let hl = Shortcut::new(vec![Fragment::hl("f"), Fragment::raw("ilter")]);
        let line: Line = hl.into();
        assert_eq!(line.spans.len(), 2);

        assert_eq!(line.spans[0].content, "f");
        assert_eq!(line.spans[0].style.fg, Some(DEFAULT_HL_COLOR));

        assert_eq!(line.spans[1].content, "ilter");
        assert_eq!(line.spans[1].style.fg, None);
    }

    #[test]
    fn test_to_line_nonascii() {
        let hl = Shortcut::new(vec![Fragment::hl("⁰"), Fragment::raw("filter")]);
        let line: Line = hl.into();
        assert_eq!(line.spans.len(), 2);

        assert_eq!(line.spans[0].content, "⁰");
        assert_eq!(line.spans[0].style.fg, Some(DEFAULT_HL_COLOR));

        assert_eq!(line.spans[1].content, "filter");
        assert_eq!(line.spans[1].style.fg, None);
    }

    #[test]
    fn test_from() {
        // beginning
        let hl = Shortcut::from("filter", 0).unwrap();
        assert_eq!(hl.parts, vec![Fragment::Hl("f".into()), Fragment::Raw("ilter".into())]);

        // middle
        let hl = Shortcut::from("filter", 3).unwrap();
        assert_eq!(
            hl.parts,
            vec![Fragment::Raw("fil".into()), Fragment::Hl("t".into()), Fragment::Raw("er".into())]
        );

        // end
        let hl = Shortcut::from("filter", 5).unwrap();
        assert_eq!(hl.parts, vec![Fragment::Raw("filte".into()), Fragment::Hl("r".into())]);

        // illegal
        let hl = Shortcut::from("⁰filter", 0);
        assert!(hl.is_err());
        let hl = Shortcut::from("filter", 100);
        assert!(hl.is_err());
    }
}
