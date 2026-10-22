use crate::{
    errors::DomainError,
    models::{valid_sort_rules, Column, ColumnSort, SortRule, StoredData, Task},
};
use std::collections::HashSet;

pub(crate) fn column_index(data: &StoredData, column_id: i64) -> Result<usize, DomainError> {
    data.columns
        .iter()
        .position(|column| column.id == column_id)
        .ok_or(DomainError::ColumnNotFound)
}

pub(crate) fn task_position(
    data: &StoredData,
    task_id: i64,
) -> Result<(usize, usize), DomainError> {
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

pub(crate) fn add_column(
    data: &mut StoredData,
    name: String,
    color: String,
) -> Result<i64, DomainError> {
    let id = data.allocate_column_id()?;
    data.columns.push(Column {
        id,
        name,
        color,
        sort_order: ColumnSort::Custom,
        sort_rules: Vec::new(),
        tasks: Vec::new(),
    });
    Ok(id)
}

pub(crate) fn update_column(
    data: &mut StoredData,
    column_id: i64,
    name: String,
    color: String,
    sort_rules: Vec<SortRule>,
) -> Result<(), DomainError> {
    if !valid_sort_rules(&sort_rules) {
        return Err(DomainError::InvalidArgument);
    }
    let index = column_index(data, column_id)?;
    let column = &mut data.columns[index];
    column.name = name;
    column.color = color;
    column.set_sort_rules(sort_rules);
    Ok(())
}

pub(crate) fn move_column(
    data: &mut StoredData,
    column_id: i64,
    before_column_id: Option<i64>,
) -> Result<(), DomainError> {
    if before_column_id == Some(column_id) {
        return Ok(());
    }
    let from = column_index(data, column_id)?;
    let column = data.columns.remove(from);
    let destination = match before_column_id {
        Some(id) => column_index(data, id)?,
        None => data.columns.len(),
    };
    data.columns.insert(destination, column);
    Ok(())
}

pub(crate) fn move_task(
    data: &mut StoredData,
    task_id: i64,
    to_column_id: i64,
    before_task_id: Option<i64>,
) -> Result<(), DomainError> {
    if before_task_id == Some(task_id) {
        return Ok(());
    }
    let (from_column, from_task) = task_position(data, task_id)?;
    let task = data.columns[from_column].tasks.remove(from_task);
    let to_column = column_index(data, to_column_id)?;
    let destination = match before_task_id {
        Some(id) => data.columns[to_column]
            .tasks
            .iter()
            .position(|candidate| candidate.id == id)
            .ok_or(DomainError::TaskNotFound)?,
        None => data.columns[to_column].tasks.len(),
    };
    data.columns[to_column].tasks.insert(destination, task);
    Ok(())
}

/// Batch operations act on a bounded set of distinct tasks and fail as a whole
/// when any task is missing, so a partial selection never commits.
pub(crate) const MAX_BATCH_TASKS: usize = 10_000;

pub(crate) fn unique_task_ids(task_ids: Vec<i64>) -> Result<Vec<i64>, DomainError> {
    if task_ids.is_empty() || task_ids.len() > MAX_BATCH_TASKS {
        return Err(DomainError::InvalidArgument);
    }
    let mut seen = HashSet::with_capacity(task_ids.len());
    Ok(task_ids.into_iter().filter(|id| seen.insert(*id)).collect())
}

pub(crate) fn delete_tasks(data: &mut StoredData, task_ids: &[i64]) -> Result<(), DomainError> {
    for &task_id in task_ids {
        let (column, task) = task_position(data, task_id)?;
        data.columns[column].tasks.remove(task);
    }
    Ok(())
}

/// Inserts the tasks in the given order before an unselected task in the
/// destination column, or at its end. The caller supplies the order because
/// a drag may already show tasks in positions that were never persisted.
pub(crate) fn move_tasks(
    data: &mut StoredData,
    task_ids: &[i64],
    to_column_id: i64,
    before_task_id: Option<i64>,
) -> Result<(), DomainError> {
    let to_column = column_index(data, to_column_id)?;
    if before_task_id.is_some_and(|id| task_ids.contains(&id)) {
        return Err(DomainError::InvalidArgument);
    }
    for &task_id in task_ids {
        task_position(data, task_id)?;
    }
    let mut moved = Vec::with_capacity(task_ids.len());
    for &task_id in task_ids {
        let (column, task) = task_position(data, task_id)?;
        moved.push(data.columns[column].tasks.remove(task));
    }
    let tasks = &mut data.columns[to_column].tasks;
    let destination = match before_task_id {
        Some(id) => tasks
            .iter()
            .position(|task| task.id == id)
            .ok_or(DomainError::TaskNotFound)?,
        None => tasks.len(),
    };
    tasks.splice(destination..destination, moved);
    Ok(())
}

pub(crate) fn set_tasks_pinned(
    data: &mut StoredData,
    task_ids: &[i64],
    pinned: bool,
) -> Result<(), DomainError> {
    for &task_id in task_ids {
        let (column, task) = task_position(data, task_id)?;
        data.columns[column].tasks[task].pinned = pinned;
    }
    Ok(())
}

pub(crate) fn set_tasks_priority(
    data: &mut StoredData,
    task_ids: &[i64],
    priority: Option<crate::models::TaskPriority>,
) -> Result<(), DomainError> {
    for &task_id in task_ids {
        let (column, task) = task_position(data, task_id)?;
        data.columns[column].tasks[task].priority = priority;
    }
    Ok(())
}

pub(crate) fn add_task(
    data: &mut StoredData,
    column_id: i64,
    mut task: Task,
    after_task_id: Option<i64>,
) -> Result<i64, DomainError> {
    let column = column_index(data, column_id)?;
    let destination = match after_task_id {
        Some(id) => {
            data.columns[column]
                .tasks
                .iter()
                .position(|candidate| candidate.id == id)
                .ok_or(DomainError::TaskNotFound)?
                + 1
        }
        None => data.columns[column].tasks.len(),
    };
    let id = data.allocate_task_id()?;
    task.id = id;
    for label in &task.labels {
        data.touch_label(label.clone());
    }
    data.columns[column].tasks.insert(destination, task);
    Ok(id)
}

pub(crate) fn update_task(
    data: &mut StoredData,
    task_id: i64,
    mut task: Task,
) -> Result<(), DomainError> {
    let (column, index) = task_position(data, task_id)?;
    let previous = &data.columns[column].tasks[index];
    let previous_labels = previous.labels.clone();
    // Creation time is immutable once known; legacy tasks may receive one here.
    // Legacy tasks stored their creation time in start_time, so callers that
    // omit it (plugins) must not leave a new start time read as the creation.
    if previous.created_time != 0 {
        task.created_time = previous.created_time;
    } else if task.created_time == 0 {
        task.created_time = previous.start_time;
    }
    task.id = task_id;
    for label in &task.labels {
        if !previous_labels.contains(label) {
            data.touch_label(label.clone());
        }
    }
    data.columns[column].tasks[index] = task;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(task: Task) -> StoredData {
        let mut data = StoredData::default();
        data.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            sort_rules: Vec::new(),
            tasks: vec![Task { id: 2, ..task }],
        });
        data
    }

    fn two_columns() -> StoredData {
        let task = |id| Task {
            id,
            ..Task::default()
        };
        let column = |id, tasks| Column {
            id,
            name: String::new(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            sort_rules: Vec::new(),
            tasks,
        };
        StoredData {
            columns: vec![
                column(1, vec![task(10), task(11), task(12)]),
                column(2, vec![task(20), task(21)]),
            ],
            ..StoredData::default()
        }
    }

    fn ids(data: &StoredData) -> Vec<Vec<i64>> {
        data.columns
            .iter()
            .map(|column| column.tasks.iter().map(|task| task.id).collect())
            .collect()
    }

    #[test]
    fn batch_ids_are_deduplicated_and_bounded() {
        assert_eq!(unique_task_ids(vec![3, 1, 3, 2]).unwrap(), vec![3, 1, 2]);
        assert!(matches!(
            unique_task_ids(vec![]),
            Err(DomainError::InvalidArgument)
        ));
        assert!(matches!(
            unique_task_ids(vec![1; MAX_BATCH_TASKS + 1]),
            Err(DomainError::InvalidArgument)
        ));
    }

    #[test]
    fn set_tasks_priority_sets_and_clears_across_columns() {
        use crate::models::TaskPriority;
        let mut data = two_columns();
        set_tasks_priority(&mut data, &[10, 21], Some(TaskPriority::Urgent)).unwrap();
        let priority = |data: &StoredData, column: usize, task: usize| {
            data.columns[column].tasks[task].priority
        };
        assert_eq!(priority(&data, 0, 0), Some(TaskPriority::Urgent));
        assert_eq!(priority(&data, 1, 1), Some(TaskPriority::Urgent));
        assert_eq!(priority(&data, 0, 1), None);

        set_tasks_priority(&mut data, &[10], None).unwrap();
        assert_eq!(priority(&data, 0, 0), None);
        assert!(matches!(
            set_tasks_priority(&mut data, &[99], None),
            Err(DomainError::TaskNotFound)
        ));
    }

    #[test]
    fn delete_tasks_removes_tasks_across_columns_or_nothing() {
        let mut data = two_columns();
        delete_tasks(&mut data, &[11, 20]).unwrap();
        assert_eq!(ids(&data), vec![vec![10, 12], vec![21]]);

        let mut data = two_columns();
        assert!(matches!(
            delete_tasks(&mut data, &[10, 99]),
            Err(DomainError::TaskNotFound)
        ));
    }

    #[test]
    fn move_tasks_inserts_in_requested_order() {
        let mut data = two_columns();
        move_tasks(&mut data, &[20, 12, 10], 2, None).unwrap();
        assert_eq!(ids(&data), vec![vec![11], vec![21, 20, 12, 10]]);

        let mut data = two_columns();
        move_tasks(&mut data, &[12, 10], 2, Some(21)).unwrap();
        assert_eq!(ids(&data), vec![vec![11], vec![20, 12, 10, 21]]);
        assert!(matches!(
            move_tasks(&mut data, &[10], 2, Some(10)),
            Err(DomainError::InvalidArgument)
        ));

        let mut data = two_columns();
        assert!(matches!(
            move_tasks(&mut data, &[10, 99], 2, None),
            Err(DomainError::TaskNotFound)
        ));
        assert!(matches!(
            move_tasks(&mut data, &[10], 9, None),
            Err(DomainError::ColumnNotFound)
        ));
        assert_eq!(ids(&data), ids(&two_columns()));
    }

    #[test]
    fn update_keeps_known_creation_time_and_resolves_legacy_creation() {
        let mut data = board(Task {
            created_time: 1_700_000_000_000,
            ..Task::default()
        });
        let edited = Task {
            start_time: 1_800_000_000_000,
            created_time: 5,
            ..Task::default()
        };
        update_task(&mut data, 2, edited.clone()).unwrap();
        assert_eq!(data.columns[0].tasks[0].created_time, 1_700_000_000_000);

        // A legacy start_time was the creation time; a caller without
        // created_time must not turn its new start into the creation time.
        let mut legacy = board(Task {
            start_time: 1_600_000_000_000,
            ..Task::default()
        });
        update_task(
            &mut legacy,
            2,
            Task {
                created_time: 0,
                ..edited
            },
        )
        .unwrap();
        let task = &legacy.columns[0].tasks[0];
        assert_eq!(
            (task.start_time, task.created_time),
            (1_800_000_000_000, 1_600_000_000_000)
        );
    }

    #[test]
    fn update_column_saves_valid_sort_rules_and_rejects_invalid_ones() {
        let mut data = board(Task::default());
        let rule = |field: &str| SortRule {
            field: field.into(),
            direction: crate::models::SortDirection::Asc,
        };
        let rules = vec![rule("due_time"), rule("title")];
        update_column(&mut data, 1, "Todo".into(), String::new(), rules.clone()).unwrap();
        assert_eq!(data.columns[0].sort_rules, rules);
        assert_eq!(data.columns[0].sort_order, ColumnSort::Custom);

        for invalid in [
            vec![rule("effort")],
            vec![rule("title"), rule("title")],
            vec![
                rule("due_time"),
                rule("start_time"),
                rule("created_time"),
                rule("title"),
            ],
        ] {
            assert!(matches!(
                update_column(&mut data, 1, "Todo".into(), String::new(), invalid),
                Err(DomainError::InvalidArgument)
            ));
        }
        assert_eq!(data.columns[0].sort_rules, rules);
    }
}
