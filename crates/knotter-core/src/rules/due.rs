use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};

use crate::error::CoreError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DueState {
    Unscheduled,
    Overdue,
    Today,
    Soon,
    Scheduled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DueSelector {
    Overdue,
    Today,
    Soon,
    Any,
    None,
}

pub const MAX_SOON_DAYS: i64 = 3650;

pub fn validate_soon_days(soon_days: i64) -> Result<i64, CoreError> {
    if !(0..=MAX_SOON_DAYS).contains(&soon_days) {
        return Err(CoreError::InvalidSoonDays(soon_days));
    }
    Ok(soon_days)
}

pub fn compute_due_state(
    now_utc: i64,
    next_touchpoint_at: Option<i64>,
    soon_days: i64,
    local_offset: FixedOffset,
) -> Result<DueState, CoreError> {
    let soon_days = validate_soon_days(soon_days)?;
    let next = match next_touchpoint_at {
        Some(value) => value,
        None => return Ok(DueState::Unscheduled),
    };

    if next < now_utc {
        return Ok(DueState::Overdue);
    }

    let (start_of_today, start_of_tomorrow) = local_day_bounds(now_utc, local_offset);

    if next >= start_of_today && next < start_of_tomorrow {
        return Ok(DueState::Today);
    }

    let soon_end = start_of_tomorrow + Duration::days(soon_days).num_seconds();
    if next >= start_of_tomorrow && next < soon_end {
        return Ok(DueState::Soon);
    }

    Ok(DueState::Scheduled)
}

fn local_day_bounds(now_utc: i64, local_offset: FixedOffset) -> (i64, i64) {
    let now = DateTime::<Utc>::from_timestamp(now_utc, 0).expect("valid timestamp");
    let local = now.with_timezone(&local_offset);
    let local_date = local.date_naive();
    let start_of_today_local = local_date.and_hms_opt(0, 0, 0).expect("midnight is valid");
    let start_of_tomorrow_local = start_of_today_local + Duration::days(1);
    let start_of_today = local_offset
        .from_local_datetime(&start_of_today_local)
        .single()
        .expect("fixed offset conversion")
        .with_timezone(&Utc)
        .timestamp();
    let start_of_tomorrow = local_offset
        .from_local_datetime(&start_of_tomorrow_local)
        .single()
        .expect("fixed offset conversion")
        .with_timezone(&Utc)
        .timestamp();

    (start_of_today, start_of_tomorrow)
}

#[cfg(test)]
mod tests {
    use super::{compute_due_state, validate_soon_days, DueState, MAX_SOON_DAYS};
    use chrono::{FixedOffset, TimeZone, Utc};

    #[test]
    fn due_state_unscheduled() {
        let now = Utc
            .with_ymd_and_hms(2024, 1, 10, 12, 0, 0)
            .unwrap()
            .timestamp();
        let offset = FixedOffset::east_opt(0).unwrap();
        assert_eq!(
            compute_due_state(now, None, 7, offset).unwrap(),
            DueState::Unscheduled
        );
    }

    #[test]
    fn due_state_today() {
        let offset = FixedOffset::east_opt(0).unwrap();
        let now = Utc
            .with_ymd_and_hms(2024, 1, 10, 12, 0, 0)
            .unwrap()
            .timestamp();
        let next = Utc
            .with_ymd_and_hms(2024, 1, 10, 18, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(
            compute_due_state(now, Some(next), 7, offset).unwrap(),
            DueState::Today
        );
    }

    #[test]
    fn due_state_soon() {
        let offset = FixedOffset::east_opt(0).unwrap();
        let now = Utc
            .with_ymd_and_hms(2024, 1, 10, 12, 0, 0)
            .unwrap()
            .timestamp();
        let next = Utc
            .with_ymd_and_hms(2024, 1, 12, 9, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(
            compute_due_state(now, Some(next), 7, offset).unwrap(),
            DueState::Soon
        );
    }

    #[test]
    fn due_state_overdue() {
        let offset = FixedOffset::east_opt(0).unwrap();
        let now = Utc
            .with_ymd_and_hms(2024, 1, 10, 12, 0, 0)
            .unwrap()
            .timestamp();
        let next = Utc
            .with_ymd_and_hms(2024, 1, 9, 12, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(
            compute_due_state(now, Some(next), 7, offset).unwrap(),
            DueState::Overdue
        );
    }

    #[test]
    fn validate_soon_days_rejects_large_values() {
        let result = validate_soon_days(MAX_SOON_DAYS + 1);
        assert!(result.is_err());
    }
}
