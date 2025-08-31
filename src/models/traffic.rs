use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Traffic {
    pub down: u64,
    pub up: u64,
}
