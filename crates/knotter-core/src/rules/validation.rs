use crate::error::CoreError;
use crate::time::TimePrecision;
use chrono::{DateTime, Local, TimeZone, Utc};

pub fn ensure_future_timestamp(now_utc: i64, timestamp: i64) -> Result<i64, CoreError> {
    ensure_future_timestamp_with_precision(now_utc, timestamp, TimePrecision::Second)
}

pub fn ensure_future_timestamp_with_precision(
    now_utc: i64,
    timestamp: i64,
    precision: TimePrecision,
) -> Result<i64, CoreError> {
    match precision {
        TimePrecision::Second => {
            if timestamp < now_utc {
                Err(CoreError::TimestampInPast)
            } else {
                Ok(timestamp)
            }
        }
        TimePrecision::Minute => {
            let min_allowed = now_utc - (now_utc % 60);
            if timestamp < min_allowed {
                return Err(CoreError::TimestampInPast);
            }
            if timestamp < now_utc {
                Ok(now_utc)
            } else {
                Ok(timestamp)
            }
        }
        TimePrecision::Date => {
            let now_date = local_date(now_utc);
            let timestamp_date = local_date(timestamp);
            if timestamp_date < now_date {
                return Err(CoreError::TimestampInPast);
            }
            Ok(end_of_day_utc(timestamp_date))
        }
    }
}

fn local_date(timestamp: i64) -> chrono::NaiveDate {
    DateTime::<Utc>::from_timestamp(timestamp, 0)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap())
        .with_timezone(&Local)
        .date_naive()
}

fn end_of_day_utc(date: chrono::NaiveDate) -> i64 {
    let naive = date
        .and_hms_opt(23, 59, 59)
        .unwrap_or_else(|| date.and_hms_opt(23, 59, 0).unwrap());
    if let Some(local_dt) = Local.from_local_datetime(&naive).single() {
        return local_dt.with_timezone(&Utc).timestamp();
    }
    Utc.from_utc_datetime(&naive).timestamp()
}

#[cfg(test)]
mod tests {
    use super::{ensure_future_timestamp, ensure_future_timestamp_with_precision};
    use crate::time::TimePrecision;
    use chrono::{Local, TimeZone, Utc};

    #[test]
    fn ensure_future_timestamp_rejects_past() {
        let now = 1_700_000_000;
        let result = ensure_future_timestamp(now, now - 1);
        assert!(result.is_err());
    }

    #[test]
    fn ensure_future_timestamp_accepts_now_or_later() {
        let now = 1_700_000_000;
        assert!(ensure_future_timestamp(now, now).is_ok());
        assert!(ensure_future_timestamp(now, now + 1).is_ok());
    }

    #[test]
    fn ensure_future_timestamp_with_precision_allows_same_minute() {
        let now = 1_700_000_045;
        let timestamp = 1_700_000_040;
        let adjusted =
            ensure_future_timestamp_with_precision(now, timestamp, TimePrecision::Minute)
                .expect("minute precision");
        assert_eq!(adjusted, now);
    }

    #[test]
    fn ensure_future_timestamp_with_precision_rejects_previous_minute() {
        let now = 1_700_000_065;
        let timestamp = 1_700_000_000;
        assert!(
            ensure_future_timestamp_with_precision(now, timestamp, TimePrecision::Minute).is_err()
        );
    }

    #[test]
    fn ensure_future_timestamp_with_precision_allows_same_date() {
        let now_local = Local.with_ymd_and_hms(2030, 1, 15, 12, 0, 0).unwrap();
        let ts_local = Local.with_ymd_and_hms(2030, 1, 15, 0, 0, 0).unwrap();
        let now = now_local.with_timezone(&Utc).timestamp();
        let timestamp = ts_local.with_timezone(&Utc).timestamp();
        let adjusted = ensure_future_timestamp_with_precision(now, timestamp, TimePrecision::Date)
            .expect("date precision");
        let expected = Local
            .with_ymd_and_hms(2030, 1, 15, 23, 59, 59)
            .unwrap()
            .with_timezone(&Utc)
            .timestamp();
        assert_eq!(adjusted, expected);
    }

    #[test]
    fn ensure_future_timestamp_with_precision_rejects_previous_date() {
        let now_local = Local.with_ymd_and_hms(2030, 1, 15, 12, 0, 0).unwrap();
        let ts_local = Local.with_ymd_and_hms(2030, 1, 14, 23, 59, 59).unwrap();
        let now = now_local.with_timezone(&Utc).timestamp();
        let timestamp = ts_local.with_timezone(&Utc).timestamp();
        assert!(
            ensure_future_timestamp_with_precision(now, timestamp, TimePrecision::Date).is_err()
        );
    }
}
