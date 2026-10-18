//! Calendar semantics for task times, matching `src/routes/utils/task-date-range.ts`.
//!
//! A time at local midnight is a date without a time of day, unless the task
//! marks it as a chosen 00:00 (`start_at_midnight` / `due_at_midnight`). A
//! date-only start begins at that midnight; a date-only due date lasts until
//! the end of its day.

use chrono::{
    DateTime, Datelike, Days, Duration, Local, Months, NaiveDateTime, NaiveTime, TimeZone,
};

fn local_time(millis: i64) -> Option<DateTime<Local>> {
    Local.timestamp_millis_opt(millis).single()
}

/// A local wall-clock time in milliseconds. A time skipped by a daylight
/// saving change moves forward an hour, as JavaScript `Date` does.
fn from_local(time: NaiveDateTime) -> Option<i64> {
    Local
        .from_local_datetime(&time)
        .earliest()
        .or_else(|| {
            Local
                .from_local_datetime(&(time + Duration::hours(1)))
                .earliest()
        })
        .map(|time| time.timestamp_millis())
}

fn local_midnight(date: chrono::NaiveDate) -> Option<i64> {
    from_local(date.and_time(NaiveTime::MIN))
}

/// Sequential local calendar day, independent of daylight saving offsets.
pub fn local_day_number(millis: i64) -> Option<i64> {
    local_time(millis).map(|time| i64::from(time.date_naive().num_days_from_ce()))
}

/// Moves a time by whole local days, keeping its wall-clock time.
pub fn add_local_days(millis: i64, days: i64) -> Option<i64> {
    let time = local_time(millis)?.naive_local();
    let moved = if days >= 0 {
        time.checked_add_days(Days::new(days.unsigned_abs()))
    } else {
        time.checked_sub_days(Days::new(days.unsigned_abs()))
    }?;
    from_local(moved)
}

/// Moves a time by local months, keeping its wall-clock time and clamping the
/// day to the end of shorter months.
pub fn add_local_months(millis: i64, months: u32) -> Option<i64> {
    from_local(
        local_time(millis)?
            .naive_local()
            .checked_add_months(Months::new(months))?,
    )
}

pub fn is_date_only(millis: i64, at_midnight: bool) -> bool {
    !at_midnight && local_time(millis).is_some_and(|time| time.time() == NaiveTime::MIN)
}

/// Local midnight that starts the day containing `millis`.
pub fn local_day_start(millis: i64) -> i64 {
    local_time(millis)
        .and_then(|time| local_midnight(time.date_naive()))
        .unwrap_or(millis)
}

/// Last instant a task is still on time; a date-only due date covers its whole day.
pub fn due_limit(due_time: i64, at_midnight: bool) -> i64 {
    if !is_date_only(due_time, at_midnight) {
        return due_time;
    }
    local_time(due_time)
        .and_then(|time| time.date_naive().checked_add_days(Days::new(1)))
        .and_then(local_midnight)
        .map_or(due_time, |next_day| next_day - 1)
}

/// [`due_limit`] for the unsigned millisecond times stored on tasks.
pub fn due_limit_millis(due_time: u128, at_midnight: bool) -> u128 {
    i64::try_from(due_time)
        .ok()
        .and_then(|due| u128::try_from(due_limit(due, at_midnight)).ok())
        .unwrap_or(due_time)
}

#[cfg(test)]
mod tests {
    use super::{add_local_days, add_local_months, due_limit, is_date_only, local_day_start};
    use chrono::{Local, TimeZone};

    fn local(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        Local
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .earliest()
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn date_only_due_dates_last_until_the_end_of_the_day() {
        let due = local(2026, 10, 14, 0, 0);
        assert!(is_date_only(due, false));
        assert_eq!(due_limit(due, false), local(2026, 10, 15, 0, 0) - 1);
    }

    #[test]
    fn timed_due_dates_keep_their_time() {
        let due = local(2026, 10, 14, 13, 0);
        assert!(!is_date_only(due, false));
        assert_eq!(due_limit(due, false), due);
    }

    #[test]
    fn a_chosen_midnight_is_a_time() {
        let due = local(2026, 10, 14, 0, 0);
        assert!(!is_date_only(due, true));
        assert_eq!(due_limit(due, true), due);
    }

    #[test]
    fn calendar_moves_keep_the_local_time() {
        assert_eq!(
            add_local_days(local(2026, 10, 14, 13, 0), 3),
            Some(local(2026, 10, 17, 13, 0))
        );
        assert_eq!(
            add_local_days(local(2026, 10, 14, 0, 0), -14),
            Some(local(2026, 9, 30, 0, 0))
        );
        assert_eq!(
            add_local_months(local(2024, 1, 31, 0, 0), 1),
            Some(local(2024, 2, 29, 0, 0))
        );
    }

    #[test]
    fn day_start_is_local_midnight() {
        assert_eq!(
            local_day_start(local(2026, 10, 14, 13, 30)),
            local(2026, 10, 14, 0, 0)
        );
    }
}
