use std::borrow::Cow;

use ratatui::style::{Color, Style};
use ratatui::text::Span;

pub const DEFAULT_HL_COLOR: Color = Color::Indexed(130);

/// Per-fragment behavior in compact mode. Compact never reorders fragments:
/// it only shortens text, collapses a label into a separator, or hides a
/// fragment.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CompactForm {
    /// Identical in both modes.
    Same,
    /// Alternate keycap text or label key in compact mode.
    Alt(&'static str),
    /// Label collapses into a plain separator in compact mode.
    Sep(&'static str),
    /// Omitted in compact mode.
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum FragmentKind {
    /// Keycap text, always rendered with the highlight style.
    Key(&'static str),
    /// Word keycap: `word[..idx]`, `word[idx]`, `word[idx + 1..]` fuse with a
    /// following ASCII label when the translation keeps the word shape
    /// (`y` + `yes` → `yes`).
    FuseKey { word: &'static str, idx: usize },
    /// Plain literal separator between keycaps, such as `/`.
    Sep(&'static str),
    /// Translation key describing what the keycaps do.
    Label(&'static str),
}

/// A piece of a shortcut hint.
///
/// Locale files only contain plain words for labels; separators, spacing and
/// highlighting are decided here instead of being baked into translations.
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    kind: FragmentKind,
    compact: CompactForm,
}

impl Fragment {
    #[inline]
    pub const fn key(text: &'static str) -> Self {
        Self { kind: FragmentKind::Key(text), compact: CompactForm::Same }
    }

    /// Marks the `word` character at `idx` as the keycap that may fuse with
    /// the next label.
    #[inline]
    pub const fn fuse_key(word: &'static str, idx: usize) -> Self {
        Self { kind: FragmentKind::FuseKey { word, idx }, compact: CompactForm::Same }
    }

    #[inline]
    pub const fn sep(text: &'static str) -> Self {
        Self { kind: FragmentKind::Sep(text), compact: CompactForm::Same }
    }

    #[inline]
    pub const fn label(key: &'static str) -> Self {
        Self { kind: FragmentKind::Label(key), compact: CompactForm::Same }
    }

    /// Use another keycap text or label key in compact mode.
    #[must_use]
    pub const fn compact(mut self, text: &'static str) -> Self {
        self.compact = CompactForm::Alt(text);
        self
    }

    /// Collapse this label into a plain separator in compact mode.
    #[must_use]
    pub const fn compact_sep(mut self, text: &'static str) -> Self {
        self.compact = CompactForm::Sep(text);
        self
    }

    /// Omit this fragment in compact mode. For a label separating two keycaps,
    /// prefer [`Fragment::compact_sep`] so they do not run together.
    #[must_use]
    pub const fn hidden(mut self) -> Self {
        self.compact = CompactForm::Hidden;
        self
    }

    /// The fragment as it renders in `mode`: compact forms may shorten text,
    /// collapse a label into a separator, or hide the fragment entirely.
    fn resolve(&self, mode: ShortcutMode) -> Option<FragmentKind> {
        if mode == ShortcutMode::Full {
            return Some(self.kind);
        }
        match (self.kind, self.compact) {
            (_, CompactForm::Hidden) => None,
            (FragmentKind::Key(_), CompactForm::Alt(text)) => Some(FragmentKind::Key(text)),
            (FragmentKind::Sep(_), CompactForm::Alt(text)) => Some(FragmentKind::Sep(text)),
            (FragmentKind::Label(_), CompactForm::Alt(key)) => Some(FragmentKind::Label(key)),
            (FragmentKind::Label(_), CompactForm::Sep(text)) => Some(FragmentKind::Sep(text)),
            (FragmentKind::Key(text), _) => Some(FragmentKind::Key(text)),
            (kind @ FragmentKind::FuseKey { .. }, _) => Some(kind),
            (FragmentKind::Sep(text), _) => Some(FragmentKind::Sep(text)),
            (FragmentKind::Label(key), _) => Some(FragmentKind::Label(key)),
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
    parts: Vec<Fragment>,
}

impl Shortcut {
    pub fn new(parts: Vec<Fragment>) -> Self {
        Self { parts }
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
        // Labels are separated from neighboring keycaps by a single space;
        // consecutive keycaps and separators stay tight (e.g. `PgUp/PgDn`).
        let mut spans = Vec::with_capacity(self.parts.len() + 2);
        let mut label_before = false;
        let mut idx = 0;

        while idx < self.parts.len() {
            let Some(piece) = self.parts[idx].resolve(mode) else {
                idx += 1;
                continue;
            };

            // Fuse a word key with a following ASCII label that keeps the
            // word shape: `y` + `yes` renders as `yes` with `y` highlighted,
            // and `n` + `dns` as `dns` with `n` highlighted at its index.
            if let FragmentKind::FuseKey { word: fuse_word, idx: fuse_idx } = piece
                && let Some(next) = next_label(&self.parts[idx + 1..], mode)
                && let Cow::Borrowed(label) = translate_label(next.label)
                && label.is_ascii()
                && label.get(fuse_idx..fuse_idx + 1) == fuse_word.get(fuse_idx..fuse_idx + 1)
            {
                if !spans.is_empty() {
                    spans.push(Span::raw(" "));
                }
                if fuse_idx > 0 {
                    spans.push(Span::raw(&label[..fuse_idx]));
                }
                spans.push(Span::styled(&label[fuse_idx..fuse_idx + 1], style));
                if fuse_idx + 1 < label.len() {
                    spans.push(Span::raw(&label[fuse_idx + 1..]));
                }
                label_before = true;
                idx += next.offset + 1;
                continue;
            }

            // A space separates labels from any neighboring piece; the
            // fusion branch above keeps its own spacing because a word key is
            // only label-like once its translation is known.
            let space_before = match piece {
                FragmentKind::Label(..) => !spans.is_empty(),
                _ => label_before,
            };
            if space_before {
                spans.push(Span::raw(" "));
            }

            match piece {
                FragmentKind::Key(key) => {
                    spans.push(Span::styled(key, style));
                    label_before = false;
                }
                FragmentKind::FuseKey { word: fuse_word, idx: fuse_idx } => {
                    spans.push(Span::styled(&fuse_word[fuse_idx..fuse_idx + 1], style));
                    label_before = false;
                }
                FragmentKind::Sep(sep) => {
                    spans.push(Span::raw(sep));
                    label_before = false;
                }
                FragmentKind::Label(label_key) => {
                    spans.push(Span::raw(translate_label(label_key)));
                    label_before = true;
                }
            }
            idx += 1;
        }
        spans
    }

    pub fn width_for(&self, mode: ShortcutMode) -> usize {
        self.spans_for(mode, None).iter().map(Span::width).sum()
    }
}

/// The next resolved label after the current fragment, plus how many parts it
/// spans (hidden fragments in between do not break the fusion).
struct NextLabel {
    offset: usize,
    label: &'static str,
}

fn next_label(parts: &[Fragment], mode: ShortcutMode) -> Option<NextLabel> {
    for (offset, fragment) in parts.iter().enumerate() {
        match fragment.resolve(mode) {
            Some(FragmentKind::Label(label)) => {
                return Some(NextLabel { offset: offset + 1, label });
            }
            Some(_) => return None,
            None => continue,
        }
    }
    None
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

/// Creates a word shortcut: the keycap is the character of `$word` at `$idx`,
/// the label key is `shortcuts.<$word>`, and any trailing fragments (extra
/// keycaps or separators) are appended after the label. In ASCII locales the
/// key character is highlighted at `$idx` inside the translated word.
///
/// The index is validated at compile time (bounds, ASCII).
///
/// ```
/// # use crate::shortcut;
/// shortcut!("refresh", 0); // en: `refresh` with `r` highlighted; zh: `r 筛选`
/// shortcut!("dns", 1); // en: `dns` with `n` highlighted; zh: `n DNS 查询`
/// ```
#[macro_export]
macro_rules! shortcut {
    ($word:literal, $idx:literal $(, $tail:expr)* $(,)?) => {{
        const _: () = assert!($idx < $word.len(), "shortcut index out of bounds");
        const _: () = assert!($word.as_bytes()[$idx] < 128, "shortcut key must be ASCII");
        $crate::widgets::shortcut::Shortcut::new(vec![
            $crate::widgets::shortcut::Fragment::fuse_key($word, $idx),
            $crate::widgets::shortcut::Fragment::label(concat!("shortcuts.", $word)),
            $($tail),*
        ])
    }};
}

pub fn shortcuts_full_width(shortcuts: &[Shortcut], pad_width: usize) -> usize {
    shortcuts.iter().map(|v| v.width_for(ShortcutMode::Full) + pad_width).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents<'a, 'b>(spans: &'b [Span<'a>]) -> Vec<&'b str> {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn shortcut_macro_testcases() {
        struct Case {
            name: &'static str,
            shortcut: Shortcut,
            parts: Vec<Fragment>,
            // English full-mode span contents.
            spans: Vec<&'static str>,
            // Index into `spans` of the highlighted keycap.
            key: usize,
        }

        let cases = vec![
            Case {
                name: "head key",
                shortcut: crate::shortcut!("refresh", 0),
                parts: vec![Fragment::fuse_key("refresh", 0), Fragment::label("shortcuts.refresh")],
                spans: vec!["r", "efresh"],
                key: 0,
            },
            Case {
                name: "middle key",
                shortcut: crate::shortcut!("dns", 1),
                parts: vec![Fragment::fuse_key("dns", 1), Fragment::label("shortcuts.dns")],
                spans: vec!["d", "n", "s"],
                key: 1,
            },
            Case {
                name: "single trailing fragment",
                shortcut: crate::shortcut!("yes", 0, Fragment::key("↵")),
                parts: vec![
                    Fragment::fuse_key("yes", 0),
                    Fragment::label("shortcuts.yes"),
                    Fragment::key("↵"),
                ],
                spans: vec!["y", "es", " ", "↵"],
                key: 0,
            },
            Case {
                name: "multiple trailing fragments and trailing comma",
                shortcut: crate::shortcut!(
                    "no",
                    0,
                    Fragment::key("q"),
                    Fragment::sep("/"),
                    Fragment::key("Esc"),
                ),
                parts: vec![
                    Fragment::fuse_key("no", 0),
                    Fragment::label("shortcuts.no"),
                    Fragment::key("q"),
                    Fragment::sep("/"),
                    Fragment::key("Esc"),
                ],
                spans: vec!["n", "o", " ", "q", "/", "Esc"],
                key: 0,
            },
            Case {
                name: "later occurrence honors the index",
                shortcut: crate::shortcut!("refresh", 3),
                parts: vec![Fragment::fuse_key("refresh", 3), Fragment::label("shortcuts.refresh")],
                spans: vec!["ref", "r", "esh"],
                key: 1,
            },
            Case {
                name: "missing catalog word falls back to the source word",
                shortcut: crate::shortcut!("zzz-missing", 0),
                parts: vec![
                    Fragment::fuse_key("zzz-missing", 0),
                    Fragment::label("shortcuts.zzz-missing"),
                ],
                spans: vec!["z", "zz-missing"],
                key: 0,
            },
        ];

        for case in cases {
            assert_eq!(case.shortcut.parts, case.parts, "{}: parts", case.name);
            let spans = case.shortcut.spans_for(ShortcutMode::Full, None);
            assert_eq!(contents(&spans), case.spans, "{}: spans", case.name);
            assert_eq!(
                spans[case.key].style.fg,
                Some(DEFAULT_HL_COLOR),
                "{}: keycap style",
                case.name
            );
        }
    }

    #[test]
    fn labels_are_spaced_from_keycaps_once() {
        let shortcut =
            Shortcut::new(vec![Fragment::key("Del"), Fragment::label("shortcuts.reset")]);
        let spans = shortcut.spans_for(ShortcutMode::Full, None);

        assert_eq!(contents(&spans), ["Del", " ", "reset"]);
    }

    #[test]
    fn compact_forms_testcases() {
        struct Case {
            name: &'static str,
            shortcut: Shortcut,
            full: Vec<&'static str>,
            compact: Vec<&'static str>,
        }

        let cases = vec![
            Case {
                name: "alt shortens keycaps and labels",
                shortcut: Shortcut::new(vec![
                    Fragment::key("PgUp").compact("PU"),
                    Fragment::label("shortcuts.width").compact("shortcuts.width_short"),
                ]),
                full: vec!["PgUp", " ", "width"],
                compact: vec!["PU", " ", "w"],
            },
            Case {
                name: "hidden omits keycap and label",
                shortcut: Shortcut::new(vec![
                    Fragment::key("⇧⇤").hidden(),
                    Fragment::label("shortcuts.nav").hidden(),
                    Fragment::key("↑"),
                    Fragment::sep("/"),
                    Fragment::key("↓"),
                ]),
                full: vec!["⇧⇤", " ", "nav", " ", "↑", "/", "↓"],
                compact: vec!["↑", "/", "↓"],
            },
            Case {
                name: "sep collapses label between keys",
                shortcut: Shortcut::new(vec![
                    Fragment::key("↑"),
                    Fragment::label("shortcuts.scroll").compact_sep("/"),
                    Fragment::key("↓"),
                ]),
                full: vec!["↑", " ", "scroll", " ", "↓"],
                compact: vec!["↑", "/", "↓"],
            },
        ];

        for case in cases {
            assert_eq!(
                contents(&case.shortcut.spans_for(ShortcutMode::Full, None)),
                case.full,
                "{}: full",
                case.name
            );
            assert_eq!(
                contents(&case.shortcut.spans_for(ShortcutMode::Compact, None)),
                case.compact,
                "{}: compact",
                case.name
            );
        }
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
}
