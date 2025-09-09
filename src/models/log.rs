use serde::Deserialize;
use strum::{Display, EnumIter};

#[derive(Debug, Clone, Copy, PartialEq, Display, EnumIter, Deserialize)]
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
}
