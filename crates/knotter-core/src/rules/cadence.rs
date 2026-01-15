use crate::error::CoreError;

pub const MAX_CADENCE_DAYS: i32 = 3650;

pub fn schedule_next(now_utc: i64, cadence_days: i32) -> Result<i64, CoreError> {
    if cadence_days <= 0 || cadence_days > MAX_CADENCE_DAYS {
        return Err(CoreError::InvalidCadenceDays(cadence_days));
    }

    let seconds = i64::from(cadence_days) * 86_400;
    Ok(now_utc + seconds)
}

pub fn next_touchpoint_after_touch(
    now_utc: i64,
    cadence_days: Option<i32>,
    reschedule_requested: bool,
    existing_next: Option<i64>,
) -> Result<Option<i64>, CoreError> {
    if !reschedule_requested {
        return Ok(existing_next);
    }

    match cadence_days {
        Some(days) => Ok(Some(schedule_next(now_utc, days)?)),
        None => Ok(existing_next),
    }
}

#[cfg(test)]
mod tests {
    use super::{next_touchpoint_after_touch, schedule_next, MAX_CADENCE_DAYS};

    #[test]
    fn schedule_next_adds_days() {
        let now = 1_700_000_000;
        let scheduled = schedule_next(now, 7).unwrap();
        assert_eq!(scheduled, now + 7 * 86_400);
    }

    #[test]
    fn schedule_next_rejects_large_values() {
        let now = 1_700_000_000;
        let result = schedule_next(now, MAX_CADENCE_DAYS + 1);
        assert!(result.is_err());
    }

    #[test]
    fn touch_reschedule_respects_flag() {
        let now = 1_700_000_000;
        let existing = Some(now + 123);
        let result = next_touchpoint_after_touch(now, Some(7), false, existing).unwrap();
        assert_eq!(result, existing);
    }
}
