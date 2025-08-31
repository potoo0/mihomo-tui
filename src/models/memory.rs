use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Memory {
    #[serde(rename(deserialize = "inuse"))]
    pub used: u64,
}
