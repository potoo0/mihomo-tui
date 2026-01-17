use serde::Deserialize;

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
    pub updated_at: Option<String>,
}
