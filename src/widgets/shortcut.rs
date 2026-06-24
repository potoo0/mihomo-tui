use anyhow::{Result, anyhow};
use ratatui::style::{Color, Style};
use ratatui::text::Span;

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

    pub fn into_span<'a>(self, hl_style: Option<Style>) -> Span<'a> {
        match self {
            Self::Raw(s) if !s.is_empty() => Span::raw(s.into_string()),
            Self::Hl(s) if !s.is_empty() => Span::styled(
                s.into_string(),
                hl_style.unwrap_or(Style::default().fg(DEFAULT_HL_COLOR)),
            ),
            _ => Span::raw(""),
        }
    }

    pub fn span(&'_ self, hl_style: Option<Style>) -> Span<'_> {
        match self {
            Self::Raw(s) if !s.is_empty() => Span::raw(s.as_ref()),
            Self::Hl(s) if !s.is_empty() => {
                Span::styled(s.as_ref(), hl_style.unwrap_or(Style::default().fg(DEFAULT_HL_COLOR)))
            }
            _ => Span::raw(""),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutMode {
    Full,
    Compact,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shortcut {
    full: Vec<Fragment>,
    compact: Option<Vec<Fragment>>,
}

impl Shortcut {
    pub fn new(parts: Vec<Fragment>) -> Self {
        Self { full: parts, compact: None }
    }

    pub fn compact(mut self, parts: Vec<Fragment>) -> Self {
        self.compact = Some(parts);
        self
    }

    fn parts(&self, mode: ShortcutMode) -> &[Fragment] {
        match mode {
            ShortcutMode::Full => &self.full,
            ShortcutMode::Compact => self.compact.as_deref().unwrap_or(&self.full),
        }
    }

    /// Creates a `Shortcut` from `s`, highlighting the character at `hl_index`.
    ///
    /// # Notes
    /// - **ASCII-only**: treats `hl_index` as both byte and char index (single-byte chars).
    pub fn from<S: AsRef<str>>(s: S, hl_idx: usize) -> Result<Self> {
        let text = s.as_ref();
        if !text.is_ascii() {
            return Err(anyhow!("Shortcut::from expects ASCII text"));
        }
        let len = text.len();
        if hl_idx >= text.len() {
            return Err(anyhow!(
                "hl_index {} is out of bounds for string of length {}",
                hl_idx,
                len
            ));
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

    pub fn into_spans<'a>(self, hl_style: Option<Style>) -> Vec<Span<'a>> {
        self.into_spans_for(ShortcutMode::Full, hl_style)
    }

    pub fn into_spans_for<'a>(self, mode: ShortcutMode, hl_style: Option<Style>) -> Vec<Span<'a>> {
        let parts = match mode {
            ShortcutMode::Full => self.full,
            ShortcutMode::Compact => self.compact.unwrap_or(self.full),
        };
        parts.into_iter().map(|v| v.into_span(hl_style)).collect()
    }

    pub fn spans_for(&'_ self, mode: ShortcutMode, hl_style: Option<Style>) -> Vec<Span<'_>> {
        self.parts(mode).iter().map(|v| v.span(hl_style)).collect()
    }

    pub fn width_for(&self, mode: ShortcutMode) -> usize {
        self.parts(mode).iter().map(|v| v.span(None).width()).sum()
    }
}

pub fn shortcuts_full_width(shortcuts: &[Shortcut], pad_width: usize) -> usize {
    shortcuts.iter().map(|v| v.width_for(ShortcutMode::Full) + pad_width).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from() {
        // beginning
        let hl = Shortcut::from("filter", 0).unwrap();
        assert_eq!(hl.full, vec![Fragment::Hl("f".into()), Fragment::Raw("ilter".into())]);

        // middle
        let hl = Shortcut::from("filter", 3).unwrap();
        assert_eq!(
            hl.full,
            vec![Fragment::Raw("fil".into()), Fragment::Hl("t".into()), Fragment::Raw("er".into())]
        );

        // end
        let hl = Shortcut::from("filter", 5).unwrap();
        assert_eq!(hl.full, vec![Fragment::Raw("filte".into()), Fragment::Hl("r".into())]);

        // illegal
        let hl = Shortcut::from("⁰filter", 0);
        assert!(hl.is_err());
        let hl = Shortcut::from("filter", 100);
        assert!(hl.is_err());
    }

    #[test]
    fn compact_falls_back_to_full() {
        let shortcut = Shortcut::from("filter", 0).unwrap();

        assert_eq!(
            shortcut.spans_for(ShortcutMode::Compact, None),
            shortcut.spans_for(ShortcutMode::Full, None)
        );
        assert_eq!(
            shortcut.width_for(ShortcutMode::Compact),
            shortcut.width_for(ShortcutMode::Full)
        );
    }

    #[test]
    fn shortcuts_full_width_adds_pad_width_per_shortcut() {
        let shortcuts = vec![
            Shortcut::new(vec![Fragment::raw("foo")]),
            Shortcut::new(vec![Fragment::hl("⇧⇤"), Fragment::raw(" nav ")]),
        ];

        assert_eq!(shortcuts_full_width(&shortcuts, 2), 14);
    }
}
