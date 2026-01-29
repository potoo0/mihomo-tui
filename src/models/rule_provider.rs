use serde::Deserialize;
use time::OffsetDateTime;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleProvider {
    // pub r#type: String,
    pub name: String,
    pub behavior: String,
    // pub format: String,
    pub vehicle_type: String,
    pub rule_count: u32,
    /// updated time in RFC3339Nano format, e.g. "2006-01-02T15:04:05.999999999Z07:00"
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,

    // for ui only
    #[serde(skip)]
    pub updated_at_str: Option<Box<str>>,
}
