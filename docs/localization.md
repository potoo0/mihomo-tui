# Localization maintenance

Translations use `rust-i18n` and are embedded in the binary at compile time from
`locales/en.yml` and `locales/zh-CN.yml`. English is the default and fallback.
`src/i18n/mod.rs` provides a thin presentation adapter and language metadata;
`messages.rs` wraps parameterized messages with named arguments. The crate owns
active locale state; there is no second translation backend or locale store.

English source text remains the key for existing labels to keep the display-layer
changes small. New parameterized messages use namespaced keys and full sentences.
Do not translate store identifiers, configuration keys, filter grammar, API values,
user-defined names or incoming logs. Do not split new messages into fragments.

Translations return `Cow<str>`; use owned strings when a widget outlives its source.
Calculate terminal widths with Ratatui, not UTF-8 byte lengths. Translate shortcut
labels at render time and keep activation keys independent of descriptions. The
header uses actual display widths for its language indicator and version, while
retaining the existing tab abbreviation and clipping behavior.

Uppercase `L` opens the centered language popup from the main view. Its width is
30% and height is 50% of the available content area. The persistence note stays in two fixed rows at the bottom; only the language list
scrolls. Short lists have no scrollbar
or paging hints. Overflowing lists scroll to keep the selection visible and show
the existing scrollbar style, PgUp/PgDn paging and g/G first/last hints. Focused input
and existing popups receive their keys first. Lowercase `l` and `Ctrl-L` keep their
existing behavior. Arrows or `j/k` navigate, Enter applies and closes, and Esc or q
cancel. The selected candidate is separate from the active language. Tabs remain
1–8; `ui.startup-tab: Language` is no longer supported.

The main configuration defaults to `language: en`; `zh-CN` selects Simplified
Chinese. The optional language field in runtime schema version 1 remains compatible
with existing sidecars. The existing atomic writer saves preferences. Disabling
runtime settings permits session-only switching without writing a file.

To add a language:

1. Add its catalog to `locales/`, preserving keys and named placeholders from English.
2. Add its stable code, native name and header indicator to `Language` and `ALL`.
3. Update the locale-to-language mapping and catalog validation tests.
4. Test fallback, switching, terminal widths and persistence.

Catalog tests check that both languages have the same keys and placeholders and
that embedded lookups return the catalog values. YAML placeholder mistakes require
these checks; they are not validated by Rust's `format!` compiler checks.

Validation:

```sh
cargo +nightly fmt-check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --release --locked
```

Use a mock API for terminal interaction tests. Include 80-/140-column terminals,
popup apply/cancel, return to the original page, persistence, disabled writes,
repeated language changes, and uppercase L in a focused text input.
