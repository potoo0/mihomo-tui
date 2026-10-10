use std::borrow::Cow;

use anyhow::{Result, anyhow};
use ratatui::style::{Color, Style};
use ratatui::text::Span;

pub const DEFAULT_HL_COLOR: Color = Color::Indexed(130);

/// A piece of a shortcut hint.
///
/// Locale files only contain plain words for [`Fragment::Label`]; separators,
/// spacing and highlighting are decided here instead of being baked into
/// translations.
#[derive(Debug, Clone, PartialEq)]
pub enum Fragment {
    /// Keycap text, always rendered with the highlight style.
    Key(Box<str>),
    /// Plain literal separator between keycaps, such as `/`.
    Sep(Box<str>),
    /// Translation key describing what the keycaps do.
    Label(Box<str>),
}

impl Fragment {
    #[inline]
    pub fn key<S: Into<Box<str>>>(s: S) -> Self {
        Self::Key(s.into())
    }

    #[inline]
    pub fn sep<S: Into<Box<str>>>(s: S) -> Self {
        Self::Sep(s.into())
    }

    #[inline]
    pub fn label<S: Into<Box<str>>>(s: S) -> Self {
        Self::Label(s.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutMode {
    Full,
    Compact,
}

/// A word shortcut created by [`Shortcut::from`].
#[derive(Debug, Clone, PartialEq)]
struct WordHint {
    /// Translation key, always `shortcuts.<word>`; the word is recovered by
    /// stripping the prefix instead of storing a second copy.
    key: Box<str>,
    /// Highlighted character index (also its byte index, ASCII-only).
    hl: usize,
}

impl WordHint {
    /// Original ASCII word, used for shape checks and as the fallback label
    /// when the catalog has no translation.
    fn word(&self) -> &str {
        self.key.strip_prefix("shortcuts.").unwrap_or(&self.key)
    }

    fn spans(&self, style: Style) -> Vec<Span<'_>> {
        let translated = rust_i18n::t!(self.key.as_ref());
        let label: &str = match translated {
            // No-arg translations are borrowed from the embedded catalogs.
            Cow::Borrowed(label) if label != &*self.key => label,
            // Missing translation: render the source word itself.
            _ => self.word(),
        };

        let word = self.word();
        let prefix = &word[..self.hl];
        let key_char = &word[self.hl..self.hl + 1];
        if label.starts_with(prefix) && label[prefix.len()..].starts_with(key_char) {
            // The translation keeps the source word shape; fuse them together.
            let (head, tail) = label.split_at(prefix.len());
            let (_, tail) = tail.split_at(key_char.len());
            let mut spans = Vec::with_capacity(2);
            if !head.is_empty() {
                spans.push(Span::raw(head));
            }
            spans.push(Span::styled(key_char, style));
            if !tail.is_empty() {
                spans.push(Span::raw(tail));
            }
            spans
        } else {
            // Different alphabet: show the keycap, then the translated label.
            vec![Span::styled(key_char, style), Span::raw(" "), Span::raw(label)]
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shortcut {
    full: Vec<Fragment>,
    compact: Option<Vec<Fragment>>,
    word: Option<WordHint>,
}

impl Shortcut {
    pub fn new(parts: Vec<Fragment>) -> Self {
        Self { full: parts, compact: None, word: None }
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

    /// Creates a `Shortcut` from `s`, highlighting the character at `hl_idx`.
    ///
    /// The catalog key is `shortcuts.<s>`. When the translation keeps the
    /// original word shape (e.g. English `filter`), the word is rendered as a
    /// whole with the highlighted character inside it; otherwise the keycap is
    /// rendered separately, followed by the translated label (`f 筛选`).
    ///
    /// # Notes
    /// - **ASCII-only**: treats `hl_idx` as both byte and char index.
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
            parts.push(Fragment::key(&text[..hl_idx]));
        }
        parts.push(Fragment::key(&text[hl_idx..pivot]));
        if pivot < len {
            parts.push(Fragment::key(&text[pivot..]));
        }

        let mut shortcut = Self::new(parts);
        shortcut.word = Some(WordHint { key: format!("shortcuts.{text}").into(), hl: hl_idx });
        Ok(shortcut)
    }

    pub fn into_spans<'a>(self, hl_style: Option<Style>) -> Vec<Span<'a>> {
        self.into_spans_for(ShortcutMode::Full, hl_style)
    }

    pub fn into_spans_for<'a>(self, mode: ShortcutMode, hl_style: Option<Style>) -> Vec<Span<'a>> {
        self.spans_for(mode, hl_style)
            .into_iter()
            .map(|span| Span::styled(span.content.into_owned(), span.style))
            .collect()
    }

    pub fn spans_for(&'_ self, mode: ShortcutMode, hl_style: Option<Style>) -> Vec<Span<'_>> {
        let style = hl_style.unwrap_or_else(|| Style::default().fg(DEFAULT_HL_COLOR));
        if let Some(word) = &self.word {
            return word.spans(style);
        }
        fragment_spans(self.parts(mode), style)
    }

    pub fn width_for(&self, mode: ShortcutMode) -> usize {
        self.spans_for(mode, None).iter().map(Span::width).sum()
    }
}

fn fragment_spans(parts: &[Fragment], style: Style) -> Vec<Span<'_>> {
    let mut spans = Vec::with_capacity(parts.len() + 2);
    // Labels are separated from neighboring keycaps by a single space;
    // consecutive keycaps and separators stay tight (e.g. `PgUp/PgDn`).
    let mut after_label = false;
    let mut skip_until = 0;
    for (idx, fragment) in parts.iter().enumerate() {
        if idx < skip_until {
            continue;
        }
        match fragment {
            Fragment::Key(key) => {
                // Fuse a single-char keycap with a label that starts with it,
                // e.g. `y` + `yes` renders as `yes` with `y` highlighted.
                if key.len() == 1
                    && let Some(Fragment::Label(label_key)) = parts.get(idx + 1)
                    && let Cow::Borrowed(label) = translate_label(label_key.as_ref())
                    && label.starts_with(&**key)
                {
                    if !spans.is_empty() {
                        spans.push(Span::raw(" "));
                    }
                    spans.push(Span::styled(&label[..key.len()], style));
                    spans.push(Span::raw(&label[key.len()..]));
                    after_label = true;
                    skip_until = idx + 2;
                    continue;
                }
                if after_label {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(key.as_ref(), style));
                after_label = false;
            }
            Fragment::Sep(sep) => {
                if after_label {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::raw(sep.as_ref()));
                after_label = false;
            }
            Fragment::Label(label_key) => {
                if !spans.is_empty() {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::raw(translate_label(label_key.as_ref())));
                after_label = true;
            }
        }
    }
    spans
}

/// Translates a label key, borrowing from the embedded catalogs when possible.
fn translate_label(key: &str) -> Cow<'_, str> {
    let translated = rust_i18n::t!(key);
    match translated {
        Cow::Borrowed(label) if label != key => Cow::Borrowed(label),
        // Missing keys fall back to the raw word instead of leaking the full
        // translation key into the interface.
        _ => Cow::Borrowed(key.rsplit('.').next().unwrap_or(key)),
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
        assert_eq!(hl.full, vec![Fragment::Key("f".into()), Fragment::Key("ilter".into())]);
        assert_eq!(hl.word.unwrap().hl, 0);

        // middle
        let hl = Shortcut::from("filter", 3).unwrap();
        assert_eq!(
            hl.full,
            vec![
                Fragment::Key("fil".into()),
                Fragment::Key("t".into()),
                Fragment::Key("er".into())
            ]
        );

        // end
        let hl = Shortcut::from("filter", 5).unwrap();
        assert_eq!(hl.full, vec![Fragment::Key("filte".into()), Fragment::Key("r".into())]);

        // illegal
        let hl = Shortcut::from("⁰filter", 0);
        assert!(hl.is_err());
        let hl = Shortcut::from("filter", 100);
        assert!(hl.is_err());
    }

    #[test]
    fn compact_falls_back_to_full() {
        let shortcut = Shortcut::new(vec![Fragment::key("foo")]);

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
            Shortcut::new(vec![Fragment::key("foo")]),
            Shortcut::new(vec![Fragment::key("⇧⇤"), Fragment::label("shortcuts.nav")]),
        ];

        assert_eq!(shortcuts_full_width(&shortcuts, 2), 13);
    }

    #[test]
    fn word_shortcut_keeps_english_shape() {
        let shortcut = Shortcut::from("filter", 0).unwrap();
        let spans = shortcut.spans_for(ShortcutMode::Full, None);

        assert_eq!(spans.iter().map(|s| s.content.as_ref()).collect::<Vec<_>>(), ["f", "ilter"]);
        assert_eq!(spans[0].style.fg, Some(DEFAULT_HL_COLOR));
    }

    #[test]
    fn single_char_key_fuses_with_matching_label() {
        let shortcut = Shortcut::new(vec![
            Fragment::key("y"),
            Fragment::label("shortcuts.yes"),
            Fragment::key("↵"),
        ]);
        let spans = shortcut.spans_for(ShortcutMode::Full, None);

        assert_eq!(
            spans.iter().map(|s| s.content.as_ref()).collect::<Vec<_>>(),
            ["y", "es", " ", "↵"]
        );
    }

    #[test]
    fn labels_are_spaced_from_keycaps_once() {
        let shortcut =
            Shortcut::new(vec![Fragment::key("Del"), Fragment::label("shortcuts.reset")]);
        let spans = shortcut.spans_for(ShortcutMode::Full, None);

        assert_eq!(
            spans.iter().map(|s| s.content.as_ref()).collect::<Vec<_>>(),
            ["Del", " ", "reset"]
        );
    }
}
