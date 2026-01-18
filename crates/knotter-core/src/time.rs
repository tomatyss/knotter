use chrono::{
    DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone, Timelike,
    Utc,
};
use thiserror::Error;

const DATETIME_FORMATS: [&str; 4] = [
    "%Y-%m-%d %H:%M",
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%dT%H:%M:%S",
];

#[derive(Debug, Error)]
pub enum TimeParseError {
    #[error("timestamp cannot be empty")]
    Empty,
    #[error("invalid date")]
    InvalidDate,
    #[error("invalid datetime format: expected YYYY-MM-DD or YYYY-MM-DD HH:MM")]
    InvalidDateTime,
    #[error("invalid date format: expected YYYY-MM-DD")]
    InvalidDateFormat,
    #[error("invalid time format: expected HH:MM")]
    InvalidTimeFormat,
    #[error("ambiguous local time: {0}")]
    AmbiguousLocalTime(String),
}

pub fn now_utc() -> i64 {
    Utc::now().timestamp()
}

pub fn local_offset() -> FixedOffset {
    Local::now().offset().fix()
}

pub fn parse_local_timestamp(input: &str) -> Result<i64, TimeParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(TimeParseError::Empty);
    }

    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let naive = date
            .and_hms_opt(0, 0, 0)
            .ok_or(TimeParseError::InvalidDate)?;
        return local_to_utc_timestamp(naive);
    }

    for fmt in DATETIME_FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return local_to_utc_timestamp(dt);
        }
    }

    Err(TimeParseError::InvalidDateTime)
}

pub fn parse_local_date_time(date: &str, time: Option<&str>) -> Result<i64, TimeParseError> {
    let date = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| TimeParseError::InvalidDateFormat)?;
    let time = match time {
        Some(raw) => NaiveTime::parse_from_str(raw.trim(), "%H:%M")
            .map_err(|_| TimeParseError::InvalidTimeFormat)?,
        None => NaiveTime::from_hms_opt(0, 0, 0).ok_or(TimeParseError::InvalidDate)?,
    };

    let naive = date.and_time(time);
    local_to_utc_timestamp(naive)
}

pub fn format_timestamp_date(ts: i64) -> String {
    let dt = DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
        .with_timezone(&Local);
    dt.format("%Y-%m-%d").to_string()
}

pub fn format_timestamp_datetime(ts: i64) -> String {
    let dt = DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
        .with_timezone(&Local);
    dt.format("%Y-%m-%d %H:%M").to_string()
}

pub fn format_timestamp_date_or_datetime(ts: i64) -> String {
    let dt = DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
        .with_timezone(&Local);
    if dt.hour() == 0 && dt.minute() == 0 && dt.second() == 0 {
        dt.format("%Y-%m-%d").to_string()
    } else {
        dt.format("%Y-%m-%d %H:%M").to_string()
    }
}

fn local_to_utc_timestamp(naive: NaiveDateTime) -> Result<i64, TimeParseError> {
    let local = Local
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| TimeParseError::AmbiguousLocalTime(naive.to_string()))?;
    Ok(local.with_timezone(&Utc).timestamp())
}

#[cfg(test)]
mod tests {
    use super::{
        format_timestamp_date, format_timestamp_date_or_datetime, format_timestamp_datetime,
        parse_local_date_time, parse_local_timestamp, TimeParseError,
    };
    use chrono::{Local, TimeZone, Utc};

    #[test]
    fn parse_local_timestamp_accepts_date_only() {
        let ts = parse_local_timestamp("2030-01-15").unwrap();
        let local = Utc.timestamp_opt(ts, 0).unwrap().with_timezone(&Local);
        assert_eq!(local.format("%Y-%m-%d").to_string(), "2030-01-15");
    }

    #[test]
    fn parse_local_timestamp_accepts_datetime() {
        let ts = parse_local_timestamp("2030-01-15 13:45").unwrap();
        let local = Utc.timestamp_opt(ts, 0).unwrap().with_timezone(&Local);
        assert_eq!(
            local.format("%Y-%m-%d %H:%M").to_string(),
            "2030-01-15 13:45"
        );
    }

    #[test]
    fn parse_local_timestamp_rejects_empty() {
        let err = parse_local_timestamp("").unwrap_err();
        assert!(matches!(err, TimeParseError::Empty));
    }

    #[test]
    fn parse_local_date_time_accepts_date_and_time() {
        let ts = parse_local_date_time("2030-01-15", Some("13:45")).unwrap();
        let local = Utc.timestamp_opt(ts, 0).unwrap().with_timezone(&Local);
        assert_eq!(
            local.format("%Y-%m-%d %H:%M").to_string(),
            "2030-01-15 13:45"
        );
    }

    #[test]
    fn format_helpers_match_local_time() {
        let local = Local.with_ymd_and_hms(2030, 1, 15, 13, 45, 0).unwrap();
        let ts = local.with_timezone(&Utc).timestamp();
        assert_eq!(
            format_timestamp_date(ts),
            local.format("%Y-%m-%d").to_string()
        );
        assert_eq!(
            format_timestamp_datetime(ts),
            local.format("%Y-%m-%d %H:%M").to_string()
        );
        assert_eq!(
            format_timestamp_date_or_datetime(ts),
            local.format("%Y-%m-%d %H:%M").to_string()
        );
    }
}
