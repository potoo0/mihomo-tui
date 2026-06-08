use time::format_description::FormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, UtcDateTime};

// NOTE:
// Numeric components in `time` format descriptions are zero-padded by default.
// source code: `time::format_description::modifier::Day::default()`
// (e.g. `02:03:04`). This comes from default modifiers

pub static DATE_ONLY_FMT: &[FormatItem<'static>] = format_description!("[year]-[month]-[day]");
pub static DATETIME_FMT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

/// Format OffsetDateTime as `2006-01-02 15:04:05`
///
/// # Arguments
///
/// * `dt` - OffsetDateTime
///
/// # Returns
///
/// * `None` if the value equals the Unix epoch or the conversion fails
pub fn format_datetime(dt: OffsetDateTime) -> Option<Box<str>> {
    if dt == OffsetDateTime::UNIX_EPOCH {
        return None;
    }
    dt.format(&DATETIME_FMT).ok().map(String::into_boxed_str)
}

/// Format OffsetDateTime as a compact elapsed time from now, such as `1s`, `59s`, or `1m`
///
/// # Arguments
///
/// * `dt` - OffsetDateTime
///
/// # Returns
///
/// * `0s` if the value is in the future
pub fn format_time_from_now(dt: OffsetDateTime) -> String {
    let secs = (OffsetDateTime::now_utc() - dt).whole_seconds().max(0);

    match secs {
        0..=59 => format!("{secs}s"),
        60..=3_599 => format!("{}m", secs / 60),
        3_600..=86_399 => format!("{}h", secs / 3_600),
        _ => format!("{}d", secs / 86_400),
    }
}

/// Format unix timestamp as `2006-01-02`
///
/// # Arguments
///
/// * `ts` - unix timestamp in seconds
///
/// # Returns
///
/// * `None` if the conversion fails
pub fn format_timestamp(ts: u64) -> Option<String> {
    i64::try_from(ts)
        .ok()
        .and_then(|ts| UtcDateTime::from_unix_timestamp(ts).ok())
        .and_then(|dt| dt.format(&DATE_ONLY_FMT).ok())
}

#[cfg(test)]
mod tests {
    use time::format_description::well_known::Rfc3339;

    use super::*;

    #[test]
    fn test_format_datetime() {
        let dt = OffsetDateTime::parse("2006-01-09T02:03:04.732+08:00", &Rfc3339).unwrap();
        let formatted = format_datetime(dt).unwrap();
        assert_eq!(formatted.as_ref(), "2006-01-09 02:03:04");
    }

    #[test]
    fn test_format_timestamp() {
        let dt = OffsetDateTime::parse("2006-01-09T02:03:04.732+08:00", &Rfc3339).unwrap();
        let ts = dt.unix_timestamp() as u64;
        let formatted = format_timestamp(ts).unwrap();
        assert_eq!(&formatted, "2006-01-08");
    }
}
