//! Validated task mutations shared by plugin changesets and registry operations.
use crate::{board_mutations as mutation, errors::DomainError, models::*};
use std::collections::HashSet;

pub(crate) fn valid_task(task: &Task) -> bool {
    valid_task_fields(task)
        && start_within_due(task.start_time, task.due_time, task.due_at_midnight)
}
/// Only checks times the caller changes: a date-only due synced from another
/// time zone is not local midnight here, so round-tripping it must not fail.
pub(crate) fn valid_task_update(previous: &Task, task: &Task) -> bool {
    // Legacy tasks expose no start to callers (their start_time is the creation).
    let previous_start = external_times(previous).0;
    valid_task_fields(task)
        && ((task.start_time, task.due_time, task.due_at_midnight)
            == (previous_start, previous.due_time, previous.due_at_midnight)
            || start_within_due(task.start_time, task.due_time, task.due_at_midnight))
}
fn valid_task_fields(task: &Task) -> bool {
    !task.title.trim().is_empty()
        && task.title.len() <= 4096
        && task.description.len() <= 262144
        && task.color.len() <= 64
        && task.labels.len() <= 100
        && task.labels.iter().collect::<HashSet<_>>().len() == task.labels.len()
        && task
            .labels
            .iter()
            .all(|x| !x.trim().is_empty() && x.len() <= 256)
        && task.items.len() <= 1000
        && task
            .items
            .iter()
            .map(|item| &item.id)
            .collect::<HashSet<_>>()
            .len()
            == task.items.len()
        && task
            .items
            .iter()
            .all(|x| !x.id.is_empty() && x.id.len() <= 128 && x.text.len() <= 4096)
        && task.start_time <= 8_640_000_000_000_000
        && task.due_time.is_none_or(|x| x <= 8_640_000_000_000_000)
        && task
            .recurrence
            .as_ref()
            .is_none_or(|r| r.interval > 0 && r.interval <= 10000)
}
/// Mirrors the task dialog: a start may not follow the due time, and a
/// date-only due date (local midnight) covers its whole local day.
fn start_within_due(start: u128, due: Option<u128>, due_at_midnight: bool) -> bool {
    let Some(due) = due else { return true };
    start == 0 || start <= crate::task_dates::due_limit_millis(due, due_at_midnight)
}

/// `(start, created)` as external callers see them. Tasks saved before creation
/// times were recorded stored their creation time in `start_time`; they have no
/// user-chosen start.
pub(crate) fn external_times(task: &Task) -> (u128, u128) {
    if task.created_time == 0 {
        (0, task.start_time)
    } else {
        (task.start_time, task.created_time)
    }
}
/// Epoch milliseconds, with `0` (unset) as `None`.
pub(crate) fn optional_time(value: u128) -> Result<Option<u64>, DomainError> {
    (value != 0)
        .then(|| u64::try_from(value))
        .transpose()
        .map_err(|_| DomainError::InvalidArgument)
}

/// Adapters clear caller-supplied creation times; the host stamps them here.
pub(crate) fn create_task(
    data: &mut StoredData,
    column_id: i64,
    mut task: Task,
    now: u128,
) -> Result<i64, DomainError> {
    if !valid_task(&task) {
        return Err(DomainError::InvalidArgument);
    }
    if task.created_time == 0 {
        task.created_time = now;
    }
    mutation::add_task(data, column_id, task, None)
}

pub(crate) fn update_task(
    data: &mut StoredData,
    task_id: i64,
    task: Task,
) -> Result<(), DomainError> {
    let (column, index) = mutation::task_position(data, task_id)?;
    if !valid_task_update(&data.columns[column].tasks[index], &task) {
        return Err(DomainError::InvalidArgument);
    }
    mutation::update_task(data, task_id, task)
}

/// Positions are meaningful only in custom-ordered columns.
pub(crate) fn move_task(
    data: &mut StoredData,
    task_id: i64,
    column_id: i64,
    before_task_id: Option<i64>,
) -> Result<(), DomainError> {
    let column = mutation::column_index(data, column_id)?;
    if data.columns[column].sort_order != ColumnSort::Custom {
        return Err(DomainError::InvalidArgument);
    }
    mutation::task_position(data, task_id)?;
    if before_task_id == Some(task_id) && data.columns[column].tasks.iter().all(|t| t.id != task_id)
    {
        return Err(DomainError::InvalidArgument);
    }
    mutation::move_task(data, task_id, column_id, before_task_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};

    fn local_ms(y: i32, m: u32, d: u32, h: u32, min: u32) -> u128 {
        Local
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .earliest()
            .unwrap()
            .timestamp_millis() as u128
    }

    #[test]
    fn start_may_not_follow_due_except_within_a_date_only_due_day() {
        let due = local_ms(2026, 7, 15, 9, 30);
        assert!(start_within_due(0, Some(due), false));
        assert!(start_within_due(due + 1, None, false));
        assert!(start_within_due(due, Some(due), false));
        assert!(!start_within_due(due + 1, Some(due), false));

        let date_only = local_ms(2026, 7, 15, 0, 0);
        let next_day = local_ms(2026, 7, 16, 0, 0);
        assert!(start_within_due(
            local_ms(2026, 7, 15, 23, 59),
            Some(date_only),
            false
        ));
        assert!(start_within_due(next_day - 1, Some(date_only), false));
        assert!(!start_within_due(next_day, Some(date_only), false));
        // A chosen 00:00 due is a time, not the whole day.
        assert!(!start_within_due(date_only + 1, Some(date_only), true));

        let task = Task {
            title: "Plan".into(),
            start_time: next_day,
            due_time: Some(date_only),
            ..Task::default()
        };
        assert!(!valid_task(&task));
        assert!(valid_task(&Task {
            start_time: date_only,
            ..task.clone()
        }));

        // Unchanged times round-trip even when invalid here; changes are checked.
        let stored = Task {
            created_time: 1,
            ..task.clone()
        };
        assert!(valid_task_update(&stored, &task));
        // Marking the unchanged due as a chosen 00:00 changes its meaning.
        assert!(!valid_task_update(
            &stored,
            &Task {
                due_at_midnight: true,
                ..task.clone()
            }
        ));
        assert!(!valid_task_update(
            &stored,
            &Task {
                start_time: next_day + 1,
                ..task.clone()
            }
        ));
        let legacy = Task {
            start_time: next_day,
            created_time: 0,
            ..task.clone()
        };
        assert!(valid_task_update(
            &legacy,
            &Task {
                start_time: 0,
                ..task.clone()
            }
        ));
        assert!(!valid_task_update(&legacy, &task));
    }
}
