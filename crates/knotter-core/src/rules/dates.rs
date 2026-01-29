use crate::error::CoreError;
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, Utc};

pub fn local_today(now_utc: i64, local_offset: FixedOffset) -> Result<NaiveDate, CoreError> {
    let now = DateTime::<Utc>::from_timestamp(now_utc, 0).ok_or(CoreError::InvalidTimestamp)?;
    Ok(now.with_timezone(&local_offset).date_naive())
}

pub fn date_occurs_today(
    now_utc: i64,
    month: u8,
    day: u8,
    local_offset: FixedOffset,
) -> Result<bool, CoreError> {
    let today = local_today(now_utc, local_offset)?;
    if today.month() == month as u32 && today.day() == day as u32 {
        return Ok(true);
    }

    if month == 2 && day == 29 {
        return Ok(today.month() == 2 && today.day() == 28 && !is_leap_year(today.year()));
    }

    Ok(false)
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::{date_occurs_today, is_leap_year};
    use chrono::{FixedOffset, TimeZone, Utc};

    #[test]
    fn date_occurs_today_exact_match() {
        let offset = FixedOffset::east_opt(0).unwrap();
        let now = Utc
            .with_ymd_and_hms(2024, 6, 10, 12, 0, 0)
            .unwrap()
            .timestamp();
        assert!(date_occurs_today(now, 6, 10, offset).expect("date"));
        assert!(!date_occurs_today(now, 6, 11, offset).expect("date"));
    }

    #[test]
    fn date_occurs_today_leap_day_fallback() {
        let offset = FixedOffset::east_opt(0).unwrap();
        let non_leap = Utc
            .with_ymd_and_hms(2023, 2, 28, 12, 0, 0)
            .unwrap()
            .timestamp();
        assert!(date_occurs_today(non_leap, 2, 29, offset).expect("date"));

        let leap = Utc
            .with_ymd_and_hms(2024, 2, 28, 12, 0, 0)
            .unwrap()
            .timestamp();
        assert!(!date_occurs_today(leap, 2, 29, offset).expect("date"));
        let leap_day = Utc
            .with_ymd_and_hms(2024, 2, 29, 12, 0, 0)
            .unwrap()
            .timestamp();
        assert!(date_occurs_today(leap_day, 2, 29, offset).expect("date"));
    }

    #[test]
    fn leap_year_logic() {
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2000));
    }
}
