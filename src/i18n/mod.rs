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
    column_key(title).map_or(Cow::Borrowed(title), |key| rust_i18n::t!(key))
}

/// Maps a stable column title to its catalog key; `None` leaves the title as-is.
fn column_key(title: &str) -> Option<&'static str> {
    let key = match title {
        "Alive" => "columns.alive",
        "Host" => "columns.host",
        "Rule" => "columns.rule",
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
        _ => return None,
    };
    Some(key)
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn column_translation_preserves_config_identifiers() {
        assert_eq!(column_title("Host"), "Host");
        assert_eq!(column_title("custom"), "custom");
        assert_eq!(rust_i18n::t!(column_key("Host").unwrap(), locale = "zh-CN"), "主机");
    }

    #[test]
    fn table_column_titles_have_catalog_keys() {
        use crate::store::connections::CONNECTION_COLS;
        use crate::store::logs::LOG_COLS;
        use crate::store::rule_providers::RULE_PROVIDER_COLS;
        use crate::store::rules::RULE_COLS;

        let titles = CONNECTION_COLS
            .iter()
            .map(|def| def.col.title)
            .chain(RULE_COLS.iter().map(|def| def.col.title))
            .chain(RULE_PROVIDER_COLS.iter().map(|def| def.col.title))
            .chain(LOG_COLS.iter().map(|def| def.title));

        for title in titles {
            let key = column_key(title).unwrap_or_else(|| panic!("{title}: no columns.* key"));
            let en = rust_i18n::t!(key, locale = "en");
            assert_eq!(en.as_ref(), title, "{key}: en value must round-trip the title");
        }
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
            assert!(!en.is_empty(), "{key}: empty en value");
            assert!(!zh.is_empty(), "{key}: empty zh value");
            assert_eq!(en, en.trim(), "{key}: surrounding whitespace");
            assert_eq!(zh, zh.trim(), "{key}: surrounding whitespace");
            assert_eq!(placeholders(en), placeholders(zh), "{key}");
            assert_eq!(rust_i18n::t!(key.as_str(), locale = "en").as_ref(), en, "{key}: en lookup");
            assert_eq!(
                rust_i18n::t!(key.as_str(), locale = "zh-CN").as_ref(),
                zh,
                "{key}: zh-CN lookup"
            );
            if key.starts_with("columns.") {
                assert_eq!(
                    column_key(en),
                    Some(key.as_str()),
                    "{key}: en value must map back to its key"
                );
            }
        }
    }
}
