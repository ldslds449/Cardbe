//! Completion follows a task's position. Tasks in the board's done column are
//! completed, and archived tasks are completed by archiving; there is no
//! separate checkbox that could disagree with where a task is.

use crate::{
    board_mutations::{column_index, move_tasks, task_position},
    commands::archive::{archive_task_in, next_recurring_task},
    errors::DomainError,
    models::StoredData,
};
use std::collections::HashMap;

/// Stamps `completed_at` on tasks that are in the done column and clears it
/// elsewhere. A recurring task moved into the done column leaves its next
/// occurrence where it came from, as archiving does. Runs on local writes only,
/// so received snapshots are never rewritten.
pub fn reconcile(
    before: &StoredData,
    after: &mut StoredData,
    now: u128,
) -> Result<(), DomainError> {
    let done = after.done_column_id();
    let previous: HashMap<i64, (i64, usize)> = before
        .columns
        .iter()
        .flat_map(|column| {
            column
                .tasks
                .iter()
                .enumerate()
                .map(move |(index, task)| (task.id, (column.id, index)))
        })
        .collect();
    let mut continued = Vec::new();
    for column in &mut after.columns {
        let in_done = Some(column.id) == done;
        for task in &mut column.tasks {
            if !in_done {
                task.completed_at = None;
            } else if task.completed_at.is_none() {
                task.completed_at = Some(now);
                if let Some(&(from, index)) = previous.get(&task.id) {
                    if from != column.id && task.recurrence.is_some() && task.due_time.is_some() {
                        continued.push((task.clone(), from, index));
                    }
                }
            }
        }
    }
    for (task, from, index) in continued {
        let id = after.allocate_task_id()?;
        let Some(mut next) = next_recurring_task(&task, now, id)? else {
            continue;
        };
        next.completed_at = None;
        // The original column may have been deleted in the same write.
        let Some(column) = after
            .columns
            .iter()
            .position(|column| column.id == from)
            .or_else(|| {
                after
                    .columns
                    .iter()
                    .position(|column| Some(column.id) != done)
            })
            .map(|index| &mut after.columns[index])
        else {
            continue;
        };
        let index = index.min(column.tasks.len());
        column.tasks.insert(index, next);
    }
    Ok(())
}

/// Makes `column_id` the only done column, or clears it with `None`.
pub fn set_done_column(data: &mut StoredData, column_id: Option<i64>) -> Result<(), DomainError> {
    if let Some(id) = column_id {
        column_index(data, id)?;
    }
    for column in &mut data.columns {
        column.done = Some(column.id) == column_id;
    }
    Ok(())
}

/// Moves tasks to the done column, or archives them when the board has none.
/// Tasks that are already done stay where they are.
pub fn complete_tasks(
    data: &mut StoredData,
    task_ids: &[i64],
    now: u128,
) -> Result<(), DomainError> {
    for &task_id in task_ids {
        task_position(data, task_id)?;
    }
    let done = data.done_column_id();
    let open: Vec<i64> = task_ids
        .iter()
        .copied()
        .filter(|&id| {
            task_position(data, id).is_ok_and(|(column, _)| Some(data.columns[column].id) != done)
        })
        .collect();
    match done {
        Some(done) if !open.is_empty() => move_tasks(data, &open, done, None),
        Some(_) => Ok(()),
        None => open
            .into_iter()
            .try_for_each(|id| archive_task_in(data, id, now).map(|_| ())),
    }
}

/// Moves done or archived tasks to `column_id`, which must not be the done
/// column. Tasks that are already open stay where they are.
pub fn reopen_tasks(
    data: &mut StoredData,
    task_ids: &[i64],
    column_id: i64,
) -> Result<(), DomainError> {
    let destination = column_index(data, column_id)?;
    if data.columns[destination].done {
        return Err(DomainError::InvalidArgument);
    }
    let done = data.done_column_id();
    let mut moved = Vec::new();
    for &task_id in task_ids {
        if let Ok((column, _)) = task_position(data, task_id) {
            if Some(data.columns[column].id) == done {
                moved.push(task_id);
            }
            continue;
        }
        let archive = data
            .archives
            .iter()
            .position(|archive| archive.task.id == task_id)
            .ok_or(DomainError::TaskNotFound)?;
        let archive = data.archives.remove(archive);
        data.columns[destination].tasks.push(archive.task);
    }
    if moved.is_empty() {
        return Ok(());
    }
    move_tasks(data, &moved, column_id, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Archive, Column, Recurrence, RecurrenceFrequency, Task};

    const NOW: u128 = 1_800_000_000_000;
    const DAY: u128 = 86_400_000;

    fn column(id: i64, done: bool, tasks: Vec<Task>) -> Column {
        Column {
            id,
            name: format!("Column {id}"),
            color: String::new(),
            sort_order: Default::default(),
            sort_rules: Vec::new(),
            done,
            tasks,
        }
    }

    fn task(id: i64) -> Task {
        Task {
            id,
            title: format!("Task {id}"),
            ..Task::default()
        }
    }

    fn board() -> StoredData {
        StoredData {
            columns: vec![
                column(1, false, vec![task(10), task(11)]),
                column(2, true, vec![]),
            ],
            ..StoredData::default()
        }
    }

    fn apply(
        data: &mut StoredData,
        change: impl FnOnce(&mut StoredData) -> Result<(), DomainError>,
    ) {
        let before = data.clone();
        change(data).unwrap();
        reconcile(&before, data, NOW).unwrap();
    }

    fn find(data: &StoredData, id: i64) -> (i64, &Task) {
        data.columns
            .iter()
            .find_map(|c| c.tasks.iter().find(|t| t.id == id).map(|t| (c.id, t)))
            .unwrap()
    }

    #[test]
    fn position_decides_completion_and_keeps_the_first_time() {
        let mut data = board();
        apply(&mut data, |d| move_tasks(d, &[10], 2, None));
        assert_eq!(find(&data, 10).1.completed_at, Some(NOW));

        // Staying in the done column keeps the original completion time.
        let before = data.clone();
        reconcile(&before, &mut data, NOW + DAY).unwrap();
        assert_eq!(find(&data, 10).1.completed_at, Some(NOW));

        apply(&mut data, |d| move_tasks(d, &[10], 1, None));
        assert_eq!(find(&data, 10).1.completed_at, None);
    }

    #[test]
    fn moving_the_done_flag_completes_and_reopens_in_place() {
        let mut data = board();
        apply(&mut data, |d| move_tasks(d, &[10], 2, None));
        apply(&mut data, |d| set_done_column(d, Some(1)));
        assert!(data.columns[0].done && !data.columns[1].done);
        assert_eq!(find(&data, 10).0, 2);
        assert_eq!(find(&data, 10).1.completed_at, None);
        assert_eq!(find(&data, 11).1.completed_at, Some(NOW));

        apply(&mut data, |d| set_done_column(d, None));
        assert_eq!(data.done_column_id(), None);
        assert_eq!(find(&data, 11).1.completed_at, None);
        assert!(set_done_column(&mut data, Some(99)).is_err());
    }

    #[test]
    fn concurrent_flags_count_only_the_first_done_column() {
        let mut data = board();
        data.columns[0].done = true;
        data.columns[0].tasks[0].completed_at = Some(1);
        data.columns[1].tasks.push(Task {
            completed_at: Some(1),
            ..task(12)
        });
        let before = data.clone();
        reconcile(&before, &mut data, NOW).unwrap();
        assert_eq!(find(&data, 10).1.completed_at, Some(1));
        assert_eq!(find(&data, 11).1.completed_at, Some(NOW));
        assert_eq!(find(&data, 12).1.completed_at, None);
    }

    #[test]
    fn recurring_tasks_moved_into_done_continue_where_they_were() {
        let mut data = board();
        data.columns[0].tasks[1].recurrence = Some(Recurrence {
            frequency: RecurrenceFrequency::Daily,
            interval: 1,
        });
        data.columns[0].tasks[1].due_time = Some(NOW - DAY / 2);
        apply(&mut data, |d| move_tasks(d, &[11], 2, None));
        assert_eq!(find(&data, 11).0, 2);
        let next = &data.columns[0].tasks[1];
        assert_ne!(next.id, 11);
        assert_eq!(next.title, "Task 11");
        assert_eq!(next.completed_at, None);
        assert!(next.due_time.unwrap() > NOW - DAY / 2);

        // Flagging the column it already sits in does not repeat it.
        let mut data = board();
        data.columns[0].tasks[0].recurrence = data_recurrence();
        data.columns[0].tasks[0].due_time = Some(NOW);
        apply(&mut data, |d| set_done_column(d, Some(1)));
        assert_eq!(data.columns[0].tasks.len(), 2);
    }

    fn data_recurrence() -> Option<Recurrence> {
        Some(Recurrence {
            frequency: RecurrenceFrequency::Weekly,
            interval: 1,
        })
    }

    #[test]
    fn completing_moves_to_the_done_column_or_archives_without_one() {
        let mut data = board();
        apply(&mut data, |d| complete_tasks(d, &[10, 11], NOW));
        assert_eq!(
            data.columns[1]
                .tasks
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            [10, 11]
        );
        // Already done: unchanged rather than moved again.
        apply(&mut data, |d| complete_tasks(d, &[11], NOW));
        assert_eq!(data.columns[1].tasks.len(), 2);
        assert!(complete_tasks(&mut data, &[99], NOW).is_err());

        let mut data = board();
        data.columns[1].done = false;
        apply(&mut data, |d| complete_tasks(d, &[10], NOW));
        assert_eq!(data.archives.len(), 1);
        assert_eq!(data.archives[0].time, NOW);
        assert_eq!(data.archives[0].task.completed_at, None);
    }

    #[test]
    fn reopening_moves_done_and_archived_tasks_to_the_chosen_column() {
        let mut data = board();
        data.columns.push(column(3, false, vec![]));
        apply(&mut data, |d| complete_tasks(d, &[10], NOW));
        data.archives.push(Archive {
            time: NOW,
            task: task(20),
        });
        apply(&mut data, |d| reopen_tasks(d, &[10, 20, 11], 3));
        assert_eq!(
            data.columns[2]
                .tasks
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            [20, 10]
        );
        assert_eq!(find(&data, 11).0, 1);
        assert!(data.archives.is_empty());
        assert!(data
            .columns
            .iter()
            .flat_map(|c| &c.tasks)
            .all(|t| t.completed_at.is_none()));

        assert!(matches!(
            reopen_tasks(&mut data, &[10], 2),
            Err(DomainError::InvalidArgument)
        ));
        assert!(matches!(
            reopen_tasks(&mut data, &[99], 1),
            Err(DomainError::TaskNotFound)
        ));
    }
}
