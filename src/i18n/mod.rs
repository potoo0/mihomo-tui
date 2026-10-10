//! Presentation-only localization. Configuration keys, API values and user data stay unchanged.
use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use strum::{EnumCount, EnumIter, EnumString, IntoStaticStr};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, EnumIter, EnumCount, IntoStaticStr, EnumString,
)]
#[strum(const_into_str)]
pub enum Language {
    #[default]
    #[strum(serialize = "en")]
    English,
    #[strum(serialize = "zh-CN")]
    SimplifiedChinese,
}

impl Language {
    pub const fn code(self) -> &'static str {
        self.into_str()
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

impl Serialize for Language {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.into())
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use std::str::FromStr;

        let s = String::deserialize(d)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

pub fn language() -> Language {
    use std::str::FromStr;

    Language::from_str(&rust_i18n::locale()).unwrap_or(Language::English)
}

pub fn set_language(language: Language) {
    rust_i18n::set_locale(language.code());
}

/// Column titles are also stable config and filter identifiers, so translate them only at draw
/// time.
pub fn column_title(title: &str) -> Cow<'_, str> {
    translate_column(language(), title)
}

fn translate_column(language: Language, title: &str) -> Cow<'_, str> {
    let key = match title {
        "Alive" => "columns.alive",
        "Host" => "columns.host",
        "Chains" => "columns.chains",
        "DownRate" => "columns.down_rate",
        "UpRate" => "columns.up_rate",
        "DownTotal" => "columns.down_total",
        "UpTotal" => "columns.up_total",
        "SourceIP" => "columns.source_ip",
        "Type" => "columns.type",
        "Process" => "columns.process",
        "SniffHost" => "columns.sniff_host",
        "ConnectTime" => "columns.connect_time",
        "SourcePort" => "columns.source_port",
        "Dest" => "columns.destination",
        "Inbound" => "columns.inbound",
        "Level" => "columns.level",
        "Content" => "columns.content",
        "Index" => "columns.index",
        "Size" => "columns.size",
        "Disabled" => "columns.disabled",
        "Hits" => "columns.hits",
        "HitAt" => "columns.hit_at",
        "Name" => "columns.name",
        "VehicleType" => "columns.vehicle_type",
        "Behavior" => "columns.behavior",
        "RuleCount" => "columns.rule_count",
        "UpdatedAt" => "columns.updated_at",
        _ => return Cow::Borrowed(title),
    };
    rust_i18n::t!(key, locale = language.code())
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn column_translation_preserves_config_identifiers() {
        assert_eq!(translate_column(Language::English, "Host"), "Host");
        assert_eq!(translate_column(Language::SimplifiedChinese, "Host"), "主机");
        assert_eq!(translate_column(Language::SimplifiedChinese, "custom"), "custom");
    }

    #[test]
    fn language_codes_are_stable_and_validate_input() {
        for language in Language::iter() {
            let encoded = yaml_serde::to_string(&language).unwrap();
            assert_eq!(yaml_serde::from_str::<Language>(&encoded).unwrap(), language);
        }
        assert!(yaml_serde::from_str::<Language>("invalid").is_err());
    }

    #[test]
    fn locale_catalogs_have_matching_keys_and_placeholders() {
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
            if key.starts_with("columns.") {
                assert_eq!(translate_column(Language::SimplifiedChinese, en), zh, "{key}");
            }
        }
    }
}
