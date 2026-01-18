use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub r#type: String,
    pub payload: String,
    pub proxy: String,
}
