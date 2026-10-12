use serde::Deserialize;
use strum::{EnumIter, IntoStaticStr};

#[derive(Debug, Clone, Copy, PartialEq, EnumIter, IntoStaticStr, Deserialize)]
#[serde(rename_all = "lowercase")]
#[strum(const_into_str)]
pub enum LogLevel {
    #[strum(serialize = "error")]
    Error,
    #[strum(serialize = "warning")]
    Warning,
    #[strum(serialize = "info")]
    Info,
    #[strum(serialize = "debug")]
    Debug,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Log {
    pub r#type: LogLevel,
    pub payload: String,
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn test_log_iter() {
        let mut iter = LogLevel::iter();
        assert_eq!(Some(LogLevel::Error), iter.next());
        assert_eq!(Some(LogLevel::Warning), iter.next());
        assert_eq!(Some(LogLevel::Info), iter.next());
        assert_eq!(Some(LogLevel::Debug), iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn test_level_into_str() {
        assert_eq!(LogLevel::Error.into_str(), "error");
        assert_eq!(LogLevel::Warning.into_str(), "warning");
        assert_eq!(LogLevel::Info.into_str(), "info");
        assert_eq!(LogLevel::Debug.into_str(), "debug");
    }
}
