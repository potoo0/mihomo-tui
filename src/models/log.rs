use serde::Deserialize;
use strum::Display;

#[derive(Debug, Clone, Copy, Display, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    #[strum(to_string = "error")]
    Error,
    #[strum(to_string = "warning")]
    Warning,
    #[strum(to_string = "info")]
    Info,
    #[strum(to_string = "debug")]
    Debug,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Log {
    pub payload: String,
    pub r#type: LogLevel,
}
