use crate::errors::{CommandError, DomainError};
use crate::{
    models::{Archive, RecurrenceFrequency, StoredData, Task},
    state::{
        load_archives_for_board, lock_app, update_stored_with_archives_for_board, SharedAppData,
    },
    task_dates::{add_local_days, add_local_months, due_limit_millis, local_day_number},
};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

fn current_time_millis() -> Result<u128, DomainError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| DomainError::Internal(error.to_string()))
}

fn column_index(data: &StoredData, column_id: i64) -> Result<usize, DomainError> {
    data.columns
        .iter()
        .position(|column| column.id == column_id)
        .ok_or(DomainError::ColumnNotFound)
}

fn task_position(data: &StoredData, task_id: i64) -> Result<(usize, usize), DomainError> {
    data.columns
        .iter()
        .enumerate()
        .find_map(|(column_idx, column)| {
            column
                .tasks
                .iter()
                .position(|task| task.id == task_id)
                .map(|task_idx| (column_idx, task_idx))
        })
        .ok_or(DomainError::TaskNotFound)
}

fn advance_due_time(
    due_time: u128,
    frequency: RecurrenceFrequency,
    interval: u32,
) -> Result<u128, DomainError> {
    let due_time = i64::try_from(due_time).map_err(|_| "Task due time is out of range")?;
    let interval = interval.max(1);
    // Local calendar arithmetic keeps the wall-clock time and month-end days
    // the user sees, and matches the calendar preview.
    let next = match frequency {
        RecurrenceFrequency::Daily => add_local_days(due_time, i64::from(interval)),
        RecurrenceFrequency::Weekly => add_local_days(due_time, 7 * i64::from(interval)),
        RecurrenceFrequency::Monthly => add_local_months(due_time, interval),
    }
    .ok_or_else(|| "The next recurring task date is out of range".to_string())?;
    u128::try_from(next)
        .map_err(|_| DomainError::Internal("The next task date is before 1970".into()))
}

/// Moves the start by as many local days as the due date moved, keeping its
/// wall-clock time so date-only starts stay date-only.
fn shift_start_time(start_time: u128, original_due_time: u128, next_due_time: u128) -> u128 {
    let local = |time: u128| i64::try_from(time).ok();
    let days = local(next_due_time)
        .and_then(local_day_number)
        .zip(local(original_due_time).and_then(local_day_number))
        .map(|(next, original)| next - original);
    days.zip(local(start_time))
        .and_then(|(days, start)| add_local_days(start, days))
        .and_then(|start| u128::try_from(start).ok())
        .unwrap_or_else(|| start_time.saturating_add(next_due_time - original_due_time))
}

fn next_recurring_task(task: &Task, now: u128, id: i64) -> Result<Option<Task>, DomainError> {
    let Some(recurrence) = &task.recurrence else {
        return Ok(None);
    };
    let Some(original_due_time) = task.due_time else {
        return Ok(None);
    };

    let mut next_due_time = original_due_time;
    loop {
        next_due_time = advance_due_time(next_due_time, recurrence.frequency, recurrence.interval)?;
        // A date-only occurrence stays current until the end of its day.
        if due_limit_millis(next_due_time, task.due_at_midnight) > now {
            break;
        }
    }

    let mut next = task.clone();
    next.id = id;
    next.due_time = Some(next_due_time);
    // Legacy tasks without a creation time stored it in start_time instead of
    // a user-chosen start, so it must not become the next occurrence's start.
    next.start_time = if task.created_time == 0 || task.start_time == 0 {
        0
    } else {
        shift_start_time(task.start_time, original_due_time, next_due_time)
    };
    next.created_time = now;
    for item in &mut next.items {
        item.completed = false;
    }
    Ok(Some(next))
}

#[tauri::command]
pub fn get_archives(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<Archive>, CommandError> {
    load_archives_for_board(&state, expected_board_id).map_err(CommandError::from)
}

#[tauri::command]
pub fn get_archive_task(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    task_id: i64,
) -> Result<Task, CommandError> {
    let guard = lock_app(&state)?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    guard
        .database
        .load_archive_task(expected_board_id, task_id)
        .map_err(CommandError::repository)
        .and_then(|task| task.ok_or(CommandError::TaskNotFound))
}

/// Batch lookup for canvases; archived tasks that no longer exist are omitted.
#[tauri::command]
pub fn get_archive_tasks(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    task_ids: Vec<i64>,
) -> Result<Vec<Task>, CommandError> {
    if task_ids.len() > 100_000 {
        return Err(CommandError::InvalidArgument);
    }
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    guard
        .database
        .load_archive_tasks(expected_board_id, &task_ids)
        .map_err(CommandError::repository)
}

#[tauri::command]
pub fn archive_task(
    state: State<'_, SharedAppData>,
    task_id: i64,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    let time = current_time_millis()?;
    update_stored_with_archives_for_board(&state, expected_board_id, |data| {
        let (column, task_index) = task_position(data, task_id)?;
        let task = data.columns[column].tasks.remove(task_index);
        if task.recurrence.is_some() && task.due_time.is_some() {
            let next_id = data.allocate_task_id()?;
            if let Some(next) = next_recurring_task(&task, time, next_id)? {
                data.columns[column].tasks.insert(task_index, next);
            }
        }
        data.archives.push(Archive { time, task });
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn archive_all_tasks(
    state: State<'_, SharedAppData>,
    column_id: i64,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    let time = current_time_millis()?;
    update_stored_with_archives_for_board(&state, expected_board_id, |data| {
        let column = column_index(data, column_id)?;
        let tasks = std::mem::take(&mut data.columns[column].tasks);
        let mut next_tasks = Vec::new();
        let mut archives = Vec::with_capacity(tasks.len());
        for task in tasks {
            if task.recurrence.is_some() && task.due_time.is_some() {
                let next_id = data.allocate_task_id()?;
                if let Some(next) = next_recurring_task(&task, time, next_id)? {
                    next_tasks.push(next);
                }
            }
            archives.push(Archive { time, task });
        }
        data.columns[column].tasks = next_tasks;
        data.archives.extend(archives);
        Ok(())
    })
    .map_err(CommandError::from)
}

#[cfg(test)]
mod tests {
    use super::next_recurring_task;
    use crate::models::{Recurrence, RecurrenceFrequency, Task, TaskItem};

    fn recurring_task(due_time: u128, frequency: RecurrenceFrequency) -> Task {
        Task {
            id: 4,
            title: "Routine".into(),
            start_time: due_time - 3_600_000,
            due_time: Some(due_time),
            created_time: due_time - 86_400_000,
            recurrence: Some(Recurrence {
                frequency,
                interval: 1,
            }),
            items: vec![TaskItem {
                id: "step".into(),
                text: "Do it".into(),
                completed: true,
            }],
            ..Task::default()
        }
    }

    #[test]
    fn daily_recurrence_skips_missed_occurrences_and_resets_checklist() {
        let task = recurring_task(1_700_000_000_000, RecurrenceFrequency::Daily);
        let next = next_recurring_task(&task, 1_700_000_000_000 + 3 * 86_400_000, 8)
            .unwrap()
            .unwrap();

        assert_eq!(next.id, 8);
        assert_eq!(next.due_time, Some(1_700_000_000_000 + 4 * 86_400_000));
        assert_eq!(next.start_time, task.start_time + 4 * 86_400_000);
        assert_eq!(next.created_time, 1_700_000_000_000 + 3 * 86_400_000);
        assert!(!next.items[0].completed);

        let legacy = Task {
            created_time: 0,
            ..task.clone()
        };
        let next = next_recurring_task(&legacy, 1_700_000_000_000 + 3 * 86_400_000, 9)
            .unwrap()
            .unwrap();
        assert_eq!(next.start_time, 0);
    }

    #[test]
    fn date_only_occurrence_stays_current_until_the_day_ends() {
        use chrono::{Local, TimeZone};
        let at = |day, hour| {
            u128::try_from(
                Local
                    .with_ymd_and_hms(2026, 10, day, hour, 0, 0)
                    .earliest()
                    .unwrap()
                    .timestamp_millis(),
            )
            .unwrap()
        };
        let task = recurring_task(at(13, 0), RecurrenceFrequency::Daily);

        let next = next_recurring_task(&task, at(14, 10), 8).unwrap().unwrap();
        assert_eq!(next.due_time, Some(at(14, 0)));
    }

    #[test]
    fn monthly_recurrence_clamps_to_the_end_of_the_local_month() {
        use chrono::{Local, TimeZone};
        let at = |month, day, hour| {
            u128::try_from(
                Local
                    .with_ymd_and_hms(2024, month, day, hour, 0, 0)
                    .earliest()
                    .unwrap()
                    .timestamp_millis(),
            )
            .unwrap()
        };
        let mut task = recurring_task(at(1, 31, 0), RecurrenceFrequency::Monthly);
        task.start_time = at(1, 29, 0);
        let next = next_recurring_task(&task, at(1, 31, 0), 9)
            .unwrap()
            .unwrap();

        assert_eq!(next.due_time, Some(at(2, 29, 0)));
        assert_eq!(next.start_time, at(2, 27, 0));
    }
}

#[tauri::command]
pub fn unarchive_task(
    state: State<'_, SharedAppData>,
    column_id: i64,
    task_id: i64,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_with_archives_for_board(&state, expected_board_id, |data| {
        let column = column_index(data, column_id)?;
        let archive = data
            .archives
            .iter()
            .position(|archive| archive.task.id == task_id)
            .ok_or(DomainError::TaskNotFound)?;
        let archive = data.archives.remove(archive);
        data.columns[column].tasks.push(archive.task);
        Ok(())
    })
    .map_err(CommandError::from)
}
