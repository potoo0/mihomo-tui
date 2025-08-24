use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Log {
    pub payload: String,
    pub r#type: String,
}
