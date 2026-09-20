use crate::{
    models::{Archive, RecurrenceFrequency, StoredData, Task},
    state::{load_archives_for_board, update_stored_with_archives_for_board, SharedAppData},
};
use chrono::{DateTime, Duration, Months, Utc};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

fn current_time_millis() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| error.to_string())
}

fn column_index(data: &StoredData, column_id: i64) -> Result<usize, String> {
    data.columns
        .iter()
        .position(|column| column.id == column_id)
        .ok_or_else(|| format!("Column not found: {column_id}"))
}

fn task_position(data: &StoredData, task_id: i64) -> Result<(usize, usize), String> {
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
        .ok_or_else(|| format!("Task not found: {task_id}"))
}

fn advance_due_time(
    due_time: u128,
    frequency: RecurrenceFrequency,
    interval: u32,
) -> Result<u128, String> {
    let due_time = i64::try_from(due_time).map_err(|_| "Task due time is out of range")?;
    let date = DateTime::<Utc>::from_timestamp_millis(due_time)
        .ok_or_else(|| "Task due time is invalid".to_string())?;
    let interval = interval.max(1);
    let next = match frequency {
        RecurrenceFrequency::Daily => date.checked_add_signed(Duration::days(i64::from(interval))),
        RecurrenceFrequency::Weekly => {
            date.checked_add_signed(Duration::weeks(i64::from(interval)))
        }
        RecurrenceFrequency::Monthly => date.checked_add_months(Months::new(interval)),
    }
    .ok_or_else(|| "The next recurring task date is out of range".to_string())?;
    u128::try_from(next.timestamp_millis())
        .map_err(|_| "The next task date is before 1970".to_string())
}

fn next_recurring_task(task: &Task, now: u128, id: i64) -> Result<Option<Task>, String> {
    let Some(recurrence) = &task.recurrence else {
        return Ok(None);
    };
    let Some(original_due_time) = task.due_time else {
        return Ok(None);
    };

    let mut next_due_time = original_due_time;
    loop {
        next_due_time = advance_due_time(next_due_time, recurrence.frequency, recurrence.interval)?;
        if next_due_time > now {
            break;
        }
    }

    let shift = next_due_time - original_due_time;
    let mut next = task.clone();
    next.id = id;
    next.due_time = Some(next_due_time);
    if next.start_time != 0 {
        next.start_time = next.start_time.saturating_add(shift);
    }
    for item in &mut next.items {
        item.completed = false;
    }
    Ok(Some(next))
}

#[tauri::command]
pub fn get_archives(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<Archive>, String> {
    load_archives_for_board(&state, expected_board_id)
}

#[tauri::command]
pub fn archive_task(
    state: State<'_, SharedAppData>,
    task_id: i64,
    expected_board_id: i64,
) -> Result<(), String> {
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
}

#[tauri::command]
pub fn archive_all_tasks(
    state: State<'_, SharedAppData>,
    column_id: i64,
    expected_board_id: i64,
) -> Result<(), String> {
    let time = current_time_millis()?;
    update_stored_with_archives_for_board(&state, expected_board_id, |data| {
        let column = column_index(data, column_id)?;
        let tasks = data.columns[column].tasks.drain(..).collect::<Vec<_>>();
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
        assert!(!next.items[0].completed);
    }

    #[test]
    fn monthly_recurrence_clamps_to_the_end_of_the_month() {
        let task = recurring_task(1_706_659_200_000, RecurrenceFrequency::Monthly); // 2024-01-31 UTC
        let next = next_recurring_task(&task, 1_706_659_200_000, 9)
            .unwrap()
            .unwrap();

        assert_eq!(next.due_time, Some(1_709_164_800_000)); // 2024-02-29 UTC
    }
}

#[tauri::command]
pub fn unarchive_task(
    state: State<'_, SharedAppData>,
    column_id: i64,
    task_id: i64,
    expected_board_id: i64,
) -> Result<(), String> {
    update_stored_with_archives_for_board(&state, expected_board_id, |data| {
        let column = column_index(data, column_id)?;
        let archive = data
            .archives
            .iter()
            .position(|archive| archive.task.id == task_id)
            .ok_or_else(|| format!("Archived task not found: {task_id}"))?;
        let archive = data.archives.remove(archive);
        data.columns[column].tasks.push(archive.task);
        Ok(())
    })
}
