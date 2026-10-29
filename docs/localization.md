# Localization

UI text uses `rust-i18n`; catalogs are embedded at compile time from
`locales/en.yml` and `locales/zh-CN.yml`. English is the default and fallback.

## Conventions

- Use semantic keys (`tabs.config`, `shortcuts.nav`, `updates.up_to_date`);
  do not use English source text as a key.
- Keep a full sentence in one key. Use named placeholders (`%{count}`) for
  parameters; do not concatenate translated fragments.
- Never translate identifiers, configuration keys, filter grammar, API values
  or user data.
- Filter fields keep English column identifiers stable in every locale. The
  localized column title of the active locale is additionally accepted as an
  input alias by `RowFilter`; translated titles must stay unambiguous, which the
  catalog tests enforce. Filter patterns survive language switches; localized
  aliases only match while their language is active.
- Do not translate CLI-only output, diagnostics, product names (`mihomo-tui`,
  `mihomo core`), or technical readouts such as `Width` / `Height` and `level:`.
- Call `rust_i18n::t!` at render time so language switching applies
  immediately; `i18n::set_language` updates the global locale.
- Translations return `Cow<str>`; borrow when possible. Compute terminal
  widths with Ratatui (`Span::width`), not byte lengths.

## Shortcuts

`Fragment::key` / `sep` / `label` describe keycaps, separators and translated
labels. Locale values are plain words; spacing and the keycap/word fusion rule
(`y` + `yes` → `yes` with `y` highlighted) are owned by the shortcut widget.
Word shortcuts use the `shortcut!("refresh", 0)` macro: the keycap is the word
character at that index and the label key is `shortcuts.<word>`; the index is
validated at compile time. Compact mode never reorders fragments — use
`.compact(text)`, `.compact_sep("/")` or `.hidden()` per fragment.

## Language popup

Uppercase `L` opens the popup; arrows or `j`/`k` navigate, Enter applies,
Esc or `q` cancels. The choice persists through the existing runtime-config
writer, or is session-only when runtime config is disabled.
`language: en | zh-CN` in the config sets the startup language.

## Adding a language

1. Add `locales/<code>.yml` with the same keys and placeholders as English.
2. Add the variant to `Language` in `src/i18n/mod.rs`; update the `code`,
   `indicator` and `native_name` mappings.
3. Run the catalog parity tests and check terminal widths in both languages.

## Validation

```sh
cargo +nightly fmt-check && cargo clippy-strict
cargo test --locked
```
