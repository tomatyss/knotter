use chrono::{
    DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone, Timelike,
    Utc,
};
use thiserror::Error;

const DATETIME_FORMATS_MINUTES: [&str; 2] = ["%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M"];
const DATETIME_FORMATS_SECONDS: [&str; 2] = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimePrecision {
    Date,
    Minute,
    Second,
}

#[derive(Debug, Error)]
pub enum TimeParseError {
    #[error("timestamp cannot be empty")]
    Empty,
    #[error("invalid date")]
    InvalidDate,
    #[error(
        "invalid datetime format: expected YYYY-MM-DD, YYYY-MM-DD HH:MM, or YYYY-MM-DD HH:MM:SS (T separator allowed)"
    )]
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
    parse_local_timestamp_with_precision(input).map(|(timestamp, _)| timestamp)
}

pub fn parse_local_timestamp_with_precision(
    input: &str,
) -> Result<(i64, TimePrecision), TimeParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(TimeParseError::Empty);
    }

    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let naive = date
            .and_hms_opt(0, 0, 0)
            .ok_or(TimeParseError::InvalidDate)?;
        return Ok((local_to_utc_timestamp(naive)?, TimePrecision::Date));
    }

    for fmt in DATETIME_FORMATS_SECONDS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return Ok((local_to_utc_timestamp(dt)?, TimePrecision::Second));
        }
    }

    for fmt in DATETIME_FORMATS_MINUTES {
        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return Ok((local_to_utc_timestamp(dt)?, TimePrecision::Minute));
        }
    }

    Err(TimeParseError::InvalidDateTime)
}

pub fn parse_local_date_time(date: &str, time: Option<&str>) -> Result<i64, TimeParseError> {
    parse_local_date_time_with_precision(date, time).map(|(timestamp, _)| timestamp)
}

pub fn parse_local_date_time_with_precision(
    date: &str,
    time: Option<&str>,
) -> Result<(i64, TimePrecision), TimeParseError> {
    let date = NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d")
        .map_err(|_| TimeParseError::InvalidDateFormat)?;
    let (time, precision) = match time {
        Some(raw) => (
            NaiveTime::parse_from_str(raw.trim(), "%H:%M")
                .map_err(|_| TimeParseError::InvalidTimeFormat)?,
            TimePrecision::Minute,
        ),
        None => (
            NaiveTime::from_hms_opt(0, 0, 0).ok_or(TimeParseError::InvalidDate)?,
            TimePrecision::Date,
        ),
    };

    let naive = date.and_time(time);
    Ok((local_to_utc_timestamp(naive)?, precision))
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

pub fn format_timestamp_time(ts: i64) -> String {
    let dt = DateTime::<Utc>::from_timestamp(ts, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
        .with_timezone(&Local);
    dt.format("%H:%M").to_string()
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
        format_timestamp_time, parse_local_date_time, parse_local_date_time_with_precision,
        parse_local_timestamp, parse_local_timestamp_with_precision, TimeParseError, TimePrecision,
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
    fn parse_local_timestamp_infers_precision() {
        let (date_ts, date_precision) = parse_local_timestamp_with_precision("2030-01-15").unwrap();
        let (minute_ts, minute_precision) =
            parse_local_timestamp_with_precision("2030-01-15 13:45").unwrap();
        let (second_ts, second_precision) =
            parse_local_timestamp_with_precision("2030-01-15 13:45:30").unwrap();

        assert_eq!(date_precision, TimePrecision::Date);
        assert_eq!(minute_precision, TimePrecision::Minute);
        assert_eq!(second_precision, TimePrecision::Second);
        assert!(date_ts <= minute_ts);
        assert!(minute_ts <= second_ts);
    }

    #[test]
    fn parse_local_date_time_infers_precision() {
        let (_ts, precision) = parse_local_date_time_with_precision("2030-01-15", None).unwrap();
        assert_eq!(precision, TimePrecision::Date);

        let (_ts, precision) =
            parse_local_date_time_with_precision("2030-01-15", Some("13:45")).unwrap();
        assert_eq!(precision, TimePrecision::Minute);
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
        assert_eq!(format_timestamp_time(ts), local.format("%H:%M").to_string());
        assert_eq!(
            format_timestamp_date_or_datetime(ts),
            local.format("%Y-%m-%d %H:%M").to_string()
        );
    }
}
