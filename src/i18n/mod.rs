//! Presentation-only localization. Configuration keys, API values and user data stay unchanged.
pub mod messages;
use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-CN")]
    SimplifiedChinese,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::SimplifiedChinese];

    pub const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
        }
    }

    pub const fn indicator(self) -> &'static str {
        match self {
            Self::English => "EN",
            Self::SimplifiedChinese => "中文",
        }
    }

    pub const fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
        }
    }
}

pub fn language() -> Language {
    match rust_i18n::locale().as_ref() {
        "zh-CN" => Language::SimplifiedChinese,
        _ => Language::English,
    }
}

pub fn set_language(language: Language) {
    rust_i18n::set_locale(language.code());
}

/// Translate only built-in presentation text. Unknown text remains unchanged.
pub fn tr(text: &str) -> Cow<'_, str> {
    translate(language(), text)
}

pub fn translate(language: Language, text: &str) -> Cow<'_, str> {
    rust_i18n::t!(text, locale = language.code())
}

/// Render shortcut descriptions independently of their activation keys.
pub fn shortcut_label(text: &str) -> Option<Cow<'_, str>> {
    if language() == Language::English {
        return None;
    }
    let key = format!("shortcuts.{text}");
    let label = rust_i18n::t!(&key).into_owned();
    (label != key).then_some(Cow::Owned(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_falls_back_and_preserves_english() {
        assert_eq!(translate(Language::English, "Config"), "Config");
        assert_eq!(translate(Language::SimplifiedChinese, "Config"), "配置");
        assert_eq!(
            translate(Language::SimplifiedChinese, "user-defined proxy"),
            "user-defined proxy"
        );
        assert_eq!(
            rust_i18n::t!("messages.expiry", locale = "missing", value = "tomorrow"),
            "Expire: tomorrow"
        );
    }

    #[test]
    fn cjk_labels_use_terminal_cell_width() {
        let label = translate(Language::SimplifiedChinese, "Language");
        assert_eq!(ratatui::text::Span::raw(label).width(), 4);
    }

    #[test]
    fn language_codes_are_stable_and_validate_input() {
        for language in Language::ALL {
            let encoded = yaml_serde::to_string(&language).unwrap();
            assert_eq!(yaml_serde::from_str::<Language>(&encoded).unwrap(), language);
        }
        assert!(yaml_serde::from_str::<Language>("invalid").is_err());
    }

    #[test]
    fn embedded_catalogs_have_matching_keys_and_placeholders() {
        use std::collections::{BTreeMap, BTreeSet};

        let english: BTreeMap<String, yaml_serde::Value> =
            yaml_serde::from_str(include_str!("../../locales/en.yml")).unwrap();
        let chinese: BTreeMap<String, yaml_serde::Value> =
            yaml_serde::from_str(include_str!("../../locales/zh-CN.yml")).unwrap();
        assert_eq!(english.keys().collect::<Vec<_>>(), chinese.keys().collect::<Vec<_>>());

        fn placeholders(text: &str) -> BTreeSet<&str> {
            text.split("%{").skip(1).map(|part| part.split_once('}').unwrap().0).collect()
        }

        for (key, value) in &english {
            if key == "_version" {
                continue;
            }
            let en = value.as_str().unwrap();
            let zh = chinese[key].as_str().unwrap();
            assert_eq!(placeholders(en), placeholders(zh), "{key}");
            assert_eq!(translate(Language::English, key), en, "{key}");
            assert_eq!(translate(Language::SimplifiedChinese, key), zh, "{key}");
        }
    }

    #[test]
    fn named_message_parameters_are_interpolated() {
        assert_eq!(
            rust_i18n::t!("messages.terminated", locale = "zh-CN", ok = 3, err = 1),
            "已终止 3 个连接，1 个失败。"
        );
        assert_eq!(
            rust_i18n::t!("messages.terminated", locale = "en", ok = 3, err = 1),
            "Terminated 3 connections, 1 failed."
        );
    }
}
